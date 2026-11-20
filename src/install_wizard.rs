//! Model of the `ecotokens install` setup assistant.
//!
//! The assistant runs only when `install` is called with no option on a
//! terminal. This module holds what it is about — the harnesses, the features
//! each of them supports, the starting state and the resulting [`WizardPlan`]
//! — and the one probe it needs ([`detect_ollama`]). The interactive screen
//! lives in `tui::install`; applying the plan is left to the caller, which
//! reuses the regular install, `router`, `handoff` and `auto-watch` code paths.

use std::path::Path;
use std::time::Duration;

/// Default Ollama model proposed for AI summaries.
pub use crate::config::settings::DEFAULT_LOCAL_MODEL as DEFAULT_AI_SUMMARY_MODEL;

/// Ollama base URL used when `ai_summary_url` is not set.
pub const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// What the assistant found about Ollama on this machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum OllamaStatus {
    /// The server answered; these models are pulled.
    Running { models: Vec<String> },
    /// The `ollama` binary is on `PATH` but the server did not answer.
    Stopped,
    /// Neither the server nor the binary were found.
    #[default]
    Missing,
}

impl OllamaStatus {
    pub fn models(&self) -> &[String] {
        match self {
            OllamaStatus::Running { models } => models,
            _ => &[],
        }
    }

    /// Pulled models that generate text (usable for AI summary).
    pub fn text_models(&self) -> Vec<String> {
        self.models()
            .iter()
            .filter(|m| !is_embedding_model(m))
            .cloned()
            .collect()
    }

    /// Pulled embedding models (usable for semantic search).
    pub fn embedding_models(&self) -> Vec<String> {
        self.models()
            .iter()
            .filter(|m| is_embedding_model(m))
            .cloned()
            .collect()
    }
}

/// Model names from an Ollama `GET /api/tags` response.
pub fn parse_tags(json: &serde_json::Value) -> Vec<String> {
    json["models"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter_map(|m| m["name"].as_str().or_else(|| m["model"].as_str()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Whether an executable named `name` is in one of the `PATH` directories.
fn on_path(name: &str, path_var: Option<&std::ffi::OsStr>) -> bool {
    path_var.is_some_and(|p| {
        std::env::split_paths(p).any(|dir| {
            let candidate = dir.join(name);
            candidate.is_file() && is_executable(&candidate)
        })
    })
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_: &Path) -> bool {
    true
}

/// Probe Ollama: ask the server at `url` for its models, else look for the
/// binary on `PATH`. Only a localhost URL is queried, as for the summary
/// itself; any other URL counts as unreachable.
pub fn detect_ollama(url: &str, timeout: Duration) -> OllamaStatus {
    detect_ollama_with(url, timeout, std::env::var_os("PATH").as_deref())
}

fn detect_ollama_with(
    url: &str,
    timeout: Duration,
    path_var: Option<&std::ffi::OsStr>,
) -> OllamaStatus {
    let local = url.parse::<reqwest::Url>().ok().is_some_and(|u| {
        matches!(
            u.host_str(),
            Some("localhost" | "127.0.0.1" | "::1" | "[::1]")
        )
    });
    if local {
        let tags = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .ok()
            .and_then(|c| {
                c.get(format!("{}/api/tags", url.trim_end_matches('/')))
                    .send()
                    .ok()
            })
            .filter(|r| r.status().is_success())
            .and_then(|r| r.json::<serde_json::Value>().ok());
        if let Some(json) = tags {
            return OllamaStatus::Running {
                models: parse_tags(&json),
            };
        }
    }
    if on_path(executable_name(), path_var) {
        OllamaStatus::Stopped
    } else {
        OllamaStatus::Missing
    }
}

fn executable_name() -> &'static str {
    if cfg!(windows) {
        "ollama.exe"
    } else {
        "ollama"
    }
}

/// Whether `model` is among `pulled`; `llama3.2` matches `llama3.2:latest`.
pub fn model_is_pulled(model: &str, pulled: &[String]) -> bool {
    let norm = |m: &str| {
        if m.contains(':') {
            m.to_string()
        } else {
            format!("{m}:latest")
        }
    };
    let want = norm(model.trim());
    pulled.iter().any(|p| norm(p) == want)
}

/// A harness ecotokens can be installed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harness {
    Claude,
    Gemini,
    Qwen,
    Pi,
    Hermes,
    Codex,
    OpenCode,
}

impl Harness {
    pub const ALL: [Harness; 7] = [
        Harness::Claude,
        Harness::Gemini,
        Harness::Qwen,
        Harness::Pi,
        Harness::Hermes,
        Harness::Codex,
        Harness::OpenCode,
    ];

    /// The `--target` value of this harness.
    pub fn target(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Gemini => "gemini",
            Harness::Qwen => "qwen",
            Harness::Pi => "pi",
            Harness::Hermes => "hermes",
            Harness::Codex => "codex",
            Harness::OpenCode => "opencode",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Harness::Claude => "Claude Code",
            Harness::Gemini => "Gemini CLI",
            Harness::Qwen => "Qwen Code",
            Harness::Pi => "Pi",
            Harness::Hermes => "Hermes Agent",
            Harness::Codex => "Codex",
            Harness::OpenCode => "OpenCode",
        }
    }

    /// What `install` writes for this harness.
    pub fn installs(self) -> &'static str {
        match self {
            Harness::Claude => "hooks and MCP server in ~/.claude/settings.json",
            Harness::Gemini => "hooks and MCP server in ~/.gemini/settings.json",
            Harness::Qwen => "hooks and MCP server in ~/.qwen/settings.json",
            Harness::Pi => "extension in ~/.pi/agent/extensions/",
            Harness::Hermes => "plugin in ~/.hermes/plugins/",
            Harness::Codex => "plugin, hooks and MCP server in ~/.codex/",
            Harness::OpenCode => "plugin in ~/.config/opencode/plugins/",
        }
    }

    /// Whether `ecotokens watch` can start and stop with this harness' sessions.
    /// Gemini has no session hooks; Codex has no SessionEnd; OpenCode's plugin
    /// does not drive the watcher.
    pub fn supports_auto_watch(self) -> bool {
        matches!(
            self,
            Harness::Claude | Harness::Qwen | Harness::Pi | Harness::Hermes
        )
    }
}

