pub mod env_file;
pub mod session_store;
pub mod settings;

pub use session_store::SessionStore;
pub use settings::Settings;

/// Rejects a price (USD per million tokens) that is negative, NaN or infinite.
pub fn validate_price(flag: &str, value: Option<f64>) -> Result<(), String> {
    match value {
        Some(v) if !v.is_finite() || v < 0.0 => Err(format!(
            "{flag} must be a finite, non-negative number of USD per million tokens (got {v})"
        )),
        _ => Ok(()),
    }
}

/// Formats an optional price as `$<v>` or `unset`.
pub fn fmt_price(price: Option<f64>) -> String {
    price.map_or_else(|| "unset".into(), |v| format!("${v}"))
}

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
    // A per-process monotonic counter guarantees a unique temp name even when two
    // writes from the same PID land in the same nanosecond (or after 2262, where
    // `timestamp_nanos_opt` saturates to 0).
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp_path = parent.join(format!(
        ".{file_name}.{}.{}.{}.tmp",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        seq
    ));

    let write_result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        file.write_all(contents.as_ref())?;
        file.sync_all()?;
        // The temp file lives in `parent` — the same directory (and therefore the
        // same filesystem) as `path` — so this rename is atomic rather than a
        // copy-then-delete.
        std::fs::rename(&tmp_path, path)
    })();

    if write_result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }

    write_result
}
