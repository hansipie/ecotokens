use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

const CONTENT_MAX_CHARS: usize = 4096;

fn truncate_content(s: String) -> String {
    if s.chars().count() <= CONTENT_MAX_CHARS {
        s
    } else {
        let t: String = s.chars().take(CONTENT_MAX_CHARS).collect();
        t + "\n…[truncated]"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CommandFamily {
    Git,
    Cargo,
    Cpp,
    Fs,
    Markdown,
    Python,
    ConfigFile,
    Go,
    Js,
    Gh,
    Container,
    Grep,
    Aws,
    Network,
    Db,
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FilterMode {
    Filtered,
    Passthrough,
    Summarized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interception {
    pub id: String,
    pub timestamp: String,
    pub command: String,
    pub command_family: CommandFamily,
    pub git_root: Option<String>,
    pub tokens_before: u32,
    pub tokens_after: u32,
    pub savings_pct: f32,
    pub mode: FilterMode,
    pub redacted: bool,
    pub duration_ms: u32,
    #[serde(default)]
    pub content_before: Option<String>,
    #[serde(default)]
    pub content_after: Option<String>,
}

impl Interception {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command: String,
        command_family: CommandFamily,
        git_root: Option<String>,
        tokens_before: u32,
        tokens_after: u32,
        mode: FilterMode,
        redacted: bool,
        duration_ms: u32,
        content_before: Option<String>,
        content_after: Option<String>,
    ) -> Self {
        let savings_pct = if mode == FilterMode::Passthrough || tokens_before == 0 {
            0.0
        } else {
            ((1.0 - tokens_after as f64 / tokens_before as f64) * 100.0) as f32
        };
        Interception {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            command,
            command_family,
            git_root,
            tokens_before,
            tokens_after,
            savings_pct,
            mode,
            redacted,
            duration_ms,
            content_before: content_before.map(truncate_content),
            content_after: content_after.map(truncate_content),
        }
    }
}

pub fn metrics_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ecotokens").join("metrics.jsonl"))
}

/// Append one Interception as a JSONL line to `path`.
pub fn append_to(path: &std::path::Path, interception: &Interception) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(interception)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")
}

/// Read all Interceptions from the JSONL store at `path`.
pub fn read_from(path: &std::path::Path) -> std::io::Result<Vec<Interception>> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(path)?;
    let mut items = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(item) = serde_json::from_str(line) {
            items.push(item);
        }
    }
    Ok(items)
}

/// Append to default metrics path (~/.config/ecotokens/metrics.jsonl).
#[allow(dead_code)]
pub fn append(interception: &Interception) -> std::io::Result<()> {
    let path = metrics_path().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "cannot resolve config dir")
    })?;
    append_to(&path, interception)
}

/// Read from default metrics path.
#[allow(dead_code)]
pub fn read_all() -> std::io::Result<Vec<Interception>> {
    match metrics_path() {
        Some(p) => read_from(&p),
        None => Ok(vec![]),
    }
}

/// Atomically rewrite `path` with the given interceptions.
///
/// Writes to a `.tmp` sidecar first, then renames to `path` — safe against
/// interruption (Ctrl-C, crash) because the rename is atomic on the same
/// filesystem.
pub fn write_to(path: &std::path::Path, items: &[Interception]) -> std::io::Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp_path = {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        path.with_file_name(format!("{name}.tmp"))
    };

    let mut file = std::fs::File::create(&tmp_path)?;
    for item in items {
        let line = serde_json::to_string(item)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        writeln!(file, "{line}")?;
    }
    file.flush()?;
    drop(file);

    std::fs::rename(&tmp_path, path)
}
