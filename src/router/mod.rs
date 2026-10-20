//! Model router: Jev sizes each Claude Code message, and small jobs are
//! handed to a helper agent running on a smaller, cheaper model.
//!
//! Claude Code cannot switch the main session's model per message, so the
//! router works through a `UserPromptSubmit` hook ([`hook`]) that injects a
//! delegation instruction naming one of the helper agents written by
//! [`agents`]. Every failure path injects nothing: the message goes through
//! exactly as if the router weren't there.

pub mod agents;
pub mod card;
pub mod cli;
pub mod dossier;
pub mod hook;
pub mod location;
pub mod outcome;
pub mod policy;
pub mod session;
pub mod stats;

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::config::Settings;
use crate::jev::{Answers, Judge, Question, Questions, Usage};

pub const SIZE_QUESTION: &str = "size";
pub const FOLLOWUP_QUESTION: &str = "needs_conversation";

/// Job sizes, smallest to biggest, each backed by one helper agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Size {
    Tiny,
    Everyday,
    Large,
    Hardest,
}

impl Size {
    pub const ALL: [Size; 4] = [Size::Tiny, Size::Everyday, Size::Large, Size::Hardest];

    pub fn as_str(self) -> &'static str {
        match self {
            Size::Tiny => "tiny",
            Size::Everyday => "everyday",
            Size::Large => "large",
            Size::Hardest => "hardest",
        }
    }

    pub fn parse(s: &str) -> Option<Size> {
        Size::ALL.into_iter().find(|size| size.as_str() == s)
    }

    /// Name of the helper agent (`~/.claude/agents/<name>.md`).
    pub fn agent_name(self) -> &'static str {
        match self {
            Size::Tiny => "router-tiny",
            Size::Everyday => "router-everyday",
            Size::Large => "router-large",
            Size::Hardest => "router-hardest",
        }
    }

    /// Model alias written in the agent's `model:` frontmatter.
    pub fn model_alias(self) -> &'static str {
        match self {
            Size::Tiny => "haiku",
            Size::Everyday => "sonnet",
            Size::Large => "opus",
            Size::Hardest => "fable",
        }
    }

    /// The next tier up, stopping at the top one.
    pub fn raised(self) -> Size {
        match self {
            Size::Tiny => Size::Everyday,
            Size::Everyday => Size::Large,
            Size::Large | Size::Hardest => Size::Hardest,
        }
    }

    pub fn model_label(self) -> &'static str {
        match self {
            Size::Tiny => "Claude Haiku",
            Size::Everyday => "Claude Sonnet",
            Size::Large => "Claude Opus",
            Size::Hardest => "Claude Fable",
        }
    }

    /// What the size covers, shared by the Jev criteria and the agent files.
    pub fn scope(self) -> &'static str {
        match self {
            Size::Tiny => "a quick lookup, a rename, a factual question or a one-line answer",
            Size::Everyday => {
                "a normal email, a social post, a short document, a simple explanation \
                 or a small self-contained code change"
            }
            Size::Large => {
                "a multi-step build, research, a full report, or code changes across \
                 several files"
            }
            Size::Hardest => {
                "strategy, architecture, high-stakes decisions, or anything where a \
                 wrong call is expensive"
            }
        }
    }
}

/// What the router did with one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Decision {
    /// Handed to the helper agent for the chosen size.
    Delegated,
    /// Jev was less sure than `router_min_confidence`: main session handles it.
    SelfUnsure,
    /// A short reply that only makes sense in the conversation.
    SelfFollowup,
    /// Empty message, slash command, `!` shell command, task notification or
    /// subagent hand-back: Jev not asked.
    Skipped,
    /// Jev failed recently; not asked until the marker expires.
    JevDown,
    /// Jev failed or answered unusably for this message.
    Error,
    /// The working folder is a private location: nothing was sent anywhere.
    Private,
    /// Strict mode is on and the working folder is not on the allow list.
    NotAllowed,
    /// The rules chose a route but its helper agent is not usable.
    NoRoute,
}

impl Decision {
    pub const ALL: [Decision; 9] = [
        Decision::Delegated,
        Decision::SelfUnsure,
        Decision::SelfFollowup,
        Decision::Skipped,
        Decision::JevDown,
        Decision::Error,
        Decision::Private,
        Decision::NotAllowed,
        Decision::NoRoute,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Delegated => "delegated",
            Decision::SelfUnsure => "self_unsure",
            Decision::SelfFollowup => "self_followup",
            Decision::Skipped => "skipped",
            Decision::JevDown => "jev_down",
            Decision::Error => "error",
            Decision::Private => "private",
            Decision::NotAllowed => "not_allowed",
            Decision::NoRoute => "no_route",
        }
    }

    pub fn parse(s: &str) -> Option<Decision> {
        Decision::ALL.into_iter().find(|d| d.as_str() == s)
    }
}

