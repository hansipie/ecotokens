//! Routing policy: an ordered list of rules that maps a task card to a route.
//!
//! The default rules ship embedded in the binary (`policy_default.toml`); one
//! optional user file (`~/.config/ecotokens/router_rules.toml`) adds rules
//! that are evaluated first. Each user rule is validated on its own: an
//! invalid rule is dropped and reported, never allowed to break routing.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::card::{Fallback, Nature, Risk, TaskCard};
use super::Size;
use crate::config::Settings;

pub const DEFAULT_VERSION: &str = "2026.1";
const DEFAULT_RULES: &str = include_str!("policy_default.toml");

pub const MAX_RULES: usize = 100;
pub const MAX_HINTS: usize = 3;
pub const MAX_HINT_CHARS: usize = 160;
pub const MAX_NAME_CHARS: usize = 48;

/// Name of the rule that routes by size alone.
pub const SIZE_ONLY_RULE: &str = "size-only";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Producer,
    Analyst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Review {
    None,
    Suggested,
    Required,
}

impl Review {
    pub fn as_str(self) -> &'static str {
        match self {
            Review::None => "none",
            Review::Suggested => "suggested",
            Review::Required => "required",
        }
    }

    fn parse(s: &str) -> Option<Review> {
        [Review::None, Review::Suggested, Review::Required]
            .into_iter()
            .find(|r| r.as_str() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Approval {
    None,
    Irreversible,
}

impl Approval {
    pub fn as_str(self) -> &'static str {
        match self {
            Approval::None => "none",
            Approval::Irreversible => "irreversible",
        }
    }

    fn parse(s: &str) -> Option<Approval> {
        [Approval::None, Approval::Irreversible]
            .into_iter()
            .find(|a| a.as_str() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Bound {
    Exact,
    AtLeast,
}

impl Bound {
    pub fn as_str(self) -> &'static str {
        match self {
            Bound::Exact => "exact",
            Bound::AtLeast => "at-least",
        }
    }
}

/// Where a rule sends a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// A fixed tier (subject to the rule's [`Bound`]).
    Tier(Size),
    /// The tier of the size answer.
    BySize,
    /// The read-only analyst.
    Analyst,
}

impl Route {
    pub fn as_str(self) -> &'static str {
        match self {
            Route::Tier(size) => size.as_str(),
            Route::BySize => "size",
            Route::Analyst => "analyst",
        }
    }

    fn parse(s: &str) -> Option<Route> {
        match s {
            "size" => Some(Route::BySize),
            "analyst" => Some(Route::Analyst),
            other => Size::parse(other).map(Route::Tier),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    High,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    User,
    Default,
}

/// Conditions over the card; every given condition must hold.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct When {
    pub size: Vec<Size>,
    pub nature: Vec<Nature>,
    pub risk: Vec<Risk>,
    pub ambiguity: Option<Level>,
    pub verifiable: Option<bool>,
}

impl When {
    /// JSON shape used by `router rules --json`.
    pub fn to_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        if !self.size.is_empty() {
            m.insert(
                "size".into(),
                self.size.iter().map(|s| s.as_str()).collect(),
            );
        }
        if !self.nature.is_empty() {
            m.insert(
                "nature".into(),
                self.nature.iter().map(|n| n.as_str()).collect(),
            );
        }
        if !self.risk.is_empty() {
            m.insert(
                "risk".into(),
                self.risk.iter().map(|r| r.as_str()).collect(),
            );
        }
        if let Some(level) = self.ambiguity {
            m.insert(
                "ambiguity".into(),
                match level {
                    Level::High => "high",
                    Level::Low => "low",
                }
                .into(),
            );
        }
        if let Some(v) = self.verifiable {
            m.insert("verifiable".into(), if v { "yes" } else { "no" }.into());
        }
        serde_json::Value::Object(m)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub name: String,
    pub when: When,
    pub route: Route,
    pub bound: Bound,
    pub review: Review,
    pub approval: Approval,
    pub hints: Vec<String>,
    pub source: Source,
}

/// A user rule that was dropped, with the reason.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InvalidRule {
    pub name: Option<String>,
    pub reason: String,
}

/// What a card resolved to: the rule and everything the guidance needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub rule: String,
    pub source: Source,
    /// Tier of the helper (the analyst runs on the `large` tier).
    pub tier: Size,
    pub role: Role,
    pub review: Review,
    pub approval: Approval,
    pub hints: Vec<String>,
}

impl Match {
    /// Name of the helper agent to delegate to.
    pub fn agent_name(&self) -> &'static str {
        match self.role {
            Role::Analyst => "router-analyst",
            Role::Producer => self.tier.agent_name(),
        }
    }
}

