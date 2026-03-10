use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Persisted state for a background watch process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundState {
    /// PID of the watch process
    pub pid: u32,
    /// Directory being watched
    pub watch_path: String,
    /// Index directory
    pub index_dir: String,
    /// Start timestamp (ISO 8601)
    pub started_at: String,
    /// Path to the log file (optional)
    pub log_file: Option<String>,
}

impl BackgroundState {
    /// Create a new background state for the current process.
    pub fn new(
        watch_path: impl AsRef<Path>,
        index_dir: impl AsRef<Path>,
        log_file: Option<String>,
    ) -> Self {
        Self {
            pid: std::process::id(),
            watch_path: watch_path.as_ref().to_string_lossy().to_string(),
            index_dir: index_dir.as_ref().to_string_lossy().to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
            log_file,
        }
    }

    /// Persist state to `~/.config/ecotokens/watch-bg.json`.
    pub fn save(&self) -> std::io::Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ecotokens");
        fs::create_dir_all(&config_dir)?;

        let state_file = config_dir.join("watch-bg.json");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(&state_file, json)?;
        Ok(state_file)
    }

    /// Load state from `~/.config/ecotokens/watch-bg.json`.
    pub fn load() -> Option<Self> {
        let config_dir = dirs::config_dir()?;
        let state_file = config_dir.join("ecotokens").join("watch-bg.json");
        let content = fs::read_to_string(&state_file).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Remove the background state file.
    pub fn remove() -> std::io::Result<()> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ecotokens");
        let state_file = config_dir.join("watch-bg.json");
        if state_file.exists() {
            fs::remove_file(state_file)?;
        }
        Ok(())
    }

    /// Returns true if the process is still running.
    pub fn is_running(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            Path::new(&format!("/proc/{}", self.pid)).exists()
        }
        #[cfg(not(target_os = "linux"))]
        {
            true
        }
    }

    /// Send SIGTERM to the background process and clean up state.
    pub fn stop(&self) -> std::io::Result<()> {
        if !self.is_running() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Process {} is not running", self.pid),
            ));
        }

        #[cfg(unix)]
        {
            let status = std::process::Command::new("kill")
                .arg("-TERM")
                .arg(self.pid.to_string())
                .status()?;

            if !status.success() {
                return Err(std::io::Error::other(
                    format!("Failed to stop process {}", self.pid),
                ));
            }
        }

        #[cfg(not(unix))]
        {
            let status = std::process::Command::new("taskkill")
                .arg("/PID")
                .arg(self.pid.to_string())
                .arg("/F")
                .status()?;

            if !status.success() {
                return Err(std::io::Error::other(
                    format!("Failed to stop process {}", self.pid),
                ));
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
        Self::remove()?;
        Ok(())
    }
}
