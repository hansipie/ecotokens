use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Short TTL for the per-directory project-root cache — long enough to spare a
/// long-running process (daemon/watch) repeated `git` spawns, short enough that
/// a repo appearing/moving is picked up quickly.
#[cfg_attr(test, allow(dead_code))]
const CACHE_TTL: Duration = Duration::from_secs(5);

#[allow(clippy::type_complexity)]
#[cfg_attr(test, allow(dead_code))]
fn cache() -> &'static Mutex<HashMap<PathBuf, (Option<String>, Instant)>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, (Option<String>, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg_attr(test, allow(dead_code))]
pub fn project_root_for_cwd(dir: &Path) -> Option<String> {
    if let Ok(guard) = cache().lock() {
        if let Some((val, at)) = guard.get(dir) {
            if at.elapsed() < CACHE_TTL {
                return val.clone();
            }
        }
    }

    let result = compute_project_root(dir);

    if let Ok(mut guard) = cache().lock() {
        guard.insert(dir.to_path_buf(), (result.clone(), Instant::now()));
    }
    result
}

#[cfg_attr(test, allow(dead_code))]
fn compute_project_root(dir: &Path) -> Option<String> {
    let git_root = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()
        .ok()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        });

    match git_root {
        Some(root) => Some(root),
        None if is_temporary_path(dir) => None,
        None => Some(dir.to_string_lossy().to_string()),
    }
}

#[cfg_attr(test, allow(dead_code))]
fn is_temporary_path(path: &std::path::Path) -> bool {
    let temp_dir = std::env::temp_dir();
    if path.starts_with(&temp_dir) {
        return true;
    }

    match (path.canonicalize(), temp_dir.canonicalize()) {
        (Ok(path), Ok(temp_dir)) => path.starts_with(temp_dir),
        _ => false,
    }
}