/// Result of resolving a card.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// The size is unreliable: the main session keeps the message.
    MainSession,
    Matched {
        matched: Match,
        /// The route was raised one tier because the risk was unreliable.
        raised: bool,
    },
    /// No rule matched (only possible without the default rules).
    NoMatch,
}

/// The card as the rules see it.
struct EvalCard {
    size: Size,
    nature: Option<Nature>,
    risk: Option<Risk>,
    ambiguity_high: Option<bool>,
    verifiable: Option<bool>,
}

impl EvalCard {
    fn new(card: &TaskCard, settings: &Settings, use_nature: bool) -> EvalCard {
        let nature = if use_nature {
            card.nature.map(|n| n.value)
        } else {
            None
        };
        // A sensitive nature is never judged below a sensitive risk, so one
        // rule on the risk covers both.
        let risk = card.risk.map(|r| match nature {
            Some(Nature::Sensitive) => r.value.max(Risk::Sensitive),
            _ => r.value,
        });
        EvalCard {
            size: card.size,
            nature,
            risk,
            ambiguity_high: card
                .ambiguity
                .map(|a| a >= settings.router_ambiguity_min_prob),
            verifiable: card
                .verifiability
                .map(|v| v >= settings.router_verifiable_min_prob),
        }
    }
}

impl When {
    fn matches(&self, card: &EvalCard) -> bool {
        (self.size.is_empty() || self.size.contains(&card.size))
            && (self.nature.is_empty() || card.nature.is_some_and(|n| self.nature.contains(&n)))
            && (self.risk.is_empty() || card.risk.is_some_and(|r| self.risk.contains(&r)))
            && self.ambiguity.map_or(true, |level| {
                card.ambiguity_high
                    .is_some_and(|high| high == (level == Level::High))
            })
            && self
                .verifiable
                .map_or(true, |want| card.verifiable == Some(want))
    }
}

impl Rule {
    fn to_match(&self, size: Size) -> Match {
        let (tier, role) = match self.route {
            Route::Analyst => (Size::Large, Role::Analyst),
            Route::BySize => (size, Role::Producer),
            Route::Tier(tier) => (
                match self.bound {
                    Bound::Exact => tier,
                    Bound::AtLeast => tier.max(size),
                },
                Role::Producer,
            ),
        };
        Match {
            rule: self.name.clone(),
            source: self.source,
            tier,
            role,
            review: self.review,
            approval: self.approval,
            hints: self.hints.clone(),
        }
    }
}

/// FNV-1a, 64 bits. Local and stable across Rust releases (unlike
/// `DefaultHasher`), so a policy version never changes by accident.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The rules in evaluation order plus what was ignored while loading them.
#[derive(Debug, Clone, PartialEq)]
pub struct Policy {
    version: String,
    rules: Vec<Rule>,
    invalid: Vec<InvalidRule>,
    /// Warnings about the user file as a whole (not valid TOML…).
    warnings: Vec<String>,
    user_file: bool,
}

impl Policy {
    /// The shipped rules only.
    pub fn default_policy() -> Policy {
        let (rules, invalid) = parse_rules(DEFAULT_RULES, Source::Default);
        debug_assert!(invalid.is_empty(), "embedded policy is valid: {invalid:?}");
        Policy {
            version: DEFAULT_VERSION.to_string(),
            rules,
            invalid: Vec::new(),
            warnings: Vec::new(),
            user_file: false,
        }
    }