/// Outcome of routing one message. `size` and the probabilities are what
/// Jev answered, even when the main session keeps the message.
#[derive(Debug, Clone, PartialEq)]
pub struct Routing {
    pub decision: Decision,
    pub size: Option<Size>,
    pub confidence: Option<f64>,
    pub followup_prob: Option<f64>,
    pub usage: Option<Usage>,
    pub latency_ms: u64,
    pub error: Option<String>,
    /// The failure says the service is down (timeout, transport, 5xx…), so
    /// the hook stops asking for a while.
    pub service_failure: bool,
    /// The task card Jev drew up for this message.
    pub card: Option<card::TaskCard>,
    /// Name of the policy rule that matched.
    pub rule: Option<String>,
    /// Helper agent the rule routed to (`router-everyday`, `router-analyst`…).
    pub route: Option<String>,
    /// The route was raised one tier because the risk answer was unreliable.
    pub raised: bool,
    /// Policy version in force; `None` for rows written by the size-only router.
    pub policy_version: Option<String>,
    /// Opaque Claude Code session id, to match the transcript later.
    pub session_id: Option<String>,
    /// What the matched rule asks for, for the guidance text.
    pub matched: Option<policy::Match>,
}

impl Routing {
    pub fn without_jev(decision: Decision) -> Self {
        Routing {
            decision,
            size: None,
            confidence: None,
            followup_prob: None,
            usage: None,
            latency_ms: 0,
            error: None,
            service_failure: false,
            card: None,
            rule: None,
            route: None,
            raised: false,
            policy_version: None,
            session_id: None,
            matched: None,
        }
    }

    /// Where the message went, as shown in the Jev usage view: the helper
    /// agent when delegated, `self (unsure)` / `self (followup)` when the main
    /// session kept it, nothing for errors and calls that never reached Jev.
    pub fn target_label(&self) -> Option<String> {
        match self.decision {
            Decision::Delegated => self
                .route
                .clone()
                .or_else(|| self.size.map(|s| s.agent_name().to_string())),
            Decision::SelfUnsure => Some("self (unsure)".to_string()),
            Decision::SelfFollowup => Some("self (followup)".to_string()),
            Decision::Skipped
            | Decision::JevDown
            | Decision::Error
            | Decision::Private
            | Decision::NotAllowed
            | Decision::NoRoute => None,
        }
    }
}

/// Messages the router never sends to Jev.
pub fn skip_reason(prompt: &str) -> Option<&'static str> {
    let trimmed = prompt.trim_start();
    if trimmed.is_empty() {
        Some("empty")
    } else if trimmed.starts_with('/') {
        Some("slash command")
    } else if trimmed.starts_with('!') {
        Some("shell command")
    } else if trimmed.starts_with("<task-notification>") {
        Some("task notification")
    } else if (trimmed.starts_with("<agent-message")
        || trimmed.starts_with("Another Claude session sent a message"))
        && trimmed.contains("[Subagent hand-back]")
    {
        Some("subagent hand-back")
    } else {
        None
    }
}

