use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EmbedProvider {
    #[default]
    None,
    #[serde(alias = "ollama")]
    Ollama {
        url: String,
    },
    LmStudio {
        url: String,
    },
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
        ModelPrice {
            input_usd_per_1m: 0.80,
            output_usd_per_1m: 4.00,
        },
    );
    m.insert(
        "claude-sonnet-4-5".into(),
        ModelPrice {
            input_usd_per_1m: 3.00,
            output_usd_per_1m: 15.00,
        },
    );
    m.insert(
        "claude-sonnet-4-6".into(),
        ModelPrice {
            input_usd_per_1m: 3.00,
            output_usd_per_1m: 15.00,
        },
    );
    m.insert(
        "claude-opus-4-6".into(),
        ModelPrice {
            input_usd_per_1m: 15.00,
            output_usd_per_1m: 75.00,
        },
    );
    // Subscription-based: no per-token cost, token savings still tracked
    m.insert(
        "github-copilot".into(),
        ModelPrice {
            input_usd_per_1m: 0.0,
            output_usd_per_1m: 0.0,
        },
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
    #[serde(default = "EmbedProvider::default")]
    pub embed_provider: EmbedProvider,
    #[serde(default)]
    pub ai_summary_enabled: bool,
    #[serde(default)]
    pub ai_summary_model: Option<String>,
    /// Ollama base URL for AI summary (defaults to "http://localhost:11434")
    #[serde(default)]
    pub ai_summary_url: Option<String>,
    /// Minimum token count to trigger AI summarization (default: 2500)
    #[serde(default = "default_ai_summary_min_tokens")]
    pub ai_summary_min_tokens: u32,
    /// Timeout in milliseconds for Ollama API calls (default: 3000)
    #[serde(default = "default_ai_summary_timeout_ms")]
    pub ai_summary_timeout_ms: u64,
    /// Start watch automatically on each Claude Code session (default: false)
    #[serde(default)]
    pub auto_watch: bool,
    /// Depth for caller/callee trace in PostToolUse enrichment (default: 1)
    #[serde(default = "default_post_hook_depth")]
    pub post_hook_depth: u32,
    /// Apply word abbreviations to narrative text/logs/messages (default: false)
    #[serde(default)]
    pub abbreviations_enabled: bool,
    /// Extra word→abbreviation pairs that override/extend the built-in dictionary
    #[serde(skip_serializing, skip_deserializing, default)]
    pub abbreviations_custom: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct LegacySettingsFile {
    #[serde(flatten)]
    settings: Settings,
    #[serde(default)]
    abbreviations_custom: HashMap<String, String>,
}

fn default_threshold_lines() -> u32 {
    500
}
fn default_threshold_bytes() -> u32 {
    51200
}
fn default_true() -> bool {
    true
}
fn default_model() -> String {
    "claude-sonnet-4-6".into()
}
fn default_ai_summary_min_tokens() -> u32 {
    2500
}
fn default_ai_summary_timeout_ms() -> u64 {
    3000
}
fn default_post_hook_depth() -> u32 {
    1
}

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
            ai_summary_enabled: false,
            ai_summary_model: None,
            ai_summary_url: None,
            ai_summary_min_tokens: 2500,
            ai_summary_timeout_ms: 3000,
            auto_watch: false,
            post_hook_depth: 1,
            abbreviations_enabled: false,
            abbreviations_custom: HashMap::new(),
        }
    }
}

impl Settings {
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("ecotokens").join("config.json"))
    }

    fn abbreviations_path_for(config_path: &Path) -> PathBuf {
        config_path
            .parent()
            .map(|parent| parent.join("abbreviations.json"))
            .unwrap_or_else(|| PathBuf::from("abbreviations.json"))
    }

    fn load_legacy_config(path: &Path) -> LegacySettingsFile {
        let Ok(data) = std::fs::read_to_string(path) else {
            return LegacySettingsFile::default();
        };
        serde_json::from_str(&data).unwrap_or_default()
    }

    fn load_abbreviations(path: &Path) -> Option<HashMap<String, String>> {
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    fn load_from_paths(config_path: &Path, abbreviations_path: &Path) -> Self {
        let legacy = Self::load_legacy_config(config_path);
        let mut settings = legacy.settings;
        settings.abbreviations_custom =
            Self::load_abbreviations(abbreviations_path).unwrap_or(legacy.abbreviations_custom);
        settings
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Settings::default();
        };
        let abbreviations_path = Self::abbreviations_path_for(&path);
        Self::load_from_paths(&path, &abbreviations_path)
    }

    fn save_abbreviations(
        abbreviations_path: &Path,
        abbreviations_custom: &HashMap<String, String>,
    ) -> std::io::Result<()> {
        if abbreviations_custom.is_empty() {
            return match std::fs::remove_file(abbreviations_path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            };
        }

        if let Some(parent) = abbreviations_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(abbreviations_custom)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(abbreviations_path, json)
    }

    fn save_to_paths(&self, config_path: &Path, abbreviations_path: &Path) -> std::io::Result<()> {
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(config_path, json)?;
        Self::save_abbreviations(abbreviations_path, &self.abbreviations_custom)
    }

    #[allow(dead_code)]
    pub fn save(&self) -> std::io::Result<()> {
        let config_path = Self::config_path().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "cannot resolve config dir")
        })?;
        let abbreviations_path = Self::abbreviations_path_for(&config_path);
        self.save_to_paths(&config_path, &abbreviations_path)
    }

    #[allow(dead_code)]
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