/// Whether an Ollama model name looks like an embedding model rather than a
/// text generator (`nomic-embed-text`, `qwen3-embedding`, `bge-m3`, …).
pub fn is_embedding_model(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("embed") || n.starts_with("all-minilm") || n.starts_with("bge-")
}

/// Embedding model proposed when Ollama is chosen and none is pulled.
pub const SUGGESTED_OLLAMA_EMBED_MODEL: &str = "nomic-embed-text";

/// Where semantic search gets its vectors from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedChoice {
    /// No embeddings: BM25 search only.
    None,
    /// Built-in Candle model (downloaded on first use).
    Candle(String),
    /// Ollama embedding API with this model.
    Ollama(String),
}

impl EmbedChoice {
    pub fn describe(&self) -> String {
        match self {
            EmbedChoice::None => "off (BM25 only)".to_string(),
            EmbedChoice::Candle(m) => format!("candle · {}", short_model(m)),
            EmbedChoice::Ollama(m) => format!("ollama · {m}"),
        }
    }
}

/// `sentence-transformers/all-MiniLM-L6-v2` → `all-MiniLM-L6-v2`.
fn short_model(m: &str) -> &str {
    m.rsplit('/').next().unwrap_or(m)
}

/// Where `TYPESAFE_API_KEY` was found.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KeySource {
    /// The shell (or the harness) exports it; it wins over the `.env` file.
    Shell,
    /// `~/.config/ecotokens/.env`.
    EnvFile,
    #[default]
    Missing,
}

/// An optional feature the assistant can switch on or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    AutoWatch,
    Router,
    Handoff,
    HermesEnablePlugin,
    Jev,
    JevLineSelect,
    AiSummary,
    Rewrite,
    Embeddings,
}

impl Feature {
    pub const ALL: [Feature; 9] = [
        Feature::AutoWatch,
        Feature::Router,
        Feature::Handoff,
        Feature::HermesEnablePlugin,
        Feature::Jev,
        Feature::JevLineSelect,
        Feature::AiSummary,
        Feature::Rewrite,
        Feature::Embeddings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Feature::AutoWatch => "Auto-watch",
            Feature::Router => "Model router",
            Feature::Handoff => "Session handoff",
            Feature::HermesEnablePlugin => "Enable Hermes plugin",
            Feature::Jev => "Jev judgments",
            Feature::JevLineSelect => "Jev line selection",
            Feature::AiSummary => "AI summary",
            Feature::Rewrite => "Text rewrite",
            Feature::Embeddings => "Semantic search",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Feature::AutoWatch => {
                "Starts `ecotokens watch` with each session, so the code index stays up to date."
            }
            Feature::Router => {
                "Sends small jobs to cheaper helper agents. Each message is sent (secrets \
                 masked) to TypeSafe to be sized."
            }
            Feature::Handoff => {
                "Saves the session state and brings it back after /clear or a compaction."
            }
            Feature::HermesEnablePlugin => {
                "Adds ecotokens to plugins.enabled in ~/.hermes/config.yaml directly, instead \
                 of calling the hermes CLI."
            }
            Feature::Jev => {
                "TypeSafe Jev judges outputs (prose, code, language) to filter them better. \
                 Masked excerpts are sent to TypeSafe; without a key, heuristics are used."
            }
            Feature::JevLineSelect => {
                "Lets Jev pick the lines worth keeping when the generic filter shortens an output."
            }
            Feature::AiSummary => "Summarizes very large outputs with a local Ollama model.",
            Feature::Rewrite => {
                "`ecotokens rewrite` and the MCP rewrite tool use this local model (by default the \
                 AI summary one). Tick to also rewrite long prose outputs automatically."
            }
            Feature::Embeddings => {
                "Embeddings for `ecotokens search` and the MCP search tool, on top of BM25. \
                 Candle runs locally (model downloaded on first use); Ollama uses its embedding API."
            }
        }
    }

    /// The harnesses of `selected` this feature works with; empty when the
    /// feature does not apply (it is then not offered).
    pub fn harnesses(self, selected: &[Harness]) -> Vec<Harness> {
        selected
            .iter()
            .copied()
            .filter(|h| match self {
                Feature::AutoWatch => h.supports_auto_watch(),
                Feature::Router | Feature::Handoff => *h == Harness::Claude,
                Feature::HermesEnablePlugin => *h == Harness::Hermes,
                Feature::Jev
                | Feature::JevLineSelect
                | Feature::AiSummary
                | Feature::Rewrite
                | Feature::Embeddings => true,
            })
            .collect()
    }

    /// The current state of the machine (what "unchanged" means).
    pub fn current(self, ctx: &WizardContext) -> bool {
        match self {
            Feature::AutoWatch => ctx.auto_watch,
            Feature::Router => ctx.router_enabled,
            Feature::Handoff => ctx.handoff_enabled,
            Feature::HermesEnablePlugin => false,
            Feature::Jev => ctx.jev_enabled,
            Feature::JevLineSelect => ctx.jev_line_select,
            Feature::AiSummary => ctx.ai_summary_enabled,
            Feature::Rewrite => ctx.rewrite_auto,
            Feature::Embeddings => ctx.embed != EmbedChoice::None,
        }
    }

    /// Starting value of the switch: the current state, except that AI
    /// summary is proposed when Ollama is running with a text model.
    pub fn proposed(self, ctx: &WizardContext) -> bool {
        self.current(ctx) || (self == Feature::AiSummary && !ctx.ollama.text_models().is_empty())
    }

    /// A warning to show next to the feature, if any, given what is chosen.
    pub fn warning(self, ctx: &WizardContext, c: &Choices) -> Option<String> {
        let key = crate::jev::API_KEY_ENV;
        let has_key = ctx.jev_key != KeySource::Missing || c.jev_api_key.is_some();
        match self {
            Feature::Router if !has_key => Some(format!(
                "{key} is not set: the router does nothing until it is (press e on Jev to enter it)"
            )),
            Feature::Jev if !has_key => Some(format!(
                "{key} is not set: Jev falls back to heuristics (press e to enter it)"
            )),
            Feature::Jev if c.jev_api_key.is_some() && ctx.jev_key == KeySource::Shell => {
                Some(format!(
                    "your shell already exports {key}; it wins over the key saved in the .env file"
                ))
            }
            Feature::AiSummary => ollama_warning(&ctx.ollama, c.ai_model.trim(), "model"),
            Feature::Rewrite => ollama_warning(&ctx.ollama, &c.rewrite_model_used(), "model"),
            Feature::Embeddings => match &c.embed {
                EmbedChoice::Ollama(m) => ollama_warning(&ctx.ollama, m.trim(), "embedding model"),
                _ => None,
            },
            _ => None,
        }
    }
}