    /// Where the user's rules live.
    pub fn user_rules_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("ecotokens").join("router_rules.toml"))
    }

    /// Default rules plus the user's file, when there is one.
    pub fn load() -> Policy {
        match Self::user_rules_path() {
            Some(path) => Self::load_from(&path),
            None => Self::default_policy(),
        }
    }

    pub fn load_from(path: &Path) -> Policy {
        match std::fs::read(path) {
            Ok(bytes) => match String::from_utf8(bytes.clone()) {
                Ok(text) => Self::build(&text, fnv1a64(&bytes)),
                Err(_) => {
                    let mut policy = Self::default_policy();
                    policy
                        .warnings
                        .push(format!("{} is not valid UTF-8; ignored", path.display()));
                    policy
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default_policy(),
            Err(e) => {
                let mut policy = Self::default_policy();
                policy
                    .warnings
                    .push(format!("cannot read {}: {e}; ignored", path.display()));
                policy
            }
        }
    }

    /// Default rules plus user rules given as TOML text (tests, `router rules`).
    // Used by the library crate and tests; the binary reads the file.
    #[allow(dead_code)]
    pub fn from_user_toml(text: &str) -> Policy {
        Self::build(text, fnv1a64(text.as_bytes()))
    }

    fn build(text: &str, hash: u64) -> Policy {
        let mut policy = Self::default_policy();
        let table = match text.parse::<toml::Table>() {
            Ok(t) => t,
            Err(e) => {
                policy.warnings.push(format!(
                    "router_rules.toml is not valid TOML ({}); ignored",
                    e.message()
                ));
                return policy;
            }
        };
        let (user, invalid) = parse_table(&table, Source::User);
        policy.version = format!("{DEFAULT_VERSION}+u{}", &format!("{hash:016x}")[..8]);
        policy.user_file = true;
        policy.invalid = invalid;
        let mut rules = user;
        rules.append(&mut policy.rules);
        policy.rules = rules;
        policy
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    /// Rules in evaluation order: user rules, then the defaults.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// User rules that were dropped.
    pub fn invalid(&self) -> &[InvalidRule] {
        &self.invalid
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn has_user_file(&self) -> bool {
        self.user_file
    }

    /// Every warning to show, whole-file ones first.
    pub fn all_warnings(&self) -> Vec<String> {
        self.warnings
            .iter()
            .cloned()
            .chain(self.invalid.iter().map(|i| match &i.name {
                Some(name) => format!("rule `{name}` ignored: {}", i.reason),
                None => format!("a rule was ignored: {}", i.reason),
            }))
            .collect()
    }

    /// Resolves a card: card reliability first (FR-005), then the rules.
    pub fn resolve(&self, card: &TaskCard, settings: &Settings) -> Resolution {
        match card.fallback(settings.card_min_confidence()) {
            Fallback::MainSession => Resolution::MainSession,
            Fallback::Raise => Resolution::Matched {
                matched: size_only_match(card.size.raised()),
                raised: true,
            },
            Fallback::SizeOnly => self.evaluate(&EvalCard::new(card, settings, false)),
            Fallback::Rules => self.evaluate(&EvalCard::new(card, settings, true)),
        }
    }

    fn evaluate(&self, card: &EvalCard) -> Resolution {
        match self.rules.iter().find(|r| r.when.matches(card)) {
            Some(rule) => Resolution::Matched {
                matched: rule.to_match(card.size),
                raised: false,
            },
            None => Resolution::NoMatch,
        }
    }
}

fn size_only_match(tier: Size) -> Match {
    Match {
        rule: SIZE_ONLY_RULE.to_string(),
        source: Source::Default,
        tier,
        role: Role::Producer,
        review: Review::None,
        approval: Approval::None,
        hints: Vec::new(),
    }
}

fn parse_rules(text: &str, source: Source) -> (Vec<Rule>, Vec<InvalidRule>) {
    match text.parse::<toml::Table>() {
        Ok(table) => parse_table(&table, source),
        Err(e) => (
            Vec::new(),
            vec![InvalidRule {
                name: None,
                reason: e.message().to_string(),
            }],
        ),
    }
}

fn parse_table(table: &toml::Table, source: Source) -> (Vec<Rule>, Vec<InvalidRule>) {
    let mut rules = Vec::new();
    let mut invalid = Vec::new();
    let Some(entries) = table.get("rule") else {
        return (rules, invalid);
    };
    let Some(entries) = entries.as_array() else {
        invalid.push(InvalidRule {
            name: None,
            reason: "`rule` must be an array of tables ([[rule]])".into(),
        });
        return (rules, invalid);
    };
    let mut seen = HashSet::new();
    for (i, entry) in entries.iter().enumerate() {
        let name = entry
            .get("name")
            .and_then(|n| n.as_str())
            .map(str::to_string);
        let result = if i >= MAX_RULES {
            Err(format!("too many rules (limit {MAX_RULES})"))
        } else {
            parse_rule(entry, source).and_then(|rule| {
                if seen.insert(rule.name.clone()) {
                    Ok(rule)
                } else {
                    Err(format!("duplicate name `{}`", rule.name))
                }
            })
        };
        match result {
            Ok(rule) => rules.push(rule),
            Err(reason) => invalid.push(InvalidRule { name, reason }),
        }
    }
    (rules, invalid)
}

const RULE_KEYS: [&str; 7] = [
    "name", "route", "bound", "review", "approval", "hints", "when",
];
const WHEN_KEYS: [&str; 5] = ["size", "nature", "risk", "ambiguity", "verifiable"];

fn parse_rule(value: &toml::Value, source: Source) -> Result<Rule, String> {
    let table = value.as_table().ok_or("a rule must be a table")?;
    for key in table.keys() {
        if !RULE_KEYS.contains(&key.as_str()) {
            return Err(format!("unknown key `{key}`"));
        }
    }
    let name = string_key(table, "name")?.ok_or("missing `name`")?;
    if name.is_empty()
        || name.chars().count() > MAX_NAME_CHARS
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(format!(
            "invalid name `{name}` (1-{MAX_NAME_CHARS} characters of a-z, 0-9 and -)"
        ));
    }
    let route_text = string_key(table, "route")?.ok_or("missing `route`")?;
    let route = Route::parse(&route_text).ok_or_else(|| {
        format!("unknown route `{route_text}` (tiny, everyday, large, hardest, size or analyst)")
    })?;
    let bound = match string_key(table, "bound")?.as_deref() {
        None | Some("exact") => Bound::Exact,
        Some("at-least") => Bound::AtLeast,
        Some(other) => return Err(format!("unknown bound `{other}` (exact or at-least)")),
    };
    let review = match string_key(table, "review")? {
        None => Review::None,
        Some(s) => Review::parse(&s)
            .ok_or_else(|| format!("unknown review `{s}` (none, suggested or required)"))?,
    };
    let approval = match string_key(table, "approval")? {
        None => Approval::None,
        Some(s) => Approval::parse(&s)
            .ok_or_else(|| format!("unknown approval `{s}` (none or irreversible)"))?,
    };
    let hints = parse_hints(table)?;
    let when = match table.get("when") {
        None => When::default(),
        Some(v) => parse_when(v)?,
    };

    if route == Route::Analyst && bound == Bound::AtLeast {
        return Err("`bound = \"at-least\"` is not valid with the analyst".into());
    }
    if route == Route::Analyst && review != Review::None {
        return Err(
            "the analyst produces options and is not reviewed (review must be none)".into(),
        );
    }
    if review == Review::Required
        && !when
            .risk
            .iter()
            .any(|r| matches!(r, Risk::Sensitive | Risk::Destructive))
    {
        return Err(
            "`review = \"required\"` needs `when.risk` to include sensitive or destructive".into(),
        );
    }
    Ok(Rule {
        name,
        when,
        route,
        bound,
        review,
        approval,
        hints,
        source,
    })
}

fn string_key(table: &toml::Table, key: &str) -> Result<Option<String>, String> {
    match table.get(key) {
        None => Ok(None),
        Some(v) => v
            .as_str()
            .map(|s| Some(s.to_string()))
            .ok_or_else(|| format!("`{key}` must be a string")),
    }
}

fn parse_hints(table: &toml::Table) -> Result<Vec<String>, String> {
    let Some(value) = table.get("hints") else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or("`hints` must be an array of strings")?;
    if items.len() > MAX_HINTS {
        return Err(format!("too many hints (limit {MAX_HINTS})"));
    }
    items
        .iter()
        .map(|h| {
            let text = h.as_str().ok_or("`hints` must be an array of strings")?;
            if text.chars().count() > MAX_HINT_CHARS {
                Err(format!("a hint is longer than {MAX_HINT_CHARS} characters"))
            } else {
                Ok(text.to_string())
            }
        })
        .collect()
}

fn parse_list<T>(
    table: &toml::Table,
    key: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Vec<T>, String> {
    let Some(value) = table.get(key) else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or_else(|| format!("`when.{key}` must be an array"))?;
    items
        .iter()
        .map(|v| {
            v.as_str()
                .and_then(&parse)
                .ok_or_else(|| format!("unknown value in `when.{key}`"))
        })
        .collect()
}

fn parse_when(value: &toml::Value) -> Result<When, String> {
    let table = value.as_table().ok_or("`when` must be a table")?;
    for key in table.keys() {
        if !WHEN_KEYS.contains(&key.as_str()) {
            return Err(format!("unknown key `when.{key}`"));
        }
    }
    let ambiguity = match string_key(table, "ambiguity")?.as_deref() {
        None => None,
        Some("high") => Some(Level::High),
        Some("low") => Some(Level::Low),
        Some(other) => return Err(format!("unknown `when.ambiguity` `{other}` (high or low)")),
    };
    let verifiable = match string_key(table, "verifiable")?.as_deref() {
        None => None,
        Some("yes") => Some(true),
        Some("no") => Some(false),
        Some(other) => return Err(format!("unknown `when.verifiable` `{other}` (yes or no)")),
    };
    Ok(When {
        size: parse_list(table, "size", Size::parse)?,
        nature: parse_list(table, "nature", Nature::parse)?,
        risk: parse_list(table, "risk", Risk::parse)?,
        ambiguity,
        verifiable,
    })
}
