use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::{json, Value};

use super::{Answer, Answers, ChoiceAnswer, JevError, Judge, Questions, DEFAULT_URL, MODEL};

/// Production judge: one blocking HTTPS request per [`Judge::ask`], no
/// retries (hook latency budget). Every string in `state` is masked and the
/// total size capped before anything leaves the machine.
pub struct HttpJudge {
    url: String,
    api_key: String,
    max_input_chars: usize,
}

// Hand-written so the API key never ends up in a debug print.
impl std::fmt::Debug for HttpJudge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpJudge")
            .field("url", &self.url)
            .field("api_key", &"<redacted>")
            .field("max_input_chars", &self.max_input_chars)
            .finish()
    }
}

impl HttpJudge {
    /// Rejects anything but an `https://` URL before any request is built —
    /// the API key travels in a header.
    pub fn new(url: Option<&str>, api_key: String, max_input_chars: usize) -> Result<Self, String> {
        let url = url.unwrap_or(DEFAULT_URL).to_string();
        let parsed = url
            .parse::<reqwest::Url>()
            .map_err(|e| format!("invalid Jev URL: {e}"))?;
        if parsed.scheme() != "https" {
            return Err(format!("Jev URL must use https, got: {}", parsed.scheme()));
        }
        Ok(Self {
            url,
            api_key,
            max_input_chars,
        })
    }
}

/// Process-wide circuit breaker: once a transport-level failure happens, the
/// rest of the process uses the heuristics without paying the timeout again.
static BREAKER_OPEN: AtomicBool = AtomicBool::new(false);
static UNAVAILABLE_WARNED: AtomicBool = AtomicBool::new(false);

/// Prints the fallback warning to stderr at most once per process.
pub fn warn_unavailable_once(message: &str) -> bool {
    let first = UNAVAILABLE_WARNED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok();
    if first {
        eprintln!("ecotokens: warning: Jev unavailable, using built-in heuristics: {message}");
    }
    first
}

impl Judge for HttpJudge {
    fn ask(
        &self,
        state: Value,
        questions: Questions,
        timeout: Duration,
    ) -> Result<Answers, JevError> {
        if BREAKER_OPEN.load(Ordering::SeqCst) {
            return Err(JevError::Unavailable);
        }
        let result = self.send(state, &questions, timeout);
        if let Err(e) = &result {
            if e.trips_breaker() {
                BREAKER_OPEN.store(true, Ordering::SeqCst);
            }
            warn_unavailable_once(&e.to_string());
        }
        result
    }
}

impl HttpJudge {
    fn send(
        &self,
        state: Value,
        questions: &Questions,
        timeout: Duration,
    ) -> Result<Answers, JevError> {
        let body = build_request(&prepare_state(state, self.max_input_chars), questions);
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| JevError::Transport(e.to_string()))?;
        let response = client
            .post(&self.url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    JevError::Timeout
                } else {
                    JevError::Transport(e.to_string())
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(JevError::Http(status.as_u16()));
        }
        let text = response.text().map_err(|e| {
            if e.is_timeout() {
                JevError::Timeout
            } else {
                JevError::Transport(e.to_string())
            }
        })?;
        let answers = parse_response(&text)?;
        // A missing answer would silently turn into "fall back" at the call
        // site anyway; reporting it here keeps the warning meaningful.
        for id in questions.keys() {
            if !answers.0.contains_key(id) {
                return Err(JevError::BadResponse(format!("missing answer '{id}'")));
            }
        }
        Ok(answers)
    }
}

pub fn build_request(state: &Value, questions: &Questions) -> Value {
    json!({
        "model": MODEL,
        "state": state,
        "questions": questions,
    })
}

/// Mask every string leaf of `state`, then shrink leaves proportionally so
/// the total stays within `max_chars`. Shrinking keeps a head, middle, and
/// tail sample of each string instead of cutting it off.
pub fn prepare_state(state: Value, max_chars: usize) -> Value {
    let masked = map_strings(state, &|s| crate::masking::mask(s).0);
    let total = total_string_chars(&masked);
    if total <= max_chars || total == 0 {
        return masked;
    }
    map_strings(masked, &|s| {
        let len = s.chars().count();
        let budget = (len as f64 * max_chars as f64 / total as f64).floor() as usize;
        sample_text(s, budget)
    })
}

fn map_strings(value: Value, f: &dyn Fn(&str) -> String) -> Value {
    match value {
        Value::String(s) => Value::String(f(&s)),
        Value::Array(items) => Value::Array(items.into_iter().map(|v| map_strings(v, f)).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, map_strings(v, f)))
                .collect(),
        ),
        other => other,
    }
}

fn total_string_chars(value: &Value) -> usize {
    match value {
        Value::String(s) => s.chars().count(),
        Value::Array(items) => items.iter().map(total_string_chars).sum(),
        Value::Object(map) => map.values().map(total_string_chars).sum(),
        _ => 0,
    }
}

const ELISION: &str = "\n[…]\n";

/// Keep ~40% head, ~20% middle, ~40% tail of `s` within `budget` chars.
pub fn sample_text(s: &str, budget: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= budget {
        return s.to_string();
    }
    let overhead = ELISION.chars().count() * 2;
    if budget <= overhead + 3 {
        return chars.iter().take(budget).collect();
    }
    let usable = budget - overhead;
    let head = usable * 2 / 5;
    let middle = usable / 5;
    let tail = usable - head - middle;
    let mid_start = chars.len() / 2 - middle / 2;
    let mut out = String::with_capacity(budget * 4);
    out.extend(&chars[..head]);
    out.push_str(ELISION);
    out.extend(&chars[mid_start..mid_start + middle]);
    out.push_str(ELISION);
    out.extend(&chars[chars.len() - tail..]);
    out
}

pub fn parse_response(body: &str) -> Result<Answers, JevError> {
    let json: Value =
        serde_json::from_str(body).map_err(|e| JevError::BadResponse(e.to_string()))?;
    let answers = json
        .get("answers")
        .and_then(Value::as_object)
        .ok_or_else(|| JevError::BadResponse("missing 'answers' object".into()))?;

    let mut out = Answers::default();
    for (id, a) in answers {
        let parsed = match a.get("type").and_then(Value::as_str) {
            Some("noul") => a.get("noul").and_then(Value::as_f64).map(Answer::Noul),
            Some("choice") => parse_choice(a).map(Answer::Choice),
            Some("score") => a
                .get("score")
                .and_then(Value::as_f64)
                .map(|score| Answer::Score {
                    score,
                    confidence: a.get("confidence").and_then(Value::as_f64).unwrap_or(0.0),
                }),
            _ => None,
        };
        match parsed {
            Some(answer) => {
                out.0.insert(id.clone(), answer);
            }
            None => {
                return Err(JevError::BadResponse(format!(
                    "malformed answer for '{id}'"
                )))
            }
        }
    }
    Ok(out)
}

fn parse_choice(a: &Value) -> Option<ChoiceAnswer> {
    let choice = a.get("choice")?.as_str()?.to_string();
    let probabilities: HashMap<String, f64> = a
        .get("probabilities")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_f64().map(|p| (k.clone(), p)))
                .collect()
        })
        .unwrap_or_default();
    let confidence = a
        .get("confidence")
        .and_then(Value::as_f64)
        .unwrap_or_else(|| probabilities.get(&choice).copied().unwrap_or(0.0));
    Some(ChoiceAnswer {
        choice,
        probabilities,
        confidence,
    })
}
