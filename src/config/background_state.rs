use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Représente l'état persisté d'une instance watch en mode background
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundState {
    /// PID du processus watch
    pub pid: u32,
    /// Répertoire surveillé
    pub watch_path: String,
    /// Répertoire de l'index
    pub index_dir: String,
    /// Timestamp de démarrage (ISO 8601)
    pub started_at: String,
    /// Chemin du fichier log (optionnel)
    pub log_file: Option<String>,
}

impl BackgroundState {
    /// Crée un nouvel état background
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

    /// Sauvegarde l'état dans `~/.config/ecotokens/watch-bg.json`
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

    /// Charge l'état depuis `~/.config/ecotokens/watch-bg.json`
    pub fn load() -> Option<Self> {
        let config_dir = dirs::config_dir()?;
        let state_file = config_dir.join("ecotokens").join("watch-bg.json");
        let content = fs::read_to_string(&state_file).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Supprime le fichier d'état background
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

    /// Vérifie si le processus est toujours en cours d'exécution
    pub fn is_running(&self) -> bool {
        // Vérifier si le PID existe dans /proc (Unix-like)
        #[cfg(unix)]
        {
            Path::new(&format!("/proc/{}", self.pid)).exists()
        }
        #[cfg(not(unix))]
        {
            // Sur Windows, c'est moins simple, on retourne true par défaut
            true
        }
    }

    /// Arrête le processus background et nettoie le fichier d'état
    pub fn stop(&self) -> std::io::Result<()> {
        if !self.is_running() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Process {} is not running", self.pid),
            ));
        }

        #[cfg(unix)]
        {
            // Envoyer SIGTERM au processus
            let status = std::process::Command::new("kill")
                .arg("-TERM")
                .arg(self.pid.to_string())
                .status()?;

            if !status.success() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to stop process {}", self.pid),
                ));
            }
        }

        #[cfg(not(unix))]
        {
            // Sur Windows, utiliser taskkill
            let status = std::process::Command::new("taskkill")
                .arg("/PID")
                .arg(self.pid.to_string())
                .arg("/F")
                .status()?;

            if !status.success() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to stop process {}", self.pid),
                ));
            }
        }

        // Attendre un peu pour que le processus se termine
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Nettoyer le fichier d'état
        Self::remove()?;

        Ok(())
    }
}
