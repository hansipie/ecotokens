use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// State for one watched directory: number of active Claude sessions + watcher PID.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatcherEntry {
    pub sessions: u32,
    pub watcher_pid: Option<u32>,
    pub log_file: Option<String>,
    pub started_at: Option<String>,
}

/// Registry of per-path watchers and session counts.
/// Persisted to `~/.config/ecotokens/sessions.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionStore(pub HashMap<String, WatcherEntry>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionWatchDecision {
    pub watch_path: String,
    pub needs_watcher: bool,
    pub reused_existing_watcher: bool,
}

impl SessionStore {
    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("ecotokens").join("sessions.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(data) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&data).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "cannot resolve config dir")
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(&self.0).unwrap())
    }

    /// Remove stale entries: watcher is dead and no sessions reference them.
    pub fn cleanup_dead(&mut self) {
        for entry in self.0.values_mut() {
            if let Some(pid) = entry.watcher_pid {
                if !is_pid_running(pid) {
                    entry.watcher_pid = None;
                }
            }
        }
        self.0
            .retain(|_, e| e.sessions > 0 || e.watcher_pid.is_some());
    }

    /// Increment session count for `path`.
    /// Returns `true` if a watcher needs to be started (none running for this path).
    pub fn increment(&mut self, path: &str) -> bool {
        let entry = self.0.entry(path.to_string()).or_default();
        entry.sessions += 1;
        entry
            .watcher_pid
            .map(|pid| !is_pid_running(pid))
            .unwrap_or(true)
    }

    fn resolve_watch_path<'a>(&'a self, path: &'a str) -> &'a str {
        let candidate = Path::new(path);
        self.0
            .iter()
            .filter(|(stored_path, entry)| {
                (entry.sessions > 0 || entry.watcher_pid.is_some())
                    && candidate.starts_with(Path::new(stored_path))
            })
            .map(|(stored_path, entry)| {
                let is_live = entry.watcher_pid.map(is_pid_running).unwrap_or(false);
                let depth = Path::new(stored_path).components().count();
                (stored_path.as_str(), is_live, depth)
            })
            .max_by_key(|(_, is_live, depth)| (*is_live, *depth))
            .map(|(stored_path, _, _)| stored_path)
            .unwrap_or(path)
    }

    pub fn increment_for_session(&mut self, path: &str) -> SessionWatchDecision {
        let watch_path = self.resolve_watch_path(path).to_string();
        let reused_existing_watcher = watch_path != path
            && self
                .0
                .get(&watch_path)
                .and_then(|entry| entry.watcher_pid)
                .map(is_pid_running)
                .unwrap_or(false);
        let needs_watcher = self.increment(&watch_path);

        SessionWatchDecision {
            watch_path,
            needs_watcher,
            reused_existing_watcher,
        }
    }

    /// Decrement session count for `path`.
    /// Returns `Some(pid)` if the watcher should be stopped (last session closed).
    pub fn decrement(&mut self, path: &str) -> Option<u32> {
        let entry = self.0.get_mut(path)?;
        if entry.sessions > 0 {
            entry.sessions -= 1;
        }
        if entry.sessions == 0 {
            let pid = entry.watcher_pid.take();
            self.0.remove(path);
            return pid;
        }
        None
    }

    pub fn decrement_for_session(&mut self, path: &str) -> Option<u32> {
        let watch_path = self.resolve_watch_path(path).to_string();
        self.decrement(&watch_path)
    }

    /// Called from inside the daemon after `daemonize().start()` succeeds.
    pub fn register_watcher(&mut self, path: &str, pid: u32, log_file: Option<String>) {
        let entry = self.0.entry(path.to_string()).or_default();
        entry.watcher_pid = Some(pid);
        entry.log_file = log_file;
        entry.started_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Return the log file path for a watched directory, if any.
    pub fn log_file_for(&self, path: &str) -> Option<String> {
        self.0.get(path)?.log_file.clone()
    }

    /// Clear the watcher PID for a path (called when the watcher exits).
    /// Deletes the log file and removes the entry if no sessions reference it.
    pub fn clear_watcher(&mut self, path: &str) {
        if let Some(entry) = self.0.get_mut(path) {
            if let Some(ref log) = entry.log_file {
                let _ = std::fs::remove_file(log);
            }
            entry.watcher_pid = None;
            entry.log_file = None;
            if entry.sessions == 0 {
                self.0.remove(path);
            }
        }
    }

    /// Stop watcher for a specific path. Returns its PID if one was running.
    pub fn stop_watcher(&mut self, path: &str) -> Option<u32> {
        let entry = self.0.get_mut(path)?;
        if let Some(ref log) = entry.log_file {
            let _ = std::fs::remove_file(log);
        }
        let pid = entry.watcher_pid.take()?;
        entry.sessions = 0;
        self.0.remove(path);
        Some(pid)
    }

    /// Stop all watchers. Returns the list of PIDs to kill.
    pub fn stop_all(&mut self) -> Vec<u32> {
        for entry in self.0.values() {
            if let Some(ref log) = entry.log_file {
                let _ = std::fs::remove_file(log);
            }
        }
        let pids: Vec<u32> = self
            .0
            .values_mut()
            .filter_map(|e| e.watcher_pid.take())
            .collect();
        self.0.clear();
        pids
    }
}

pub fn is_pid_running(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map_or(true, |s| s.success())
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}