/// What to do so that Ollama can serve `model`, or `None` if it can.
fn ollama_warning(st: &OllamaStatus, model: &str, what: &str) -> Option<String> {
    match st {
        OllamaStatus::Missing => Some(format!(
            "Ollama not found: install it from https://ollama.com, then run `ollama pull {model}`"
        )),
        OllamaStatus::Stopped => {
            Some("Ollama is installed but not running: start it with `ollama serve`".into())
        }
        OllamaStatus::Running { models } if !model_is_pulled(model, models) => Some(format!(
            "{what} {model} is not pulled yet: run `ollama pull {model}`"
        )),
        OllamaStatus::Running { .. } => None,
    }
}

/// Values chosen on the features screen besides the switches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choices {
    pub ai_model: String,
    /// A key typed in the assistant, to save in `~/.config/ecotokens/.env`.
    pub jev_api_key: Option<String>,
    /// Provider and model used when semantic search is on.
    pub embed: EmbedChoice,
    /// Model for `rewrite`; `None` follows the AI summary model.
    pub rewrite_model: Option<String>,
}

impl Choices {
    /// The model `rewrite` will actually call.
    pub fn rewrite_model_used(&self) -> String {
        match self.rewrite_model.as_deref().map(str::trim) {
            Some(m) if !m.is_empty() => m.to_string(),
            _ => match self.ai_model.trim() {
                "" => DEFAULT_AI_SUMMARY_MODEL.to_string(),
                m => m.to_string(),
            },
        }
    }
}

/// What is already on this machine for one harness.
#[derive(Debug, Clone, Copy, Default)]
pub struct HarnessState {
    /// The harness' configuration directory exists (it looks used here).
    pub detected: bool,
    /// ecotokens is already installed for it.
    pub installed: bool,
}

/// Current state the assistant starts from; every choice defaults to it.
#[derive(Debug, Clone)]
pub struct WizardContext {
    /// One entry per [`Harness::ALL`] item, in the same order.
    pub harnesses: [HarnessState; 7],
    pub auto_watch: bool,
    pub router_enabled: bool,
    pub handoff_enabled: bool,
    /// Where `TYPESAFE_API_KEY` comes from (the router and Jev need it).
    pub jev_key: KeySource,
    pub jev_enabled: bool,
    pub jev_line_select: bool,
    pub ai_summary_enabled: bool,
    pub ai_summary_model: Option<String>,
    pub ollama: OllamaStatus,
    /// Current semantic search setting.
    pub embed: EmbedChoice,
    pub rewrite_auto: bool,
    pub rewrite_model: Option<String>,
}

impl Default for WizardContext {
    fn default() -> Self {
        WizardContext {
            harnesses: Default::default(),
            auto_watch: false,
            router_enabled: false,
            handoff_enabled: false,
            jev_key: KeySource::Missing,
            jev_enabled: false,
            jev_line_select: false,
            ai_summary_enabled: false,
            ai_summary_model: None,
            ollama: OllamaStatus::Missing,
            embed: EmbedChoice::Candle(DEFAULT_CANDLE_EMBED_MODEL.to_string()),
            rewrite_auto: false,
            rewrite_model: None,
        }
    }
}

