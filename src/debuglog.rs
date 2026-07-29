use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

pub struct DebugLogger {
    enabled: bool,
    path: PathBuf,
}

/// Recursively mask every string in a JSON value.
///
/// The debug log records raw hook payloads (`tool_input`/`tool_response`), which
/// carry file contents and command output verbatim — and debug logs are exactly
/// what users attach to bug reports. Masking happens per string rather than on
/// the serialised line: several patterns end in `[^\s\n]+`, which on compact
/// JSON would run past the closing quote and swallow neighbouring fields,
/// corrupting the structure.
fn mask_json(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(s) => Value::String(crate::masking::mask(s).0),
        Value::Array(items) => Value::Array(items.iter().map(mask_json).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), mask_json(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

impl DebugLogger {
    pub fn new(enabled: bool) -> Self {
        let path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ecotokens")
            .join("debug.log");
        DebugLogger { enabled, path }
    }

    /// Log to an explicit path so tests never touch the real config directory.
    #[cfg(test)]
    fn with_path(enabled: bool, path: PathBuf) -> Self {
        DebugLogger { enabled, path }
    }

    pub fn log(&self, uid: &str, cmd: &str, phase: &str, data: &serde_json::Value) {
        if !self.enabled {
            return;
        }
        let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
        let entry = serde_json::json!({
            "ts": ts,
            "uid": uid,
            "cmd": cmd,
            "phase": phase,
            "data": mask_json(data),
        });
        let mut line = serde_json::to_string(&entry).unwrap_or_default();
        line.push('\n');
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Cap growth: once the log exceeds MAX_LOG_BYTES, rotate it to `.log.1`
        // (keeping one generation) so a busy `debuglog=true` session cannot fill
        // the disk without bound.
        const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;
        if let Ok(meta) = std::fs::metadata(&self.path) {
            if meta.len() > MAX_LOG_BYTES {
                let _ = std::fs::rename(&self.path, self.path.with_extension("log.1"));
            } else {
                // Tighten a log created before `mode(0o600)` was applied below;
                // otherwise it keeps the umask default (typically world-readable)
                // for the rest of its life.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if meta.permissions().mode() & 0o077 != 0 {
                        let _ = std::fs::set_permissions(
                            &self.path,
                            std::fs::Permissions::from_mode(0o600),
                        );
                    }
                }
            }
        }
        let mut opts = OpenOptions::new();
        opts.append(true).create(true);
        // This file records raw hook payloads; the umask default would leave it
        // readable by every local user. `mode` only applies on creation, which is
        // why a pre-existing log is tightened above.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        if let Ok(mut file) = opts.open(&self.path) {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

pub fn gen_uid() -> String {
    // `.chars().take(8)` rather than a byte slice `[..8]`, which would panic if
    // the format string were ever shorter than 8 bytes.
    uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AWS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";

    #[test]
    fn mask_json_redacts_secrets_in_nested_strings() {
        let value = serde_json::json!({
            "tool_response": { "file": { "content": format!("key = \"{AWS_KEY}\"") } },
            "matches": [format!("config.py:1:{AWS_KEY}")],
            "depth": 3,
        });

        let masked = mask_json(&value);
        let rendered = serde_json::to_string(&masked).unwrap();

        assert!(!rendered.contains(AWS_KEY), "secret survived: {rendered}");
        // Structure and non-string values must be preserved.
        assert_eq!(masked["depth"], 3);
        assert!(masked["tool_response"]["file"]["content"].is_string());
        assert!(masked["matches"].is_array());
    }

    #[test]
    fn logged_payload_is_masked_and_stays_valid_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("debug.log");
        let logger = DebugLogger::with_path(true, path.clone());

        // Masking per-string rather than on the serialised line matters here:
        // patterns ending in `[^\s\n]+` would otherwise run past the closing
        // quote of a compact JSON value and swallow the following fields.
        logger.log(
            "uid1",
            "hook-post",
            "input",
            &serde_json::json!({ "content": format!("PASSWORD={AWS_KEY}"), "after": "kept" }),
        );

        let written = std::fs::read_to_string(&path).unwrap();
        assert!(!written.contains(AWS_KEY), "secret leaked: {written}");

        let entry: serde_json::Value = serde_json::from_str(written.trim()).expect("valid JSON");
        assert_eq!(entry["uid"], "uid1");
        assert_eq!(
            entry["data"]["after"], "kept",
            "masking must not corrupt neighbouring fields"
        );
    }

    #[cfg(unix)]
    #[test]
    fn log_file_is_created_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("debug.log");
        DebugLogger::with_path(true, path.clone()).log(
            "uid1",
            "hook-post",
            "input",
            &serde_json::json!({ "content": "hello" }),
        );

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "debug log must not be group/world readable"
        );
    }

    #[cfg(unix)]
    #[test]
    fn pre_existing_loose_permissions_are_tightened() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("debug.log");
        std::fs::write(&path, "{}\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        DebugLogger::with_path(true, path.clone()).log(
            "uid1",
            "hook-post",
            "input",
            &serde_json::json!({ "content": "hello" }),
        );

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "an existing world-readable log must be tightened"
        );
    }

    #[test]
    fn disabled_logger_writes_nothing() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("debug.log");
        DebugLogger::with_path(false, path.clone()).log(
            "uid1",
            "hook-post",
            "input",
            &serde_json::json!({ "content": "hello" }),
        );
        assert!(!path.exists());
    }
}
