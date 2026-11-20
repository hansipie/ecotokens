//! Optional `~/.config/ecotokens/.env`: `KEY=VALUE` lines exported into the
//! process environment at startup (e.g. `TYPESAFE_API_KEY`). Variables already
//! set in the real environment always win.

use std::path::{Path, PathBuf};

pub fn env_file_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ecotokens").join(".env"))
}

/// Parse `.env` content: blank lines and `#` comments are skipped, an optional
/// `export ` prefix is accepted, and a value may be wrapped in matching single
/// or double quotes. Malformed lines are ignored.
pub fn parse(content: &str) -> Vec<(String, String)> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty()
                || key.starts_with(|c: char| c.is_ascii_digit())
                || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return None;
            }
            let value = value.trim();
            let value = match value.as_bytes() {
                [q @ (b'"' | b'\''), .., last] if q == last && value.len() >= 2 => {
                    &value[1..value.len() - 1]
                }
                _ => value,
            };
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

/// Export the entries of `path` that are not already set. Returns the names
/// that were set. A missing or unreadable file is not an error.
pub fn load_from(path: &Path) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut applied = Vec::new();
    for (key, value) in parse(&content) {
        if std::env::var_os(&key).is_none() {
            std::env::set_var(&key, value);
            applied.push(key);
        }
    }
    applied
}

/// Load the default `.env`. Call once at startup, before any thread is spawned.
pub fn load() {
    if let Some(path) = env_file_path() {
        load_from(&path);
    }
}

/// The value `key` has in the `.env` file at `path`, if any.
pub fn read_value(path: &Path, key: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    parse(&content)
        .into_iter()
        .rev()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

/// Write `key=value` into the `.env` file at `path`, keeping every other
/// line. The first line that sets `key` is replaced and any later one is
/// dropped; without one, the entry is appended.
///
/// The file holds secrets, so it is never readable by others, not even for a
/// moment: the new content goes to a temporary file created with mode 600 in
/// the same directory, which then replaces `path`. An existing file with
/// looser rights ends up at 600 too. The value never appears in an error.
pub fn set_value(path: &Path, key: &str, value: &str) -> std::io::Result<()> {
    let invalid = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidInput, m.to_string());
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(invalid("invalid variable name"));
    }
    if value.contains(['\n', '\r']) {
        return Err(invalid("the value must fit on one line"));
    }
    if value.contains('\'') && value.contains('"') {
        // `parse` strips one pair of quotes and does no unescaping.
        return Err(invalid("the value cannot contain both ' and \""));
    }
    let existing = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let content = with_value(&existing, key, value);

    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("env"),
        std::process::id()
    ));
    let result = (|| {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&tmp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)?;
        restrict(path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Owner read/write only.
#[cfg(unix)]
fn restrict(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict(_: &Path) -> std::io::Result<()> {
    Ok(())
}

/// `content` with `key` set to `value` (see [`set_value`]).
fn with_value(content: &str, key: &str, value: &str) -> String {
    let needs_quotes = value.is_empty()
        || value
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '#' | '"' | '\'' | '$' | '`' | '\\'));
    // `set_value` refuses values holding both kinds of quotes.
    let entry = if needs_quotes && !value.contains('\'') {
        format!("{key}='{value}'")
    } else if needs_quotes {
        format!("{key}=\"{value}\"")
    } else {
        format!("{key}={value}")
    };
    let sets_key = |line: &str| {
        let l = line.trim_start();
        let l = l.strip_prefix("export ").unwrap_or(l);
        l.split_once('=').is_some_and(|(k, _)| k.trim() == key)
    };
    let mut out: Vec<String> = Vec::new();
    let mut done = false;
    for line in content.lines() {
        if sets_key(line) {
            if !done {
                out.push(entry.clone());
                done = true;
            }
        } else {
            out.push(line.to_string());
        }
    }
    if !done {
        out.push(entry);
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

#[cfg(test)]
mod set_value_tests {
    use super::*;

    #[cfg(unix)]
    fn mode(p: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn creates_the_file_and_its_directory_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ecotokens").join(".env");
        set_value(&path, "TYPESAFE_API_KEY", "ts_abc").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "TYPESAFE_API_KEY=ts_abc\n"
        );
        #[cfg(unix)]
        assert_eq!(mode(&path), 0o600);
        assert_eq!(
            read_value(&path, "TYPESAFE_API_KEY").as_deref(),
            Some("ts_abc")
        );
    }

    #[test]
    fn replaces_the_key_keeps_other_lines_and_tightens_rights() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(
            &path,
            "# my keys\nOTHER=1\nexport TYPESAFE_API_KEY=old\nLAST=x\nTYPESAFE_API_KEY=dup\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        set_value(&path, "TYPESAFE_API_KEY", "new").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# my keys\nOTHER=1\nTYPESAFE_API_KEY=new\nLAST=x\n"
        );
        #[cfg(unix)]
        assert_eq!(mode(&path), 0o600, "a 644 file is tightened to 600");
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "no temporary file is left behind");
    }

    #[test]
    fn values_needing_quotes_round_trip_through_parse() {
        for v in ["a b", "x#y", "it's", "say \"hi\"", "$HOME"] {
            let content = with_value("", "K", v);
            assert_eq!(
                parse(&content),
                vec![("K".to_string(), v.to_string())],
                "{v}"
            );
        }
        let dir = tempfile::tempdir().unwrap();
        assert!(set_value(&dir.path().join(".env"), "K", "a\"b'c").is_err());
    }

    #[test]
    fn rejects_bad_names_and_multiline_values_without_echoing_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        assert!(set_value(&path, "BAD-NAME", "v").is_err());
        let err = set_value(&path, "K", "secret\nline").unwrap_err();
        assert!(!err.to_string().contains("secret"));
        assert!(!path.exists(), "nothing written on error");
    }
}
