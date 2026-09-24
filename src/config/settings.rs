use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum EmbedProvider {
    /// Built-in embedding via candle (zero-config, downloads model on first use).
    Candle {
        #[serde(default = "default_candle_model")]
        model: String,
    },
    /// Disabled — embed_text returns None, search falls back to BM25 only.
    None,
    /// Ollama HTTP embedding API (e.g. qwen3-embedding:latest).
    Ollama { url: String, model: String },
    /// Catch-all for legacy configs (ollama, lm_studio) — migrated to Candle at load time.
    Legacy,
}

impl<'de> serde::Deserialize<'de> for EmbedProvider {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Deserialize as a raw JSON value to handle both formats:
        //   new:  {"type": "candle", "model": "..."}
        //   old:  {"ollama": {"url": "...", "model": "..."}}  ← externally-tagged (pre-0.19)
        let value = serde_json::Value::deserialize(deserializer)?;
        match value.get("type").and_then(|v| v.as_str()) {
            Some("candle") => {
                let model = value
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("sentence-transformers/all-MiniLM-L6-v2")
                    .to_string();
                Ok(EmbedProvider::Candle { model })
            }
            Some("none") => Ok(EmbedProvider::None),
            Some("ollama") => {
                let url = value
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("http://localhost:11434")
                    .to_string();
                let model = value
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("qwen3-embedding:latest")
                    .to_string();
                Ok(EmbedProvider::Ollama { url, model })
            }
            // Unknown type tag or missing type field (old externally-tagged format) → Legacy
            _ => Ok(EmbedProvider::Legacy),
        }
    }
}

impl Default for EmbedProvider {
    fn default() -> Self {
        EmbedProvider::Candle {
            model: default_candle_model(),
        }
    }
}

