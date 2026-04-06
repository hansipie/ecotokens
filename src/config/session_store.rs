use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

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
