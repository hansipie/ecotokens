use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EmbedProvider {
    None,
    Ollama { url: String },
    LmStudio { url: String },
}

impl Default for EmbedProvider {
    fn default() -> Self {
        EmbedProvider::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPrice {
    pub input_usd_per_1m: f64,
    pub output_usd_per_1m: f64,
}

fn default_model_pricing() -> HashMap<String, ModelPrice> {
    let mut m = HashMap::new();
    m.insert(
        "claude-haiku-4-5".into(),
        ModelPrice { input_usd_per_1m: 0.80, output_usd_per_1m: 4.00 },
    );
    m.insert(
        "claude-sonnet-4-5".into(),
        ModelPrice { input_usd_per_1m: 3.00, output_usd_per_1m: 15.00 },
    );
    m.insert(
        "claude-sonnet-4-6".into(),
        ModelPrice { input_usd_per_1m: 3.00, output_usd_per_1m: 15.00 },
    );
    m.insert(
        "claude-opus-4-6".into(),
        ModelPrice { input_usd_per_1m: 15.00, output_usd_per_1m: 75.00 },
    );
    m
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub exclusions: Vec<String>,
    #[serde(default = "default_threshold_lines")]
    pub summary_threshold_lines: u32,
    #[serde(default = "default_threshold_bytes")]
    pub summary_threshold_bytes: u32,
    #[serde(default = "default_true")]
    pub masking_enabled: bool,
    #[serde(default)]
    pub exact_token_counting: bool,
    #[serde(default)]
    pub debug: bool,
    #[serde(default = "default_model")]
    pub default_model: String,
    #[serde(default = "default_model_pricing")]
    pub model_pricing: HashMap<String, ModelPrice>,
    #[serde(default)]
    pub embed_provider: EmbedProvider,
}

fn default_threshold_lines() -> u32 { 500 }
fn default_threshold_bytes() -> u32 { 51200 }
fn default_true() -> bool { true }
fn default_model() -> String { "claude-sonnet-4-6".into() }

impl Default for Settings {
    fn default() -> Self {
        Settings {
            exclusions: vec![],
            summary_threshold_lines: 500,
            summary_threshold_bytes: 51200,
            masking_enabled: true,
            exact_token_counting: false,
            debug: false,
            default_model: "claude-sonnet-4-6".into(),
            model_pricing: default_model_pricing(),
            embed_provider: EmbedProvider::None,
        }
    }
}

impl Settings {
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("ecotokens").join("config.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Settings::default();
        };
        let Ok(data) = std::fs::read_to_string(&path) else {
            return Settings::default();
        };
        serde_json::from_str(&data).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "cannot resolve config dir")
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&path, json)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(10..=10000).contains(&self.summary_threshold_lines) {
            return Err(format!(
                "summary_threshold_lines must be in [10, 10000], got {}",
                self.summary_threshold_lines
            ));
        }
        if !(1024..=1048576).contains(&self.summary_threshold_bytes) {
            return Err(format!(
                "summary_threshold_bytes must be in [1024, 1048576], got {}",
                self.summary_threshold_bytes
            ));
        }
        Ok(())
    }
}
