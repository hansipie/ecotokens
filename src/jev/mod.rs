//! TypeSafe Jev integration — fast typed judgments (Choice / Noul / Score)
//! standing in for fragile heuristics in the rewrite gate, rewrite output
//! verification, and generic-filter line selection.
//!
//! Every caller treats Jev as strictly optional: no judge (feature off,
//! `jev_enabled = false`, no `TYPESAFE_API_KEY`) or any `Err` from
//! [`Judge::ask`] means "run the built-in heuristic exactly as before".

// Without the `jev` feature only the fallback paths are reachable.
#![cfg_attr(not(feature = "jev"), allow(dead_code))]

pub mod client;

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::config::Settings;

pub const DEFAULT_URL: &str = "https://api.typesafe.ai/v1/systemone";
pub const API_KEY_ENV: &str = "TYPESAFE_API_KEY";
pub const MODEL: &str = "jev-latest";

/// Descriptions of what a yes and a no mean for a [`Question::Noul`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoulCriteria {
    #[serde(rename = "true")]
    pub yes: String,
    #[serde(rename = "false")]
    pub no: String,
}

/// One question, serialised to the API's `{"type": ...}` shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Question {
    Noul {
        instructions: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        criteria: Option<NoulCriteria>,
    },
    Choice {
        instructions: String,
        criteria: BTreeMap<String, String>,
    },
    #[allow(dead_code)]
    Score {
        instructions: String,
        criteria: Vec<String>,
    },
}

impl Question {
    pub fn noul(instructions: impl Into<String>) -> Self {
        Question::Noul {
            instructions: instructions.into(),
            criteria: None,
        }
    }

    pub fn noul_with(
        instructions: impl Into<String>,
        yes: impl Into<String>,
        no: impl Into<String>,
    ) -> Self {
        Question::Noul {
            instructions: instructions.into(),
            criteria: Some(NoulCriteria {
                yes: yes.into(),
                no: no.into(),
            }),
        }
    }

    pub fn choice<K, V>(
        instructions: impl Into<String>,
        options: impl IntoIterator<Item = (K, V)>,
    ) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        Question::Choice {
            instructions: instructions.into(),
            criteria: options
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }
}

pub type Questions = BTreeMap<String, Question>;

#[derive(Debug, Clone, PartialEq)]
pub struct ChoiceAnswer {
    pub choice: String,
    pub probabilities: HashMap<String, f64>,
    pub confidence: f64,
}

impl ChoiceAnswer {
    /// Probability of `option`, `0.0` when absent from the distribution.
    pub fn prob(&self, option: &str) -> f64 {
        self.probabilities.get(option).copied().unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Noul(f64),
    Choice(ChoiceAnswer),
    Score { score: f64, confidence: f64 },
}

/// Answers keyed by question id. The typed getters return `None` for a
/// missing or wrong-typed answer — callers treat that as "fall back".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Answers(pub HashMap<String, Answer>);

impl Answers {
    pub fn noul(&self, id: &str) -> Option<f64> {
        match self.0.get(id)? {
            Answer::Noul(p) => Some(*p),
            _ => None,
        }
    }

    pub fn choice(&self, id: &str) -> Option<&ChoiceAnswer> {
        match self.0.get(id)? {
            Answer::Choice(c) => Some(c),
            _ => None,
        }
    }

    /// JSON rendering for the debug log.
    fn to_log_value(&self) -> Value {
        let map = self
            .0
            .iter()
            .map(|(id, a)| {
                let v = match a {
                    Answer::Noul(p) => serde_json::json!({ "type": "noul", "prob": p }),
                    Answer::Choice(c) => serde_json::json!({
                        "type": "choice",
                        "choice": c.choice,
                        "probabilities": c.probabilities,
                        "confidence": c.confidence,
                    }),
                    Answer::Score { score, confidence } => serde_json::json!({
                        "type": "score",
                        "score": score,
                        "confidence": confidence,
                    }),
                };
                (id.clone(), v)
            })
            .collect::<serde_json::Map<_, _>>();
        Value::Object(map)
    }

    // Used by the library crate and tests; unused in the binary.
    #[allow(dead_code)]
    pub fn insert(&mut self, id: impl Into<String>, answer: Answer) -> &mut Self {
        self.0.insert(id.into(), answer);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JevError {
    /// Circuit breaker open: an earlier call in this process failed at the
    /// transport level, so the heuristics are used without retrying.
    Unavailable,
    Timeout,
    Transport(String),
    Http(u16),
    BadResponse(String),
}

impl std::fmt::Display for JevError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JevError::Unavailable => {
                write!(f, "Jev disabled for this process after an earlier failure")
            }
            JevError::Timeout => write!(f, "Jev request timed out"),
            JevError::Transport(e) => write!(f, "Jev request failed: {e}"),
            JevError::Http(code) => write!(f, "Jev returned HTTP {code}"),
            JevError::BadResponse(e) => write!(f, "Jev returned an unusable response: {e}"),
        }
    }
}

impl std::error::Error for JevError {}

impl JevError {
    /// Whether this failure says the service is unusable for the rest of the
    /// process (as opposed to a problem with this one request).
    pub fn trips_breaker(&self) -> bool {
        match self {
            JevError::Timeout | JevError::Transport(_) => true,
            JevError::Http(code) => *code != 422,
            JevError::Unavailable | JevError::BadResponse(_) => false,
        }
    }
}

