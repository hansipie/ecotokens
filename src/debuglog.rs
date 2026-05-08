use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

pub struct DebugLogger {
    enabled: bool,
    path: PathBuf,
}

impl DebugLogger {
    pub fn new(enabled: bool) -> Self {
        let path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ecotokens")
            .join("debug.log");
        DebugLogger { enabled, path }
    }

    pub fn log(&self, uid: &str, cmd: &str, phase: &str, data: &serde_json::Value) {
        if !self.enabled {
            return;
        }
        let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
        let entry = serde_json::json!({
            "ts": ts,
            "uid": uid,
            "cmd": cmd,
            "phase": phase,
            "data": data,
        });
        let mut line = serde_json::to_string(&entry).unwrap_or_default();
        line.push('\n');
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.path)
        {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

pub fn gen_uid() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}
