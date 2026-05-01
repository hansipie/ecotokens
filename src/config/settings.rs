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
        #[serde(default = "default_ollama_model")]
        model: String,
    },
    LmStudio {
        url: String,
        #[serde(default = "default_lmstudio_model")]
        model: String,
    },
}

fn default_ollama_model() -> String {
    "nomic-embed-text".to_string()
}
fn default_lmstudio_model() -> String {
    "nomic-embed-text-v1.5".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPrice {
    pub input_usd_per_1m: f64,
    pub output_usd_per_1m: f64,
}

fn default_model_pricing() -> HashMap<String, ModelPrice> {
    let mut m = HashMap::new();

    // --- Anthropic Claude ---
    m.insert(
        "claude-haiku-4-5".into(),
        ModelPrice {
            input_usd_per_1m: 1.00,
            output_usd_per_1m: 5.00,
        },
    );
    m.insert(
        "claude-haiku-4-5-20251001".into(),
        ModelPrice {
            input_usd_per_1m: 1.00,
            output_usd_per_1m: 5.00,
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
    m.insert(
        "claude-opus-4-7".into(),
        ModelPrice {
            input_usd_per_1m: 5.00,
            output_usd_per_1m: 25.00,
        },
    );

    // --- OpenAI GPT ---
    m.insert(
        "gpt-4o".into(),
        ModelPrice {
            input_usd_per_1m: 2.50,
            output_usd_per_1m: 10.00,
        },
    );
    m.insert(
        "gpt-4o-mini".into(),
        ModelPrice {
            input_usd_per_1m: 0.15,
            output_usd_per_1m: 0.60,
        },
    );
    m.insert(
        "gpt-4.1".into(),
        ModelPrice {
            input_usd_per_1m: 2.00,
            output_usd_per_1m: 8.00,
        },
    );
    m.insert(
        "gpt-4.1-mini".into(),
        ModelPrice {
            input_usd_per_1m: 0.40,
            output_usd_per_1m: 1.60,
        },
    );
    m.insert(
        "gpt-4.1-nano".into(),
        ModelPrice {
            input_usd_per_1m: 0.10,
            output_usd_per_1m: 0.40,
        },
    );
    m.insert(
        "gpt-5".into(),
        ModelPrice {
            input_usd_per_1m: 1.25,
            output_usd_per_1m: 10.00,
        },
    );
    m.insert(
        "gpt-5-mini".into(),
        ModelPrice {
            input_usd_per_1m: 0.25,
            output_usd_per_1m: 2.00,
        },
    );
    m.insert(
        "gpt-5-nano".into(),
        ModelPrice {
            input_usd_per_1m: 0.05,
            output_usd_per_1m: 0.40,
        },
    );
    // --- OpenAI Reasoning ---
    m.insert(
        "o1".into(),
        ModelPrice {
            input_usd_per_1m: 15.00,
            output_usd_per_1m: 60.00,
        },
    );
    m.insert(
        "o3".into(),
        ModelPrice {
            input_usd_per_1m: 2.00,
            output_usd_per_1m: 8.00,
        },
    );
    m.insert(
        "o4-mini".into(),
        ModelPrice {
            input_usd_per_1m: 1.10,
            output_usd_per_1m: 4.40,
        },
    );

    // --- Google Gemini ---
    m.insert(
        "gemini-2.5-pro".into(),
        ModelPrice {
            input_usd_per_1m: 1.25,
            output_usd_per_1m: 10.00,
        },
    );
    m.insert(
        "gemini-2.5-flash".into(),
        ModelPrice {
            input_usd_per_1m: 0.30,
            output_usd_per_1m: 2.50,
        },
    );
    m.insert(
        "gemini-2.5-flash-lite".into(),
        ModelPrice {
            input_usd_per_1m: 0.10,
            output_usd_per_1m: 0.40,
        },
    );
    m.insert(
        "gemini-2.0-flash".into(),
        ModelPrice {
            input_usd_per_1m: 0.10,
            output_usd_per_1m: 0.40,
        },
    );

    // --- DeepSeek ---
    m.insert(
        "deepseek-chat".into(),
        ModelPrice {
            input_usd_per_1m: 1.74,
            output_usd_per_1m: 3.48,
        },
    );
    m.insert(
        "deepseek-v3".into(),
        ModelPrice {
            input_usd_per_1m: 0.252,
            output_usd_per_1m: 0.378,
        },
    );

    // --- Mistral ---
    m.insert(
        "mistral-large".into(),
        ModelPrice {
            input_usd_per_1m: 0.50,
            output_usd_per_1m: 1.50,
        },
    );
    m.insert(
        "mistral-small".into(),
        ModelPrice {
            input_usd_per_1m: 0.15,
            output_usd_per_1m: 0.60,
        },
    );

    // --- Meta Llama ---
    m.insert(
        "llama-4-maverick".into(),
        ModelPrice {
            input_usd_per_1m: 0.15,
            output_usd_per_1m: 0.60,
        },
    );
    m.insert(
        "llama-4-scout".into(),
        ModelPrice {
            input_usd_per_1m: 0.08,
            output_usd_per_1m: 0.30,
        },
    );
    m.insert(
        "llama-3.3-70b-instruct".into(),
        ModelPrice {
            input_usd_per_1m: 0.10,
            output_usd_per_1m: 0.32,
        },
    );

    // --- Alibaba Qwen ---
    m.insert(
        "qwen3.6-max".into(),
        ModelPrice {
            input_usd_per_1m: 1.30,
            output_usd_per_1m: 7.80,
        },
    );
    m.insert(
        "qwen3.6-plus".into(),
        ModelPrice {
            input_usd_per_1m: 0.50,
            output_usd_per_1m: 3.00,
        },
    );
    m.insert(
        "qwen3.6-flash".into(),
        ModelPrice {
            input_usd_per_1m: 0.25,
            output_usd_per_1m: 1.50,
        },
    );
    m.insert(
        "qwen3.5-plus".into(),
        ModelPrice {
            input_usd_per_1m: 0.40,
            output_usd_per_1m: 2.40,
        },
    );
    m.insert(
        "qwen3.5-flash".into(),
        ModelPrice {
            input_usd_per_1m: 0.10,
            output_usd_per_1m: 0.40,
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
        match serde_json::from_str(&data) {
            Ok(parsed) => parsed,
            Err(e) => {
                eprintln!(
                    "ecotokens: warning: failed to parse {} ({e}); using default settings",
                    path.display()
                );
                LegacySettingsFile::default()
            }
        }
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
        for (k, v) in default_model_pricing() {
            settings.model_pricing.entry(k).or_insert(v);
        }
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

    pub fn save(&self) -> std::io::Result<()> {
        let config_path = Self::config_path().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "cannot resolve config dir")
        })?;
        let abbreviations_path = Self::abbreviations_path_for(&config_path);
        self.save_to_paths(&config_path, &abbreviations_path)
    }

    // Exposed for callers that want to validate settings explicitly
    // (tests, tooling, or future CLI checks) without enforcing it on load.
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