/// Token counts from the response's `usage` object.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Source of typed judgments. Implemented by [`client::HttpJudge`]
/// (production) and [`StubJudge`] (deterministic, for tests).
pub trait Judge: Send + Sync {
    fn ask(
        &self,
        state: Value,
        questions: Questions,
        timeout: Duration,
    ) -> Result<Answers, JevError>;

    /// Like [`Judge::ask`], plus the token usage when the judge reports it
    /// (used for cost tracking). Judges without usage return `None`.
    fn ask_with_usage(
        &self,
        state: Value,
        questions: Questions,
        timeout: Duration,
    ) -> Result<(Answers, Option<Usage>), JevError> {
        self.ask(state, questions, timeout).map(|a| (a, None))
    }
}

/// A judge together with the settings holding its decision thresholds.
#[derive(Clone, Copy)]
pub struct JevContext<'a> {
    pub judge: &'a dyn Judge,
    pub settings: &'a Settings,
}

impl<'a> JevContext<'a> {
    pub fn new(judge: &'a dyn Judge, settings: &'a Settings) -> Self {
        Self { judge, settings }
    }

    /// `jev_timeout_ms`, capped by what remains of a caller's own budget.
    pub fn timeout(&self, remaining: Option<Duration>) -> Duration {
        let own = Duration::from_millis(self.settings.jev_timeout_ms);
        remaining.map_or(own, |r| own.min(r))
    }

    pub fn ask(
        &self,
        state: Value,
        questions: Questions,
        timeout: Duration,
    ) -> Result<Answers, JevError> {
        let logging = self.settings.debug || self.settings.debuglog;
        if !logging {
            return self.judge.ask(state, questions, timeout);
        }
        let logger = crate::debuglog::DebugLogger::new(true);
        let uid = crate::debuglog::gen_uid();
        logger.log(
            &uid,
            "jev",
            "request",
            &serde_json::json!({
                "state": state,
                "questions": questions,
                "timeout_ms": timeout.as_millis() as u64,
            }),
        );
        let start = std::time::Instant::now();
        let result = self.judge.ask(state, questions, timeout);
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let data = match &result {
            Ok(answers) => serde_json::json!({
                "ok": true,
                "elapsed_ms": elapsed_ms,
                "answers": answers.to_log_value(),
            }),
            Err(e) => serde_json::json!({
                "ok": false,
                "elapsed_ms": elapsed_ms,
                "error": e.to_string(),
                "trips_breaker": e.trips_breaker(),
            }),
        };
        logger.log(&uid, "jev", "response", &data);
        result
    }
}

/// Build the production judge when Jev is compiled in, enabled, and has an
/// API key (read from `TYPESAFE_API_KEY` only — never from config). `None`
/// means every caller uses its built-in heuristic.
#[cfg(feature = "jev")]
pub fn judge_from_settings(settings: &Settings) -> Option<Box<dyn Judge>> {
    judge_from_parts(
        settings.jev_enabled,
        settings.jev_url.as_deref(),
        std::env::var(API_KEY_ENV).ok(),
        settings.jev_max_input_chars,
    )
}

#[cfg(not(feature = "jev"))]
pub fn judge_from_settings(_settings: &Settings) -> Option<Box<dyn Judge>> {
    None
}

/// Testable core of [`judge_from_settings`].
#[cfg(feature = "jev")]
pub fn judge_from_parts(
    enabled: bool,
    url: Option<&str>,
    api_key: Option<String>,
    max_input_chars: usize,
) -> Option<Box<dyn Judge>> {
    if !enabled {
        return None;
    }
    let key = api_key.filter(|k| !k.trim().is_empty())?;
    match client::HttpJudge::new(url, key, max_input_chars) {
        Ok(j) => Some(Box::new(j)),
        Err(e) => {
            client::warn_unavailable_once(&e);
            None
        }
    }
}

/// A programmable outcome for one [`StubJudge::ask`] call.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum StubJudgeOutcome {
    Answers(Answers),
    Error(JevError),
}

/// Deterministic test double, always compiled so integration tests in
/// `tests/` can use it (same pattern as `rewrite::provider::StubProvider`).
/// Calls beyond the queued outcomes repeat the last one; an empty queue
/// answers with an error, i.e. "fall back".
#[allow(dead_code)]
pub struct StubJudge {
    queue: Mutex<VecDeque<StubJudgeOutcome>>,
    calls: AtomicUsize,
    last: Mutex<Option<(Value, Questions)>>,
}

impl Default for StubJudge {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl StubJudge {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            calls: AtomicUsize::new(0),
            last: Mutex::new(None),
        }
    }

    pub fn with_answers(answers: Answers) -> Self {
        let s = Self::new();
        s.push(StubJudgeOutcome::Answers(answers));
        s
    }

    pub fn failing(err: JevError) -> Self {
        let s = Self::new();
        s.push(StubJudgeOutcome::Error(err));
        s
    }

    pub fn push(&self, outcome: StubJudgeOutcome) {
        self.queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(outcome);
    }

    pub fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    /// State and questions of the most recent call.
    pub fn last_request(&self) -> Option<(Value, Questions)> {
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Judge for StubJudge {
    fn ask(
        &self,
        state: Value,
        questions: Questions,
        _timeout: Duration,
    ) -> Result<Answers, JevError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = Some((state, questions));
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        let outcome = if queue.len() > 1 {
            queue.pop_front()
        } else {
            queue.front().cloned()
        };
        match outcome {
            Some(StubJudgeOutcome::Answers(a)) => Ok(a),
            Some(StubJudgeOutcome::Error(e)) => Err(e),
            None => Err(JevError::BadResponse("no stub outcome queued".into())),
        }
    }
}