/// The six questions asked in one request, answered in parallel.
pub fn questions() -> Questions {
    use card::{
        Nature, Risk, AMBIGUITY_QUESTION, NATURE_QUESTION, RISK_QUESTION, VERIFIABILITY_QUESTION,
    };
    let mut q = Questions::new();
    q.insert(
        SIZE_QUESTION.into(),
        Question::choice(
            "What is the smallest AI model size that can do the job asked in `message` well? \
             Judge the work needed to produce a good answer, not the length of the message.",
            Size::ALL
                .into_iter()
                .map(|s| (s.as_str(), format!("{}: {}", s.as_str(), s.scope()))),
        ),
    );
    q.insert(
        FOLLOWUP_QUESTION.into(),
        Question::noul_with(
            "Is `message` a short reply that only makes sense inside an ongoing conversation, \
             because it refers to something said earlier (for example 'yes do that but make \
             it shorter' or 'ok go with the second one')?",
            "It is a follow-up that depends on earlier messages and cannot be handled on its own.",
            "It is a self-contained request that can be understood without the conversation.",
        ),
    );
    q.insert(
        NATURE_QUESTION.into(),
        Question::choice(
            "What kind of work does `message` ask for? Pick the one that fits best.",
            Nature::ALL
                .into_iter()
                .map(|n| (n.as_str(), format!("{}: {}", n.as_str(), n.criteria()))),
        ),
    );
    q.insert(
        RISK_QUESTION.into(),
        Question::choice(
            "How much harm could a wrong or careless result of the job in `message` do?",
            Risk::ALL
                .into_iter()
                .map(|r| (r.as_str(), format!("{}: {}", r.as_str(), r.criteria()))),
        ),
    );
    q.insert(
        AMBIGUITY_QUESTION.into(),
        Question::noul_with(
            "Does the job in `message` leave important decisions open: missing requirements, \
             several reasonable interpretations, or choices only the user can make?",
            "Important decisions are missing and a good answer needs options or questions first.",
            "The request is specific enough to be done without further decisions.",
        ),
    );
    q.insert(
        VERIFIABILITY_QUESTION.into(),
        Question::noul_with(
            "Could the result of the job in `message` be checked objectively, by running tests, \
             a build or a command, by looking at it in a browser, or by comparing a concrete state?",
            "The result can be checked objectively.",
            "The result can only be judged by opinion or taste.",
        ),
    );
    q
}

pub fn state_for(message: &str) -> Value {
    json!({ "message": message })
}

/// Turns Jev's answers into a decision under the default policy. A missing
/// or malformed answer is an error, i.e. "act as if the router weren't there".
// Used by the library crate and tests; the binary goes through `decide_with`.
#[allow(dead_code)]
pub fn decide(answers: &Answers, settings: &Settings) -> Routing {
    decide_with(answers, settings, &policy::Policy::default_policy())
}

/// [`decide`] under a given policy (the default rules plus the user's file).
pub fn decide_with(answers: &Answers, settings: &Settings, policy: &policy::Policy) -> Routing {
    use policy::Resolution;

    let followup = answers.noul(FOLLOWUP_QUESTION);
    let card = card::TaskCard::from_answers(answers);
    let mut routing = Routing {
        decision: Decision::Error,
        size: card.as_ref().map(|c| c.size),
        confidence: card.as_ref().map(|c| c.size_confidence),
        followup_prob: followup,
        card: card.clone(),
        policy_version: Some(policy.version().to_string()),
        ..Routing::without_jev(Decision::Error)
    };
    let (Some(followup), Some(card)) = (followup, card) else {
        routing.error = Some("missing or unknown answer".into());
        return routing;
    };
    if followup >= settings.router_followup_min_prob {
        routing.decision = Decision::SelfFollowup;
    } else if card.size_confidence < settings.router_min_confidence {
        routing.decision = Decision::SelfUnsure;
    } else {
        match policy.resolve(&card, settings) {
            Resolution::MainSession => routing.decision = Decision::SelfUnsure,
            Resolution::NoMatch => {
                routing.decision = Decision::NoRoute;
                routing.error = Some("no rule matched".into());
            }
            Resolution::Matched { matched, raised } => {
                routing.decision = Decision::Delegated;
                routing.rule = Some(matched.rule.clone());
                routing.route = Some(matched.agent_name().to_string());
                routing.raised = raised;
                routing.matched = Some(matched);
            }
        }
    }
    routing
}

/// Turns a delegation into [`Decision::NoRoute`] when the chosen helper is
/// not usable: its agent file is missing or shadowed by one the user wrote.
pub fn apply_availability(routing: &mut Routing, agents_dir: Option<&std::path::Path>) {
    if routing.decision != Decision::Delegated {
        return;
    }
    let Some(matched) = &routing.matched else {
        return;
    };
    let kind = match matched.role {
        policy::Role::Analyst => agents::AgentKind::Analyst,
        policy::Role::Producer => agents::AgentKind::Producer(matched.tier),
    };
    let usable = agents_dir.is_some_and(|dir| agents::is_available(dir, kind));
    if !usable {
        routing.decision = Decision::NoRoute;
        routing.error = Some(format!(
            "helper agent `{}` is not available (run `ecotokens router on`)",
            kind.name()
        ));
    }
}

/// Asks Jev about `prompt` and decides under the default policy. Never fails:
/// errors come back as [`Decision::Error`] with the reason, for the caller to
/// record.
// Used by the library crate and tests; the binary goes through `route_with`.
#[allow(dead_code)]
pub fn route(prompt: &str, settings: &Settings, judge: &dyn Judge, timeout: Duration) -> Routing {
    route_with(
        prompt,
        settings,
        judge,
        timeout,
        &policy::Policy::default_policy(),
    )
}

