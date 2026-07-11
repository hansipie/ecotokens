use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

pub struct DebugLogger {
    enabled: bool,
    path: PathBuf,
}

impl DebugLogger {
    pub fn new(enabled: bool) -> Self {
        let path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ecotokens")
            .join("debug.log");
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
            "data": data,
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
        if std::fs::metadata(&self.path)
            .map(|m| m.len() > MAX_LOG_BYTES)
            .unwrap_or(false)
        {
            let _ = std::fs::rename(&self.path, self.path.with_extension("log.1"));
        }
        if let Ok(mut file) = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.path)
        {
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