pub use crate::config::settings::DEFAULT_CANDLE_EMBED_MODEL;

impl WizardContext {
    /// Harnesses preselected: those installed or detected, else Claude Code.
    pub fn default_harnesses(&self) -> Vec<Harness> {
        let picked: Vec<Harness> = Harness::ALL
            .iter()
            .zip(self.harnesses.iter())
            .filter(|(_, st)| st.installed || st.detected)
            .map(|(h, _)| *h)
            .collect();
        if picked.is_empty() {
            vec![Harness::Claude]
        } else {
            picked
        }
    }

    /// Model proposed for AI summary: the configured one; otherwise, when
    /// Ollama has text models, the default one if it is pulled, else the
    /// first pulled one; otherwise the default.
    pub fn ai_model(&self) -> String {
        if let Some(m) = self
            .ai_summary_model
            .as_deref()
            .filter(|m| !m.trim().is_empty())
        {
            return m.to_string();
        }
        let pulled = self.ollama.text_models();
        if !pulled.is_empty() && !model_is_pulled(DEFAULT_AI_SUMMARY_MODEL, &pulled) {
            return pulled[0].clone();
        }
        DEFAULT_AI_SUMMARY_MODEL.to_string()
    }

    /// Embedding choice the switch starts from: the current one, or Candle
    /// when semantic search is off.
    pub fn embed_start(&self) -> EmbedChoice {
        match &self.embed {
            EmbedChoice::None => EmbedChoice::Candle(DEFAULT_CANDLE_EMBED_MODEL.to_string()),
            other => other.clone(),
        }
    }

    /// Ollama embedding model proposed when switching to Ollama: the first
    /// pulled embedding model, else the suggested one.
    pub fn ollama_embed_model(&self) -> String {
        self.ollama
            .embedding_models()
            .first()
            .cloned()
            .unwrap_or_else(|| SUGGESTED_OLLAMA_EMBED_MODEL.to_string())
    }

    pub fn choices(&self) -> Choices {
        Choices {
            ai_model: self.ai_model(),
            jev_api_key: None,
            embed: self.embed_start(),
            rewrite_model: self.rewrite_model.clone().filter(|m| !m.trim().is_empty()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiSummary {
    On(String),
    Off,
}

/// What the user chose. `None` means "not offered, leave as is".
#[derive(Clone, PartialEq, Eq)]
pub struct WizardPlan {
    pub harnesses: Vec<Harness>,
    pub auto_watch: Option<bool>,
    pub router: Option<bool>,
    pub handoff: Option<bool>,
    /// Hermes: enable the plugin by editing `config.yaml` directly.
    pub hermes_enable_plugin: bool,
    pub jev: Option<bool>,
    /// Left as is (`None`) while Jev is off: it has no effect then.
    pub jev_line_select: Option<bool>,
    /// A key to save in `~/.config/ecotokens/.env`. Never printed.
    pub jev_api_key: Option<String>,
    /// `Some(On(model))` turns AI summaries on with that model, `Some(Off)`
    /// turns them off, `None` leaves the setting as is.
    pub ai_summary: Option<AiSummary>,
    /// `Some` only when it differs from the current setting.
    pub embed: Option<EmbedChoice>,
    pub rewrite_auto: Option<bool>,
    /// `Some(model)` sets it, `Some(None)` clears it (follow the AI summary
    /// model); `None` leaves it as is. Only set when it changes.
    pub rewrite_model: Option<Option<String>>,
}

/// Debug output never shows the API key.
impl std::fmt::Debug for WizardPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WizardPlan")
            .field("harnesses", &self.harnesses)
            .field("auto_watch", &self.auto_watch)
            .field("router", &self.router)
            .field("handoff", &self.handoff)
            .field("hermes_enable_plugin", &self.hermes_enable_plugin)
            .field("jev", &self.jev)
            .field("jev_line_select", &self.jev_line_select)
            .field(
                "jev_api_key",
                &self.jev_api_key.as_ref().map(|_| "<redacted>"),
            )
            .field("ai_summary", &self.ai_summary)
            .field("embed", &self.embed)
            .field("rewrite_auto", &self.rewrite_auto)
            .field("rewrite_model", &self.rewrite_model)
            .finish()
    }
}

impl WizardPlan {
    /// Build the plan from the chosen harnesses, feature switches and
    /// values. A feature that does not apply to the chosen harnesses is left
    /// untouched, whatever its switch says.
    pub fn from_choices(
        ctx: &WizardContext,
        harnesses: Vec<Harness>,
        is_on: impl Fn(Feature) -> bool,
        c: &Choices,
    ) -> Self {
        let offered = |f: Feature| !f.harnesses(&harnesses).is_empty();
        let pick = |f: Feature| offered(f).then(|| is_on(f));
        let ai_summary = match pick(Feature::AiSummary) {
            Some(true) => {
                let m = c.ai_model.trim();
                Some(AiSummary::On(if m.is_empty() {
                    DEFAULT_AI_SUMMARY_MODEL.to_string()
                } else {
                    m.to_string()
                }))
            }
            Some(false) if ctx.ai_summary_enabled => Some(AiSummary::Off),
            _ => None,
        };
        let jev = pick(Feature::Jev);
        let embed = pick(Feature::Embeddings)
            .map(|on| {
                if !on {
                    return EmbedChoice::None;
                }
                match &c.embed {
                    EmbedChoice::Candle(m) if m.trim().is_empty() => {
                        EmbedChoice::Candle(DEFAULT_CANDLE_EMBED_MODEL.to_string())
                    }
                    EmbedChoice::Ollama(m) if m.trim().is_empty() => {
                        EmbedChoice::Ollama(SUGGESTED_OLLAMA_EMBED_MODEL.to_string())
                    }
                    EmbedChoice::Candle(m) => EmbedChoice::Candle(m.trim().to_string()),
                    EmbedChoice::Ollama(m) => EmbedChoice::Ollama(m.trim().to_string()),
                    EmbedChoice::None => {
                        EmbedChoice::Candle(DEFAULT_CANDLE_EMBED_MODEL.to_string())
                    }
                }
            })
            .filter(|e| *e != ctx.embed);
        let rewrite_model = c
            .rewrite_model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(str::to_string);
        let rewrite_model = (rewrite_model != ctx.rewrite_model).then_some(rewrite_model);
        WizardPlan {
            rewrite_auto: pick(Feature::Rewrite),
            rewrite_model,
            auto_watch: pick(Feature::AutoWatch),
            router: pick(Feature::Router),
            handoff: pick(Feature::Handoff),
            hermes_enable_plugin: pick(Feature::HermesEnablePlugin).unwrap_or(false),
            jev,
            jev_line_select: if jev == Some(true) {
                pick(Feature::JevLineSelect)
            } else {
                None
            },
            jev_api_key: c
                .jev_api_key
                .as_deref()
                .map(str::trim)
                .filter(|k| !k.is_empty())
                .map(str::to_string),
            ai_summary,
            embed,
            harnesses,
        }
    }

    pub fn has(&self, h: Harness) -> bool {
        self.harnesses.contains(&h)
    }
}

/// How a setting changes when the plan is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    TurnOn,
    TurnOff,
    /// Stays on with a different value (another model or provider).
    Switch,
    StaysOn,
    StaysOff,
}

/// One line of the summary screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryRow {
    pub label: &'static str,
    pub change: Change,
    pub detail: Option<String>,
}