fn default_candle_model() -> String {
    "sentence-transformers/all-MiniLM-L6-v2".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPrice {
    pub input_usd_per_1m: f64,
    pub output_usd_per_1m: f64,
}

fn default_model_pricing() -> HashMap<String, ModelPrice> {
    super::models::build_pricing_map()
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
    #[serde(
        skip_serializing,
        skip_deserializing,
        default = "default_model_pricing"
    )]
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
    /// Write command input/output to ~/.config/ecotokens/debug.log (default: false)
    #[serde(default)]
    pub debuglog: bool,

    // ── Local text rewrite (010-local-text-rewrite) ─────────────────────────
    /// Model used for rewrite transformations. Falls back to `ai_summary_model`,
    /// then a built-in default, at call time.
    #[serde(default)]
    pub rewrite_model: Option<String>,
    /// Ollama base URL for rewrite (defaults to "http://localhost:11434"). Must
    /// resolve to the local machine (FR-019).
    #[serde(default)]
    pub rewrite_url: Option<String>,
    /// Whole-operation timeout in milliseconds, including every chunk of a
    /// long document (FR-018).
    #[serde(default = "default_rewrite_timeout_ms")]
    pub rewrite_timeout_ms: u64,
    /// Assumed context window of the local model, in tokens. Chunking reserves
    /// 25% for prompt overhead and response headroom.
    #[serde(default = "default_rewrite_context_tokens")]
    pub rewrite_context_tokens: u32,
    /// A response shorter than this fraction of the input is treated as
    /// truncation and triggers fallback (FR-011). Must be in (0.0, 1.0).
    #[serde(default = "default_rewrite_truncation_ratio")]
    pub rewrite_truncation_ratio: f32,
    /// Save a diff of original vs transformed text. Off by default (FR-021).
    #[serde(default)]
    pub rewrite_save_diff: bool,
    /// Directory receiving diffs. Defaults to the OS temp directory (FR-022).
    #[serde(default)]
    pub rewrite_diff_dir: Option<PathBuf>,
    /// Maximum diffs retained; oldest pruned first. `0` disables pruning
    /// (FR-027).
    #[serde(default = "default_rewrite_diff_retention")]
    pub rewrite_diff_retention: u32,
    /// Enable automatic prose transformation in the interception pipeline.
    /// Off by default — the interception path is otherwise untouched
    /// (FR-033, FR-034).
    #[serde(default)]
    pub rewrite_auto_enabled: bool,
    /// Content below this token count passes through the automatic stage with
    /// no model call (FR-036).
    #[serde(default = "default_rewrite_auto_min_tokens")]
    pub rewrite_auto_min_tokens: u32,
    /// Interactive budget for the automatic stage, stricter than
    /// `rewrite_timeout_ms` (FR-037).
    #[serde(default = "default_rewrite_auto_timeout_ms")]
    pub rewrite_auto_timeout_ms: u64,
    /// Mode used by the automatic stage. Required when `rewrite_auto_enabled`
    /// is true (FR-033).
    #[serde(default)]
    pub rewrite_auto_mode: Option<String>,
    /// Target used by the automatic stage, when `rewrite_auto_mode` requires
    /// one (FR-033).
    #[serde(default)]
    pub rewrite_auto_target: Option<String>,

    // ── TypeSafe Jev judgments (optional, heuristics remain the fallback) ──
    /// Use Jev for content gating, rewrite verification, and (with
    /// `jev_line_select_enabled`) generic-filter line selection. Requires the
    /// `TYPESAFE_API_KEY` environment variable. Sends masked excerpts to the
    /// TypeSafe API. Off by default.
    #[serde(default)]
    pub jev_enabled: bool,
    /// Jev endpoint (defaults to `https://api.typesafe.ai/v1/systemone`). Must
    /// use https.
    #[serde(default)]
    pub jev_url: Option<String>,
    /// Per-request timeout for Jev calls, in milliseconds.
    #[serde(default = "default_jev_timeout_ms")]
    pub jev_timeout_ms: u64,
    /// Upper bound on the characters of text sent in one Jev request.
    #[serde(default = "default_jev_max_input_chars")]
    pub jev_max_input_chars: usize,
    /// Let Jev pick the error-bearing lines kept by the generic head+tail
    /// filter. Separate opt-in: it runs on the interception hot path.
    #[serde(default)]
    pub jev_line_select_enabled: bool,
    /// Minimum probability of `prose` before the auto-rewrite stage runs.
    #[serde(default = "default_jev_prose_min_prob")]
    pub jev_prose_min_prob: f64,
    /// Minimum probability of `source_code` before a rewrite is refused.
    #[serde(default = "default_jev_code_min_prob")]
    pub jev_code_min_prob: f64,
    /// Minimum confidence before a same-language translation is skipped.
    #[serde(default = "default_jev_language_min_confidence")]
    pub jev_language_min_confidence: f64,
    /// A rewrite whose faithfulness (or target-language) probability is below
    /// this falls back to the original text.
    #[serde(default = "default_jev_verify_fail_below")]
    pub jev_verify_fail_below: f64,
    /// Minimum probability before a first/last response line is stripped as
    /// model commentary.
    #[serde(default = "default_jev_commentary_min_prob")]
    pub jev_commentary_min_prob: f64,
    /// Minimum per-line probability for a line to be kept by Jev line
    /// selection.
    #[serde(default = "default_jev_line_keep_min_prob")]
    pub jev_line_keep_min_prob: f64,
    /// Jev input price in USD per million tokens, for cost estimates in
    /// `ecotokens router status`. TypeSafe publishes no price, so unset by
    /// default (tokens only).
    #[serde(default)]
    pub jev_usd_per_mtok_input: Option<f64>,
    /// Jev output price in USD per million tokens (see above).
    #[serde(default)]
    pub jev_usd_per_mtok_output: Option<f64>,

    // ── Model router (UserPromptSubmit hook, sized by Jev) ──
    /// Size every Claude Code message with Jev and hand small jobs to a
    /// cheaper helper agent. Independent of `jev_enabled`. Toggled by
    /// `ecotokens router on|off`. Off by default.
    #[serde(default)]
    pub router_enabled: bool,
    /// Jev timeout for the router, in milliseconds. The hook runs before the
    /// message is sent, so this is kept short.
    #[serde(default = "default_router_timeout_ms")]
    pub router_timeout_ms: u64,
    /// Below this size confidence the main session handles the message.
    #[serde(default = "default_router_min_confidence")]
    pub router_min_confidence: f64,
    /// At or above this probability that the message only makes sense inside
    /// the conversation, the main session handles it.
    #[serde(default = "default_router_followup_min_prob")]
    pub router_followup_min_prob: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct LegacySettingsFile {
    #[serde(flatten)]
    settings: Settings,
    #[serde(default)]
    abbreviations_custom: HashMap<String, String>,
    #[serde(default)]
    model_pricing: HashMap<String, ModelPrice>,
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
    "claude-sonnet-5".into()
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
fn default_rewrite_timeout_ms() -> u64 {
    30000
}
fn default_rewrite_context_tokens() -> u32 {
    8192
}
fn default_rewrite_truncation_ratio() -> f32 {
    0.5
}
fn default_rewrite_diff_retention() -> u32 {
    50
}
fn default_rewrite_auto_min_tokens() -> u32 {
    500
}
fn default_rewrite_auto_timeout_ms() -> u64 {
    2000
}
fn default_jev_timeout_ms() -> u64 {
    1000
}
fn default_jev_max_input_chars() -> usize {
    32000
}
fn default_jev_prose_min_prob() -> f64 {
    0.9
}
fn default_jev_code_min_prob() -> f64 {
    0.8
}
fn default_jev_language_min_confidence() -> f64 {
    0.8
}
fn default_jev_verify_fail_below() -> f64 {
    0.3
}
fn default_jev_commentary_min_prob() -> f64 {
    0.8
}
fn default_jev_line_keep_min_prob() -> f64 {
    0.05
}
fn default_router_timeout_ms() -> u64 {
    800
}
fn default_router_min_confidence() -> f64 {
    0.6
}
fn default_router_followup_min_prob() -> f64 {
    0.5
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
            default_model: "claude-sonnet-5".into(),
            model_pricing: default_model_pricing(),
            embed_provider: EmbedProvider::default(),
            ai_summary_enabled: false,
            ai_summary_model: None,
            ai_summary_url: None,
            ai_summary_min_tokens: 2500,
            ai_summary_timeout_ms: 3000,
            auto_watch: false,
            post_hook_depth: 1,
            abbreviations_enabled: false,
            abbreviations_custom: HashMap::new(),
            debuglog: false,
            rewrite_model: None,
            rewrite_url: None,
            rewrite_timeout_ms: default_rewrite_timeout_ms(),
            rewrite_context_tokens: default_rewrite_context_tokens(),
            rewrite_truncation_ratio: default_rewrite_truncation_ratio(),
            rewrite_save_diff: false,
            rewrite_diff_dir: None,
            rewrite_diff_retention: default_rewrite_diff_retention(),
            rewrite_auto_enabled: false,
            rewrite_auto_min_tokens: default_rewrite_auto_min_tokens(),
            rewrite_auto_timeout_ms: default_rewrite_auto_timeout_ms(),
            rewrite_auto_mode: None,
            rewrite_auto_target: None,
            jev_enabled: false,
            jev_url: None,
            jev_timeout_ms: default_jev_timeout_ms(),
            jev_max_input_chars: default_jev_max_input_chars(),
            jev_line_select_enabled: false,
            jev_prose_min_prob: default_jev_prose_min_prob(),
            jev_code_min_prob: default_jev_code_min_prob(),
            jev_language_min_confidence: default_jev_language_min_confidence(),
            jev_verify_fail_below: default_jev_verify_fail_below(),
            jev_commentary_min_prob: default_jev_commentary_min_prob(),
            jev_line_keep_min_prob: default_jev_line_keep_min_prob(),
            jev_usd_per_mtok_input: None,
            jev_usd_per_mtok_output: None,
            router_enabled: false,
            router_timeout_ms: default_router_timeout_ms(),
            router_min_confidence: default_router_min_confidence(),
            router_followup_min_prob: default_router_followup_min_prob(),
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

    fn pricing_path_for(config_path: &Path) -> PathBuf {
        config_path
            .parent()
            .map(|parent| parent.join("pricing.json"))
            .unwrap_or_else(|| PathBuf::from("pricing.json"))
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

    fn load_pricing(path: &Path) -> Option<HashMap<String, ModelPrice>> {
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    fn load_from_paths(config_path: &Path, abbreviations_path: &Path, pricing_path: &Path) -> Self {
        let legacy = Self::load_legacy_config(config_path);
        let mut settings = legacy.settings;
        settings.abbreviations_custom =
            Self::load_abbreviations(abbreviations_path).unwrap_or(legacy.abbreviations_custom);
        // pricing.json > migration depuis config.json > built-in seul
        let overrides = Self::load_pricing(pricing_path).unwrap_or(legacy.model_pricing);
        settings.model_pricing = default_model_pricing();
        for (k, v) in overrides {
            settings.model_pricing.insert(k, v);
        }
        // Migrate only the legacy externally-tagged providers (ollama, lm_studio)
        // → Candle. An explicit `"type": "none"` is a deliberate user choice to
        // disable embeddings and must be preserved.
        if matches!(settings.embed_provider, EmbedProvider::Legacy) {
            settings.embed_provider = EmbedProvider::default();
        }
        if let Err(e) = settings.validate() {
            eprintln!("ecotokens: warning: invalid configuration: {e}");
        }
        settings
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Settings::default();
        };
        let abbreviations_path = Self::abbreviations_path_for(&path);
        let pricing_path = Self::pricing_path_for(&path);
        Self::load_from_paths(&path, &abbreviations_path, &pricing_path)
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
        super::atomic_write(abbreviations_path, json)
    }

    fn save_pricing(
        pricing_path: &Path,
        pricing: &HashMap<String, ModelPrice>,
    ) -> std::io::Result<()> {
        let built_in = super::models::build_pricing_map();
        let overrides: HashMap<_, _> = pricing
            .iter()
            .filter(|(k, v)| {
                // A small absolute tolerance rather than `f64::EPSILON`: parsing a
                // JSON price like 0.252 can differ from the built-in constant by
                // more than one ULP, which would otherwise flag it as an override
                // and cause spurious pricing.json writes.
                const PRICE_TOL: f64 = 1e-9;
                built_in.get(*k).map_or(true, |b| {
                    (b.input_usd_per_1m - v.input_usd_per_1m).abs() > PRICE_TOL
                        || (b.output_usd_per_1m - v.output_usd_per_1m).abs() > PRICE_TOL
                })
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        if overrides.is_empty() {
            return match std::fs::remove_file(pricing_path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            };
        }
        if let Some(parent) = pricing_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&overrides)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        super::atomic_write(pricing_path, json)
    }

    fn save_to_paths(
        &self,
        config_path: &Path,
        abbreviations_path: &Path,
        pricing_path: &Path,
    ) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        super::atomic_write(config_path, json)?;
        Self::save_abbreviations(abbreviations_path, &self.abbreviations_custom)?;
        Self::save_pricing(pricing_path, &self.model_pricing)
    }

    pub fn save(&self) -> std::io::Result<()> {
        let config_path = Self::config_path().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "cannot resolve config dir")
        })?;
        let abbreviations_path = Self::abbreviations_path_for(&config_path);
        let pricing_path = Self::pricing_path_for(&config_path);
        self.save_to_paths(&config_path, &abbreviations_path, &pricing_path)
    }

    #[allow(dead_code)]
    pub fn load_from_paths_pub(
        config_path: &Path,
        abbreviations_path: &Path,
        pricing_path: &Path,
    ) -> Self {
        Self::load_from_paths(config_path, abbreviations_path, pricing_path)
    }

    /// Modes accepting a `--to`/`_target` value, mirrored here as plain
    /// strings rather than depending on `crate::rewrite::Mode` — settings.rs
    /// must compile with `--no-default-features` (rewrite disabled).
    fn rewrite_mode_requires_target(mode: &str) -> bool {
        matches!(mode, "tone" | "reading-level" | "translate")
    }

    fn is_localhost_url(url: &str) -> bool {
        url.parse::<reqwest::Url>()
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .is_some_and(|host| {
                matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1" | "[::1]")
            })
    }

    // Exposed for callers that want to validate settings explicitly
    // (tests, tooling, or future CLI checks); also run at load time (see
    // `load_from_paths`), where violations are reported rather than silently
    // ignored (FR-019, FR-033, FR-011 — contracts/config-settings.md).
    pub fn validate(&self) -> Result<(), String> {
        let mut errors = Vec::new();

        if !(10..=10000).contains(&self.summary_threshold_lines) {
            errors.push(format!(
                "summary_threshold_lines must be in [10, 10000], got {}",
                self.summary_threshold_lines
            ));
        }
        if !(1024..=1048576).contains(&self.summary_threshold_bytes) {
            errors.push(format!(
                "summary_threshold_bytes must be in [1024, 1048576], got {}",
                self.summary_threshold_bytes
            ));
        }

        if let Some(url) = &self.rewrite_url {
            if !Self::is_localhost_url(url) {
                errors.push(format!(
                    "rewrite_url must resolve to localhost, 127.0.0.1, or ::1, got: {url}"
                ));
            }
        }
        if !(self.rewrite_truncation_ratio > 0.0 && self.rewrite_truncation_ratio < 1.0) {
            errors.push(format!(
                "rewrite_truncation_ratio must be in (0.0, 1.0), got {}",
                self.rewrite_truncation_ratio
            ));
        }
        if self.rewrite_auto_enabled {
            match &self.rewrite_auto_mode {
                None => errors.push(
                    "rewrite_auto_enabled is true but rewrite_auto_mode is not set".to_string(),
                ),
                Some(mode) => {
                    if Self::rewrite_mode_requires_target(mode)
                        && self.rewrite_auto_target.is_none()
                    {
                        errors.push(format!(
                            "rewrite_auto_mode '{mode}' requires rewrite_auto_target, which is not set"
                        ));
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}
