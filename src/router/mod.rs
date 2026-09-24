//! Model router: Jev sizes each Claude Code message, and small jobs are
//! handed to a helper agent running on a smaller, cheaper model.
//!
//! Claude Code cannot switch the main session's model per message, so the
//! router works through a `UserPromptSubmit` hook ([`hook`]) that injects a
//! delegation instruction naming one of the helper agents written by
//! [`agents`]. Every failure path injects nothing: the message goes through
//! exactly as if the router weren't there.

pub mod agents;
pub mod cli;
pub mod hook;
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

    pub fn model_label(self) -> &'static str {
        match self {
            Size::Tiny => "Claude Haiku 4.5",
            Size::Everyday => "Claude Sonnet 5",
            Size::Large => "Claude Opus 5.5",
            Size::Hardest => "Claude Fable 5.1",
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
    /// Empty message, slash command or `!` shell command: Jev not asked.
    Skipped,
    /// Jev failed recently; not asked until the marker expires.
    JevDown,
    /// Jev failed or answered unusably for this message.
    Error,
}

impl Decision {
    pub const ALL: [Decision; 6] = [
        Decision::Delegated,
        Decision::SelfUnsure,
        Decision::SelfFollowup,
        Decision::Skipped,
        Decision::JevDown,
        Decision::Error,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Delegated => "delegated",
            Decision::SelfUnsure => "self_unsure",
            Decision::SelfFollowup => "self_followup",
            Decision::Skipped => "skipped",
            Decision::JevDown => "jev_down",
            Decision::Error => "error",
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
}

impl Routing {
    pub(crate) fn without_jev(decision: Decision) -> Self {
        Routing {
            decision,
            size: None,
            confidence: None,
            followup_prob: None,
            usage: None,
            latency_ms: 0,
            error: None,
            service_failure: false,
        }
    }

    /// Where the message went, as shown in the Jev usage view: the helper
    /// agent when delegated, `self (unsure)` / `self (followup)` when the main
    /// session kept it, nothing for errors and calls that never reached Jev.
    pub fn target_label(&self) -> Option<String> {
        match self.decision {
            Decision::Delegated => self.size.map(|s| s.agent_name().to_string()),
            Decision::SelfUnsure => Some("self (unsure)".to_string()),
            Decision::SelfFollowup => Some("self (followup)".to_string()),
            Decision::Skipped | Decision::JevDown | Decision::Error => None,
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
    } else {
        None
    }
}

/// The two questions asked in one request, answered in parallel.
pub fn questions() -> Questions {
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
    q
}

pub fn state_for(message: &str) -> Value {
    json!({ "message": message })
}

/// Turns Jev's answers into a decision. A missing or malformed answer is an
/// error, i.e. "act as if the router weren't there".
pub fn decide(answers: &Answers, settings: &Settings) -> Routing {
    let followup = answers.noul(FOLLOWUP_QUESTION);
    let choice = answers.choice(SIZE_QUESTION);
    let size = choice.and_then(|c| Size::parse(&c.choice));
    let mut routing = Routing {
        decision: Decision::Error,
        size,
        confidence: choice.map(|c| c.confidence),
        followup_prob: followup,
        usage: None,
        latency_ms: 0,
        error: None,
        service_failure: false,
    };
    let (Some(followup), Some(confidence), Some(_)) = (followup, routing.confidence, size) else {
        routing.error = Some("missing or unknown answer".into());
        return routing;
    };
    routing.decision = if followup >= settings.router_followup_min_prob {
        Decision::SelfFollowup
    } else if confidence < settings.router_min_confidence {
        Decision::SelfUnsure
    } else {
        Decision::Delegated
    };
    routing
}

/// Asks Jev about `prompt` and decides. Never fails: errors come back as
/// [`Decision::Error`] with the reason, for the caller to record.
pub fn route(prompt: &str, settings: &Settings, judge: &dyn Judge, timeout: Duration) -> Routing {
    if skip_reason(prompt).is_some() {
        return Routing::without_jev(Decision::Skipped);
    }
    let start = Instant::now();
    let result = judge.ask_with_usage(state_for(prompt), questions(), timeout);
    let latency_ms = start.elapsed().as_millis() as u64;
    let mut routing = match result {
        Ok((answers, usage)) => {
            let mut r = decide(&answers, settings);
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

/// Text injected into the main session for a delegated message.
pub fn delegation_context(routing: &Routing) -> Option<String> {
    if routing.decision != Decision::Delegated {
        return None;
    }
    let size = routing.size?;
    let confidence = routing.confidence.unwrap_or(0.0);
    Some(format!(
        "[ecotokens router] Jev sized this message as `{size}` (p={confidence:.2}), a job for \
         {label}. Delegate it to the `{agent}` subagent with the Agent tool. The subagent does \
         not see this conversation: pass the user's message verbatim, plus any context from the \
         conversation it needs. Then relay its answer to the user unchanged, including its final \
         line naming the model. If the message cannot sensibly be delegated (for example it asks \
         about this conversation itself), handle it yourself.",
        size = size.as_str(),
        label = size.model_label(),
        agent = size.agent_name(),
    ))
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