/// One summary row per offered setting, in [`Feature::ALL`] order, plus the
/// API key when one was entered.
pub fn changes(ctx: &WizardContext, plan: &WizardPlan) -> Vec<SummaryRow> {
    let mut rows = Vec::new();
    let mut push = |label, change, detail: Option<String>| {
        rows.push(SummaryRow {
            label,
            change,
            detail,
        })
    };
    let flip = |before: bool, after: bool| match (before, after) {
        (false, true) => Change::TurnOn,
        (true, false) => Change::TurnOff,
        (true, true) => Change::StaysOn,
        (false, false) => Change::StaysOff,
    };
    for (f, now) in [
        (Feature::AutoWatch, plan.auto_watch),
        (Feature::Router, plan.router),
        (Feature::Handoff, plan.handoff),
    ] {
        if let Some(on) = now {
            push(f.label(), flip(f.current(ctx), on), None);
        }
    }
    if !Feature::HermesEnablePlugin
        .harnesses(&plan.harnesses)
        .is_empty()
    {
        let c = if plan.hermes_enable_plugin {
            Change::TurnOn
        } else {
            Change::StaysOff
        };
        push(Feature::HermesEnablePlugin.label(), c, None);
    }
    if let Some(on) = plan.jev {
        push(Feature::Jev.label(), flip(ctx.jev_enabled, on), None);
    }
    if let Some(on) = plan.jev_line_select {
        push(
            Feature::JevLineSelect.label(),
            flip(ctx.jev_line_select, on),
            None,
        );
    }
    if plan.jev_api_key.is_some() {
        push(
            "Jev API key",
            Change::TurnOn,
            Some("saved to ~/.config/ecotokens/.env (mode 600)".into()),
        );
    }
    if !Feature::AiSummary.harnesses(&plan.harnesses).is_empty() {
        let (change, detail) = match &plan.ai_summary {
            Some(AiSummary::On(m)) => {
                let same = ctx.ai_summary_enabled && ctx.ai_summary_model.as_deref() == Some(m);
                let c = if !ctx.ai_summary_enabled {
                    Change::TurnOn
                } else if same {
                    Change::StaysOn
                } else {
                    Change::Switch
                };
                (c, Some(m.clone()))
            }
            Some(AiSummary::Off) => (Change::TurnOff, None),
            None => (Change::StaysOff, None),
        };
        push(Feature::AiSummary.label(), change, detail);
    }
    if let Some(on) = plan.rewrite_auto {
        let model = match plan.rewrite_model.as_ref().unwrap_or(&ctx.rewrite_model) {
            Some(m) => m.clone(),
            None => "same as AI summary".to_string(),
        };
        let change = match (ctx.rewrite_auto, on, plan.rewrite_model.is_some()) {
            (false, true, _) => Change::TurnOn,
            (true, false, _) => Change::TurnOff,
            (_, _, true) => Change::Switch,
            (true, true, false) => Change::StaysOn,
            (false, false, false) => Change::StaysOff,
        };
        let auto = if on { "auto" } else { "manual only" };
        push(
            Feature::Rewrite.label(),
            change,
            Some(format!("{auto} · model: {model}")),
        );
    }
    if !Feature::Embeddings.harnesses(&plan.harnesses).is_empty() {
        let now = plan.embed.as_ref().unwrap_or(&ctx.embed);
        let change = match (&ctx.embed, now, plan.embed.is_some()) {
            (_, _, false) if ctx.embed == EmbedChoice::None => Change::StaysOff,
            (_, _, false) => Change::StaysOn,
            (EmbedChoice::None, _, true) => Change::TurnOn,
            (_, EmbedChoice::None, true) => Change::TurnOff,
            _ => Change::Switch,
        };
        let detail = (*now != EmbedChoice::None).then(|| now.describe());
        push(Feature::Embeddings.label(), change, detail);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running(models: &[&str]) -> OllamaStatus {
        OllamaStatus::Running {
            models: models.iter().map(|m| m.to_string()).collect(),
        }
    }

    fn choices(ctx: &WizardContext) -> Choices {
        ctx.choices()
    }

    #[test]
    fn features_apply_only_to_their_harnesses() {
        let gemini = [Harness::Gemini];
        assert!(Feature::AutoWatch.harnesses(&gemini).is_empty());
        assert!(Feature::Router.harnesses(&gemini).is_empty());
        for f in [Feature::Jev, Feature::AiSummary, Feature::Embeddings] {
            assert_eq!(f.harnesses(&gemini), vec![Harness::Gemini], "{f:?}");
        }
        let mixed = [Harness::Claude, Harness::Codex, Harness::Hermes];
        assert_eq!(
            Feature::AutoWatch.harnesses(&mixed),
            vec![Harness::Claude, Harness::Hermes]
        );
        assert_eq!(
            Feature::HermesEnablePlugin.harnesses(&mixed),
            vec![Harness::Hermes]
        );
    }

    #[test]
    fn default_harnesses_are_detected_or_installed_else_claude() {
        let mut ctx = WizardContext::default();
        assert_eq!(ctx.default_harnesses(), vec![Harness::Claude]);
        ctx.harnesses[1].detected = true;
        ctx.harnesses[4].installed = true;
        assert_eq!(
            ctx.default_harnesses(),
            vec![Harness::Gemini, Harness::Hermes]
        );
    }

    #[test]
    fn plan_ignores_features_that_do_not_apply() {
        let ctx = WizardContext::default();
        let mut c = choices(&ctx);
        c.ai_model = "m".into();
        let plan = WizardPlan::from_choices(&ctx, vec![Harness::Gemini], |_| true, &c);
        assert_eq!(
            (plan.auto_watch, plan.router, plan.handoff),
            (None, None, None)
        );
        assert!(!plan.hermes_enable_plugin);
        assert_eq!(plan.ai_summary, Some(AiSummary::On("m".into())));
        assert_eq!(plan.jev, Some(true));
        assert_eq!(plan.jev_line_select, Some(true));
    }

    #[test]
    fn ai_summary_off_only_when_it_was_on_and_empty_model_falls_back() {
        let mut ctx = WizardContext::default();
        let mut c = choices(&ctx);
        c.ai_model = String::new();
        let off = WizardPlan::from_choices(&ctx, vec![Harness::Claude], |_| false, &c);
        assert_eq!(off.ai_summary, None);
        ctx.ai_summary_enabled = true;
        let off = WizardPlan::from_choices(&ctx, vec![Harness::Claude], |_| false, &c);
        assert_eq!(off.ai_summary, Some(AiSummary::Off));
        let on = WizardPlan::from_choices(&ctx, vec![Harness::Claude], |_| true, &c);
        assert_eq!(
            on.ai_summary,
            Some(AiSummary::On(DEFAULT_AI_SUMMARY_MODEL.into()))
        );
    }

    #[test]
    fn line_selection_is_left_alone_while_jev_is_off() {
        let ctx = WizardContext::default();
        let plan = WizardPlan::from_choices(
            &ctx,
            vec![Harness::Claude],
            |f| f == Feature::JevLineSelect,
            &choices(&ctx),
        );
        assert_eq!(plan.jev, Some(false));
        assert_eq!(plan.jev_line_select, None);
    }

    #[test]
    fn entered_key_is_trimmed_kept_out_of_debug_and_listed_in_the_summary() {
        let ctx = WizardContext::default();
        let mut c = choices(&ctx);
        c.jev_api_key = Some("  ts_secret_123  ".into());
        let plan = WizardPlan::from_choices(&ctx, vec![Harness::Claude], |_| false, &c);
        assert_eq!(plan.jev_api_key.as_deref(), Some("ts_secret_123"));
        let dbg = format!("{plan:?}");
        assert!(!dbg.contains("ts_secret_123"), "{dbg}");
        assert!(dbg.contains("<redacted>"), "{dbg}");
        let rows = changes(&ctx, &plan);
        let key = rows.iter().find(|r| r.label == "Jev API key").unwrap();
        assert_eq!(key.change, Change::TurnOn);
        assert!(!format!("{rows:?}").contains("ts_secret_123"));

        c.jev_api_key = Some("   ".into());
        let plan = WizardPlan::from_choices(&ctx, vec![Harness::Claude], |_| false, &c);
        assert_eq!(plan.jev_api_key, None, "a blank key is ignored");
    }

    #[test]
    fn embed_plan_is_set_only_when_it_changes() {
        let ctx = WizardContext::default(); // candle all-MiniLM-L6-v2
        let mut c = choices(&ctx);
        let on = |f: Feature| f == Feature::Embeddings;
        let same = WizardPlan::from_choices(&ctx, vec![Harness::Claude], on, &c);
        assert_eq!(same.embed, None);
        let off = WizardPlan::from_choices(&ctx, vec![Harness::Claude], |_| false, &c);
        assert_eq!(off.embed, Some(EmbedChoice::None));
        c.embed = EmbedChoice::Ollama(" nomic-embed-text ".into());
        let ollama = WizardPlan::from_choices(&ctx, vec![Harness::Claude], on, &c);
        assert_eq!(
            ollama.embed,
            Some(EmbedChoice::Ollama("nomic-embed-text".into()))
        );
        let row = changes(&ctx, &ollama)
            .into_iter()
            .find(|r| r.label == "Semantic search")
            .unwrap();
        assert_eq!(row.change, Change::Switch);
        assert_eq!(row.detail.as_deref(), Some("ollama · nomic-embed-text"));
    }

    #[test]
    fn changes_compare_with_the_current_state() {
        let ctx = WizardContext {
            router_enabled: true,
            ..Default::default()
        };
        let plan = WizardPlan::from_choices(
            &ctx,
            vec![Harness::Claude],
            |f| {
                matches!(
                    f,
                    Feature::AutoWatch | Feature::Router | Feature::Embeddings
                )
            },
            &choices(&ctx),
        );
        let c: Vec<(&str, Change)> = changes(&ctx, &plan)
            .iter()
            .map(|r| (r.label, r.change))
            .collect();
        assert_eq!(
            c,
            vec![
                ("Auto-watch", Change::TurnOn),
                ("Model router", Change::StaysOn),
                ("Session handoff", Change::StaysOff),
                ("Jev judgments", Change::StaysOff),
                ("AI summary", Change::StaysOff),
                ("Text rewrite", Change::StaysOff),
                ("Semantic search", Change::StaysOn),
            ]
        );
    }

    #[test]
    fn jev_and_router_warn_without_a_key_until_one_is_entered() {
        let ctx = WizardContext::default();
        let mut c = choices(&ctx);
        assert!(Feature::Router
            .warning(&ctx, &c)
            .unwrap()
            .contains("TYPESAFE_API_KEY"));
        assert!(Feature::Jev.warning(&ctx, &c).unwrap().contains("press e"));
        assert!(Feature::Handoff.warning(&ctx, &c).is_none());
        c.jev_api_key = Some("k".into());
        assert!(Feature::Router.warning(&ctx, &c).is_none());
        assert!(Feature::Jev.warning(&ctx, &c).is_none());

        let shell = WizardContext {
            jev_key: KeySource::Shell,
            ..Default::default()
        };
        assert!(Feature::Jev
            .warning(&shell, &c)
            .unwrap()
            .contains("wins over"));
    }

    #[test]
    fn rewrite_model_follows_ai_summary_unless_set() {
        let ctx = WizardContext::default();
        let mut c = choices(&ctx);
        c.ai_model = "qwen3:4b".into();
        assert_eq!(c.rewrite_model_used(), "qwen3:4b");
        c.rewrite_model = Some("mistral".into());
        assert_eq!(c.rewrite_model_used(), "mistral");
        c.rewrite_model = Some("  ".into());
        assert_eq!(
            c.rewrite_model_used(),
            "qwen3:4b",
            "blank follows AI summary"
        );
    }

    #[test]
    fn rewrite_plan_sets_the_model_only_when_it_changes() {
        let mut ctx = WizardContext::default();
        let mut c = choices(&ctx);
        let on = |f: Feature| f == Feature::Rewrite;
        let plan = WizardPlan::from_choices(&ctx, vec![Harness::Claude], on, &c);
        assert_eq!(plan.rewrite_auto, Some(true));
        assert_eq!(plan.rewrite_model, None, "still following AI summary");

        c.rewrite_model = Some("mistral".into());
        let plan = WizardPlan::from_choices(&ctx, vec![Harness::Claude], |_| false, &c);
        assert_eq!(plan.rewrite_model, Some(Some("mistral".into())));
        let row = changes(&ctx, &plan)
            .into_iter()
            .find(|r| r.label == "Text rewrite")
            .unwrap();
        assert_eq!(row.change, Change::Switch);
        assert_eq!(row.detail.as_deref(), Some("manual only · model: mistral"));

        ctx.rewrite_model = Some("mistral".into());
        let c = choices(&ctx);
        assert_eq!(c.rewrite_model.as_deref(), Some("mistral"));
        let mut cleared = c.clone();
        cleared.rewrite_model = None;
        let plan = WizardPlan::from_choices(&ctx, vec![Harness::Claude], |_| false, &cleared);
        assert_eq!(
            plan.rewrite_model,
            Some(None),
            "clearing goes back to AI summary"
        );
    }

    #[test]
    fn rewrite_warns_about_the_model_it_will_use() {
        let ctx = WizardContext {
            ollama: running(&["qwen3:4b"]),
            ..Default::default()
        };
        let mut c = choices(&ctx);
        assert!(
            Feature::Rewrite.warning(&ctx, &c).is_none(),
            "follows qwen3:4b"
        );
        c.rewrite_model = Some("phi4".into());
        assert!(Feature::Rewrite
            .warning(&ctx, &c)
            .unwrap()
            .contains("ollama pull phi4"));
    }

    #[test]
    fn tags_are_parsed_from_the_ollama_answer() {
        let json = serde_json::json!({
            "models": [{"name": "llama3.2:latest"}, {"model": "qwen3:4b"}, {"size": 1}]
        });
        assert_eq!(parse_tags(&json), vec!["llama3.2:latest", "qwen3:4b"]);
        assert!(parse_tags(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn models_are_split_into_text_and_embedding() {
        let st = running(&[
            "llama3.2:latest",
            "nomic-embed-text:latest",
            "qwen3-embedding:0.6b",
            "bge-m3:latest",
            "all-minilm:l6-v2",
            "qwen3:4b",
        ]);
        assert_eq!(st.text_models(), vec!["llama3.2:latest", "qwen3:4b"]);
        assert_eq!(st.embedding_models().len(), 4);
    }

    #[test]
    fn model_matching_adds_the_latest_tag() {
        let pulled = vec!["llama3.2:latest".to_string(), "qwen3:4b".to_string()];
        assert!(model_is_pulled("llama3.2", &pulled));
        assert!(model_is_pulled("qwen3:4b", &pulled));
        assert!(!model_is_pulled("qwen3", &pulled));
    }

    #[test]
    fn proposed_models_skip_the_other_kind() {
        let mut ctx = WizardContext {
            ollama: running(&["nomic-embed-text:latest", "qwen3:4b"]),
            ..Default::default()
        };
        assert_eq!(ctx.ai_model(), "qwen3:4b", "never an embedding model");
        assert_eq!(ctx.ollama_embed_model(), "nomic-embed-text:latest");
        ctx.ollama = running(&["qwen3:4b", "llama3.2:latest"]);
        assert_eq!(ctx.ai_model(), DEFAULT_AI_SUMMARY_MODEL);
        assert_eq!(ctx.ollama_embed_model(), SUGGESTED_OLLAMA_EMBED_MODEL);
        ctx.ai_summary_model = Some("mistral".into());
        assert_eq!(ctx.ai_model(), "mistral");
    }

    #[test]
    fn ai_summary_is_proposed_only_with_a_text_model() {
        let mut ctx = WizardContext {
            ollama: running(&["qwen3:4b"]),
            ..Default::default()
        };
        assert!(Feature::AiSummary.proposed(&ctx));
        assert!(!Feature::AiSummary.current(&ctx), "current is the setting");
        ctx.ollama = running(&["nomic-embed-text:latest"]);
        assert!(!Feature::AiSummary.proposed(&ctx));
        ctx.ollama = OllamaStatus::Stopped;
        assert!(!Feature::AiSummary.proposed(&ctx));
        ctx.ai_summary_enabled = true;
        assert!(
            Feature::AiSummary.proposed(&ctx),
            "an enabled setting stays on"
        );
    }

    #[test]
    fn ollama_warnings_say_what_to_do() {
        let mut ctx = WizardContext::default();
        let mut c = choices(&ctx);
        c.ai_model = "llama3.2".into();
        let w = |ctx: &WizardContext, c: &Choices| Feature::AiSummary.warning(ctx, c);
        assert!(w(&ctx, &c).unwrap().contains("https://ollama.com"));
        ctx.ollama = OllamaStatus::Stopped;
        assert!(w(&ctx, &c).unwrap().contains("ollama serve"));
        ctx.ollama = running(&[]);
        assert!(w(&ctx, &c).unwrap().contains("ollama pull llama3.2"));
        ctx.ollama = running(&["qwen3:4b"]);
        c.ai_model = "mistral".into();
        assert!(w(&ctx, &c).unwrap().contains("mistral is not pulled"));
        c.ai_model = "qwen3:4b".into();
        assert!(w(&ctx, &c).is_none());

        c.embed = EmbedChoice::Ollama("nomic-embed-text".into());
        let e = Feature::Embeddings.warning(&ctx, &c).unwrap();
        assert!(e.contains("ollama pull nomic-embed-text"), "{e}");
        c.embed = EmbedChoice::Candle(DEFAULT_CANDLE_EMBED_MODEL.into());
        assert!(Feature::Embeddings.warning(&ctx, &c).is_none());
    }

    #[test]
    fn detection_reports_missing_without_server_nor_binary() {
        // Port 9 (discard) on localhost refuses connections; no PATH.
        let url = "http://127.0.0.1:9";
        let t = Duration::from_millis(300);
        assert_eq!(detect_ollama_with(url, t, None), OllamaStatus::Missing);

        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join(executable_name());
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = std::ffi::OsString::from(dir.path());
        assert_eq!(
            detect_ollama_with(url, t, Some(&path)),
            OllamaStatus::Stopped
        );
        // A non-local URL is never queried.
        assert_eq!(
            detect_ollama_with("http://example.com:11434", t, None),
            OllamaStatus::Missing
        );
    }

    #[test]
    fn binary_lookup_needs_an_executable_file() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("ollama");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        let path = std::ffi::OsString::from(dir.path());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert!(!on_path("ollama", Some(&path)), "not executable yet");
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(on_path("ollama", Some(&path)));
        assert!(!on_path("ollama", None));
    }
}
