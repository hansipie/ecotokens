pub mod models;
pub mod session_store;
pub mod settings;

pub use session_store::SessionStore;
pub use settings::Settings;

use std::io::Write;
use std::path::{Path, PathBuf};

pub fn git_root() -> Option<PathBuf> {
    std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| PathBuf::from(s.trim()))
}

/// Returns the default index directory: ~/.config/ecotokens/index
pub fn default_index_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| {
            eprintln!("ecotokens: warning: could not determine config dir, falling back to '.'");
            PathBuf::from(".")
        })
        .join("ecotokens")
        .join("index")
}

/// Atomically replace `path` with `contents` using a temp file in the same directory.
pub fn atomic_write(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("ecotokens");
    let tmp_path = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    let write_result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        file.write_all(contents.as_ref())?;
        file.sync_all()?;
        std::fs::rename(&tmp_path, path)
    })();

    if write_result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }

    write_result
}
