use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use super::ProviderError;

/// Generates a transformation for a prompt. Implemented by [`OllamaProvider`]
/// (production) and [`StubProvider`] (deterministic, for tests — model output
/// is not reproducible run to run, so it is the only way to test this feature
/// per Constitution Principle III; research.md §1).
pub trait RewriteProvider: Send + Sync {
    fn generate(&self, prompt: &str, timeout: Duration) -> Result<String, ProviderError>;
}

const DEFAULT_REWRITE_URL: &str = "http://localhost:11434";

/// Reuses the request shape and SSRF guard proven in
/// `src/filter/ai_summary.rs:47-57`, but with sampling parameters suited to
/// rewriting rather than summarization (research.md §2).
#[derive(Debug)]
pub struct OllamaProvider {
    url: String,
    model: String,
}

impl OllamaProvider {
    /// Validates that `url` resolves to the local machine before storing it
    /// (FR-019). Reject anything else before any request is ever built.
    pub fn new(url: Option<&str>, model: String) -> Result<Self, String> {
        let url = url.unwrap_or(DEFAULT_REWRITE_URL).to_string();
        let parsed = url
            .parse::<reqwest::Url>()
            .map_err(|e| format!("invalid rewrite URL: {e}"))?;
        let host = parsed.host_str().unwrap_or("");
        if !matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]") {
            return Err(format!("rewrite URL must point to localhost, got: {host}"));
        }
        Ok(Self { url, model })
    }
}

impl RewriteProvider for OllamaProvider {
    fn generate(&self, prompt: &str, timeout: Duration) -> Result<String, ProviderError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

        // num_predict scales with prompt length so real paragraphs are not
        // truncated (research.md §2) — ai_summary's num_predict: 500 is wrong
        // here, it exists to force brevity, the opposite of this feature's goal.
        let approx_prompt_tokens = crate::tokens::count_tokens(prompt) as i64;
        let num_predict = (approx_prompt_tokens * 2).clamp(256, 4096);

        let payload = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
            // Reasoning-capable models (e.g. qwen3.5, deepseek-r1) otherwise
            // spend the whole `num_predict` budget on a hidden `thinking`
            // field and never emit `response` at all — observed in practice:
            // an empty `response` with `done_reason: "length"` even though
            // the model "worked", just never got to the answer. `think:
            // false` is a no-op for models without a thinking mode.
            "think": false,
            "options": {
                "temperature": 0.7,
                "num_predict": num_predict,
            }
        });

        let response = client
            .post(format!("{}/api/generate", self.url.trim_end_matches('/')))
            .json(&payload)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout
                } else {
                    ProviderError::RequestFailed(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            return Err(ProviderError::RequestFailed(format!(
                "Ollama returned status {}",
                response.status()
            )));
        }

        let json: serde_json::Value = response
            .json()
            .map_err(|e| ProviderError::BadResponse(e.to_string()))?;

        let text = json["response"]
            .as_str()
            .ok_or_else(|| ProviderError::BadResponse("missing 'response' field".into()))?
            .to_string();

        Ok(text)
    }
}

/// A programmable outcome for one [`StubProvider::generate`] call. The binary
/// crate never constructs one — hence `#[allow(dead_code)]`.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum StubOutcome {
    Text(String),
    Error(String),
    Timeout,
}

/// Deterministic test double. Not behind `#[cfg(test)]`: it must be visible to
/// integration tests in `tests/`, which compile as a separate crate and cannot
/// see items gated on the *library's* test cfg (research.md §1; mirrors the
/// always-compiled `Settings::load_from_paths_pub` test-support pattern). The
/// binary crate never constructs one — hence `#[allow(dead_code)]` below.
#[allow(dead_code)]
pub struct StubProvider {
    queue: Mutex<VecDeque<StubOutcome>>,
    calls: AtomicUsize,
}

impl Default for StubProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl StubProvider {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            calls: AtomicUsize::new(0),
        }
    }

    /// Convenience constructor: always returns the same canned response.
    pub fn with_response(text: impl Into<String>) -> Self {
        let s = Self::new();
        s.push(StubOutcome::Text(text.into()));
        s
    }

    /// Queue an outcome for the next call. Calls beyond the queued outcomes
    /// repeat the last queued outcome (or an empty string if none was queued),
    /// so single-outcome stubs work across multi-chunk requests too.
    pub fn push(&self, outcome: StubOutcome) {
        self.queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(outcome);
    }

    pub fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl RewriteProvider for StubProvider {
    fn generate(&self, _prompt: &str, _timeout: Duration) -> Result<String, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        let outcome = if queue.len() > 1 {
            queue.pop_front().unwrap()
        } else if let Some(last) = queue.front() {
            last.clone()
        } else {
            StubOutcome::Text(String::new())
        };
        match outcome {
            StubOutcome::Text(t) => Ok(t),
            StubOutcome::Error(e) => Err(ProviderError::RequestFailed(e)),
            StubOutcome::Timeout => Err(ProviderError::Timeout),
        }
    }
}

/// A "print at most once" latch. Used by the automatic pipeline stage so an
/// unreachable local model warns once per process rather than once per
/// qualifying interception (FR-038) — CLI/MCP invocations already report
/// their own fallback reason on every call via `RewriteResult::reason`, so
/// this exists only for the silent, always-on auto-rewrite path.
#[cfg_attr(test, allow(dead_code))]
pub struct WarnOnce(AtomicBool);

impl Default for WarnOnce {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(test, allow(dead_code))]
impl WarnOnce {
    pub const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    /// Prints `message` to stderr and returns `true` on the first call;
    /// every subsequent call is a silent no-op returning `false`.
    pub fn warn(&self, message: &str) -> bool {
        let should_warn = self
            .0
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok();
        if should_warn {
            eprintln!("ecotokens: warning: automatic rewrite unavailable: {message}");
        }
        should_warn
    }
}

/// Process-wide instance backing the automatic pipeline's once-per-session
/// warning (FR-038). `WarnOnce` itself is the independently-testable unit —
/// see `tests/rewrite/provider_test.rs`.
#[cfg_attr(test, allow(dead_code))]
static AUTO_PIPELINE_WARNED: WarnOnce = WarnOnce::new();

#[cfg_attr(test, allow(dead_code))]
pub fn warn_auto_pipeline_unreachable_once(message: &str) -> bool {
    AUTO_PIPELINE_WARNED.warn(message)
}