pub fn route_with(
    prompt: &str,
    settings: &Settings,
    judge: &dyn Judge,
    timeout: Duration,
    policy: &policy::Policy,
) -> Routing {
    if skip_reason(prompt).is_some() {
        return Routing::without_jev(Decision::Skipped);
    }
    let start = Instant::now();
    let result = judge.ask_with_usage(state_for(prompt), questions(), timeout);
    let latency_ms = start.elapsed().as_millis() as u64;
    let mut routing = match result {
        Ok((answers, usage)) => {
            let mut r = decide_with(&answers, settings, policy);
            r.usage = usage;
            r
        }
        Err(e) => {
            let mut r = Routing::without_jev(Decision::Error);
            r.error = Some(e.to_string());
            r.service_failure = e.trips_breaker();
            r
        }
    };
    routing.latency_ms = latency_ms;
    routing
}

/// Text injected into the main session for a delegated message: at most six
/// lines, naming the policy, the rule, the helper and what the rule asks for.
/// Never contains any of the user's message.
pub fn delegation_context(routing: &Routing) -> Option<String> {
    if routing.decision != Decision::Delegated {
        return None;
    }
    let size = routing.size?;
    let Some(matched) = &routing.matched else {
        return Some(format!(
            "[ecotokens router] Jev sized this message as `{size}`, a job for {label}. Delegate it \
             to the `{agent}` subagent with the Agent tool. The subagent does not see this \
             conversation: pass the user's message verbatim, plus any context from the \
             conversation it needs. Then relay its answer to the user unchanged, including its \
             final line naming the model. If the message cannot sensibly be delegated (for \
             example it asks about this conversation itself), handle it yourself.",
            size = size.as_str(),
            label = size.model_label(),
            agent = size.agent_name(),
        ));
    };
    let version = routing.policy_version.as_deref().unwrap_or("?");
    let mut lines = vec![
        format!(
            "[ecotokens router] Policy {version}, rule `{rule}`: delegate to the `{agent}` \
             subagent ({label}) with the Agent tool.",
            rule = matched.rule,
            agent = matched.agent_name(),
            label = matched.tier.model_label(),
        ),
        "Pass the user's message verbatim plus the context it needs; it does not see this \
         conversation. Relay its answer unchanged, including its final model line."
            .to_string(),
    ];
    if !matched.hints.is_empty() {
        // A hint is the user's own text, but it is injected into the model's
        // context, so it goes through the masker like everything else.
        let hints = crate::masking::mask(&matched.hints.join("; ")).0;
        lines.push(format!("Hints: {hints}"));
    }
    match matched.review {
        policy::Review::Required => lines.push(
            "After it returns, call `router-reviewer` with the goal, constraints, `git diff` and \
             test output (not its summary); show its verdict unchanged. A rejection does not \
             trigger an automatic second production round."
                .to_string(),
        ),
        policy::Review::Suggested => lines.push(
            "Consider a targeted `router-reviewer` pass on the risky part (goal, constraints, \
             `git diff` and test output, not its summary); optional, show its verdict unchanged."
                .to_string(),
        ),
        policy::Review::None => {}
    }
    if matched.approval == policy::Approval::Irreversible {
        lines
            .push("Ask the user for explicit approval before any irreversible action.".to_string());
    }
    if matched.role == policy::Role::Analyst {
        lines.push(
            "Read-only: ask for constraints, options, trade-offs and acceptance criteria; do not \
             implement."
                .to_string(),
        );
    }
    lines.push("If the message cannot sensibly be delegated, handle it yourself.".to_string());
    Some(lines.join("\n"))
}

/// The router's own judge: built from `router_enabled` (or `force` for
/// `ecotokens router try`), independent of `jev_enabled`.
#[cfg(feature = "jev")]
pub fn judge_for_router(settings: &Settings, force: bool) -> Option<Box<dyn Judge>> {
    crate::jev::judge_from_parts(
        settings.router_enabled || force,
        settings.jev_url.as_deref(),
        std::env::var(crate::jev::API_KEY_ENV).ok(),
        settings.jev_max_input_chars,
    )
}

#[cfg(not(feature = "jev"))]
pub fn judge_for_router(_settings: &Settings, _force: bool) -> Option<Box<dyn Judge>> {
    None
}
