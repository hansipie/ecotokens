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
