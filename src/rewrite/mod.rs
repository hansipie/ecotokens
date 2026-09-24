pub mod chunk;
pub mod detect;
pub mod diff;
pub mod modes;
pub mod provider;
pub mod sanitize;

use std::time::{Duration, Instant};

use serde::Serialize;

use crate::jev::JevContext;
pub use modes::Mode;
pub use provider::RewriteProvider;

/// Where a rewrite invocation originated. Drives budget selection and metrics
/// attribution (data-model.md `RewriteRequest.origin`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Cli,
    /// Used by the MCP tool (User Story 5, not yet wired in).
    #[allow(dead_code)]
    Mcp,
    /// Used by the automatic pipeline stage (User Story 6, gated).
    #[allow(dead_code)]
    AutoPipeline,
}

/// Validation and transformation failures. These map to non-zero exit codes at
/// the CLI boundary (contracts/cli-rewrite.md) — never to `Status::Fallback`,
/// which is reserved for a validated request whose *generation* failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewriteError {
    InvalidMode(String),
    MissingTarget { mode: &'static str },
    ForbiddenTarget { mode: &'static str },
    InvalidTarget(String),
    UnrecognizedLanguage(String),
    NotProse,
}

impl std::fmt::Display for RewriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RewriteError::InvalidMode(m) => write!(
                f,
                "unknown mode '{m}' — valid modes: paraphrase, tone, reading-level, translate"
            ),
            RewriteError::MissingTarget { mode } => {
                write!(f, "mode '{mode}' requires --to <TARGET>")
            }
            RewriteError::ForbiddenTarget { mode } => {
                write!(f, "mode '{mode}' does not accept --to <TARGET>")
            }
            RewriteError::InvalidTarget(reason) => write!(f, "invalid target: {reason}"),
            RewriteError::UnrecognizedLanguage(lang) => {
                write!(f, "unrecognized language identifier: '{lang}'")
            }
            RewriteError::NotProse => write!(
                f,
                "input looks like code, a diff, or structured data — refusing to rewrite"
            ),
        }
    }
}

impl std::error::Error for RewriteError {}

/// A generation-layer failure from a [`RewriteProvider`]. Distinct from
/// [`RewriteError`]: these never abort the request, they trigger fail-open
/// (data-model.md `RewriteResult` state diagram).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    RequestFailed(String),
    Timeout,
    BadResponse(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::RequestFailed(e) => write!(f, "request failed: {e}"),
            ProviderError::Timeout => write!(f, "request timed out"),
            ProviderError::BadResponse(e) => write!(f, "bad response: {e}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Transformed,
    NoOp,
    Fallback,
}

/// The input to one transformation (data-model.md `RewriteRequest`).
#[derive(Debug, Clone)]
pub struct RewriteRequest {
    pub text: String,
    pub mode: Mode,
    pub model: String,
    pub timeout: Duration,
    #[allow(dead_code)]
    pub origin: Origin,
    /// Response shorter than this fraction of input tokens is treated as
    /// truncation (`rewrite_truncation_ratio`, FR-011).
    pub truncation_ratio: f32,
    /// Assumed model context window in tokens (`rewrite_context_tokens`).
    /// Drives the chunk budget (`chunk::effective_chunk_budget`) for
    /// documents that exceed a single pass (FR-012 to FR-016).
    pub context_tokens: u32,
    /// Whether to save a masked diff of original vs transformed text on a
    /// successful transformation (`--save-diff`/`--no-save-diff`/
    /// `rewrite_save_diff` precedence resolved by the caller; FR-021).
    pub save_diff: bool,
    /// Directory diffs are written to when `save_diff` is set
    /// (`rewrite_diff_dir`, default: OS temp dir).
    pub diff_dir: std::path::PathBuf,
    /// Maximum diffs retained; oldest pruned first. `0` disables pruning
    /// (`rewrite_diff_retention`, FR-027).
    pub diff_retention: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RewriteResult {
    pub status: Status,
    pub reason: Option<String>,
    pub mode: String,
    pub target: Option<String>,
    pub model: String,
    pub text: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub chunk_count: usize,
    pub duration_ms: u32,
    /// Path to the saved diff file, when one was written. Always `None` for
    /// `NoOp`/`Fallback` outcomes and whenever `save_diff` was not requested
    /// or the write failed (the failure itself is reported on stderr by the
    /// caller, never surfaced here — FR-028).
    pub diff_path: Option<String>,
}

fn no_op(request: &RewriteRequest, reason: &str) -> RewriteResult {
    let tokens = crate::tokens::count_tokens(&request.text) as u32;
    RewriteResult {
        status: Status::NoOp,
        reason: Some(reason.to_string()),
        mode: request.mode.name().to_string(),
        target: request.mode.target().map(str::to_string),
        model: request.model.clone(),
        text: request.text.clone(),
        tokens_in: tokens,
        tokens_out: tokens,
        chunk_count: 0,
        duration_ms: 0,
        diff_path: None,
    }
}

fn fallback(request: &RewriteRequest, reason: String, duration_ms: u32) -> RewriteResult {
    let tokens = crate::tokens::count_tokens(&request.text) as u32;
    RewriteResult {
        status: Status::Fallback,
        reason: Some(reason),
        mode: request.mode.name().to_string(),
        target: request.mode.target().map(str::to_string),
        model: request.model.clone(),
        text: request.text.clone(),
        tokens_in: tokens,
        tokens_out: tokens,
        chunk_count: 1,
        duration_ms,
        diff_path: None,
    }
}

/// Transform `request.text` through `provider`. Validation failures return
/// `Err`; every generation-layer failure fails open into
/// `Ok(Status::Fallback)` with the input byte-identical (FR-017, SC-002).
/// Documents exceeding the effective chunk budget take the multi-chunk path
/// in [`chunked_rewrite`] instead of the single-pass sequence below (FR-012
/// to FR-016); a single-chunk document is completely unaffected by that path
/// existing at all.
// Used by the library crate and tests; unused in the binary.
#[allow(dead_code)]
pub fn rewrite(
    request: RewriteRequest,
    provider: &dyn RewriteProvider,
) -> Result<RewriteResult, RewriteError> {
    rewrite_with_judge(request, provider, None)
}

/// [`rewrite`] with optional Jev judgments: the code refusal and
/// same-language check come from [`detect::rewrite_gate`], and each model
/// response is verified by [`sanitize::judge_response`]. `None`, or any Jev
/// failure, runs exactly the heuristic path of [`rewrite`].
pub fn rewrite_with_judge(
    request: RewriteRequest,
    provider: &dyn RewriteProvider,
    jev: Option<JevContext<'_>>,
) -> Result<RewriteResult, RewriteError> {
    let start = Instant::now();

    if request.text.trim().is_empty() {
        return Ok(no_op(&request, "empty input"));
    }

    let is_translate = matches!(request.mode, Mode::Translate { .. });
    let gate = detect::rewrite_gate(&request.text, is_translate, jev, Some(request.timeout));

    if gate.is_code {
        return Err(RewriteError::NotProse);
    }

    if let Mode::Translate { target } = &request.mode {
        if let Some(source) = gate.language {
            if source.eq_ignore_ascii_case(target) {
                return Ok(no_op(
                    &request,
                    &format!("input is already in the target language ({target})"),
                ));
            }
        }
    }

    let chunk_budget = chunk::effective_chunk_budget(request.context_tokens);
    if crate::tokens::count_tokens(&request.text) as u32 > chunk_budget {
        return Ok(chunked_rewrite(
            &request,
            provider,
            chunk_budget,
            start,
            jev,
        ));
    }

    let (sanitized, spans) = sanitize::extract(&request.text);
    let prompt = request.mode.prompt(&sanitized, None);

    let response = match provider.generate(&prompt, request.timeout) {
        Ok(r) => r,
        Err(e) => {
            return Ok(fallback(
                &request,
                e.to_string(),
                start.elapsed().as_millis() as u32,
            ))
        }
    };

    let verdict = judge_output(
        jev,
        &sanitized,
        &response,
        &request.mode,
        start,
        request.timeout,
    );

    let input_had_fence = sanitized.contains("```");
    let cleaned = sanitize::cleanup_response_with(
        &response,
        input_had_fence,
        verdict.as_ref().map(|v| &v.commentary),
    );

    let input_tokens = crate::tokens::count_tokens(&sanitized) as u32;
    if let Some(reason) = sanitize::detect_failure(input_tokens, &cleaned, request.truncation_ratio)
        .or_else(|| verdict.and_then(|v| v.failure))
    {
        return Ok(fallback(
            &request,
            reason,
            start.elapsed().as_millis() as u32,
        ));
    }

    let restored = match sanitize::restore(&cleaned, &spans) {
        Ok(t) => t,
        Err(reason) => {
            return Ok(fallback(
                &request,
                reason,
                start.elapsed().as_millis() as u32,
            ))
        }
    };

    let duration_ms = start.elapsed().as_millis() as u32;
    let tokens_in = crate::tokens::count_tokens(&request.text) as u32;
    let tokens_out = crate::tokens::count_tokens(&restored) as u32;

    let diff_path = if request.save_diff {
        save_diff_or_warn(&request, &restored)
    } else {
        None
    };

    let result = RewriteResult {
        status: Status::Transformed,
        reason: None,
        mode: request.mode.name().to_string(),
        target: request.mode.target().map(str::to_string),
        model: request.model.clone(),
        text: restored,
        tokens_in,
        tokens_out,
        chunk_count: 1,
        duration_ms,
        diff_path,
    };

    record_metrics(&request, &result);

    Ok(result)
}

/// Build, save, and prune a transformation diff (FR-021 to FR-028). A write
/// failure degrades to a stderr warning and `None` — it must never affect
/// `stdout`, the exit code, or the returned result's `text` (FR-028).
fn save_diff_or_warn(request: &RewriteRequest, transformed: &str) -> Option<String> {
    match diff::write_diff(diff::DiffRequest {
        original: &request.text,
        transformed,
        mode: request.mode.name(),
        target: request.mode.target(),
        model: &request.model,
        chunk_count: 1,
        diff_dir: &request.diff_dir,
        retention: request.diff_retention,
    }) {
        Ok(path) => Some(path.display().to_string()),
        Err(e) => {
            eprintln!(
                "ecotokens: warning: could not save rewrite diff to {}: {e}",
                request.diff_dir.display()
            );
            None
        }
    }
}

fn remaining_timeout(start: Instant, whole: Duration) -> Option<Duration> {
    let elapsed = start.elapsed();
    if elapsed >= whole {
        None
    } else {
        Some(whole - elapsed)
    }
}

/// Jev verdict on one model response, within what remains of the
/// operation's budget. `None` (no judge, empty response, budget spent, or a
/// failed call) means the heuristic cleanup and structural checks alone decide.
fn judge_output(
    jev: Option<JevContext<'_>>,
    original: &str,
    response: &str,
    mode: &Mode,
    start: Instant,
    whole: Duration,
) -> Option<sanitize::Verdict> {
    let ctx = jev?;
    if response.trim().is_empty() {
        return None;
    }
    let remaining = remaining_timeout(start, whole)?;
    sanitize::judge_response(original, response, mode, ctx, ctx.timeout(Some(remaining)))
}

/// Multi-chunk path (FR-012 to FR-016). Each chunk is validated, protected,
/// prompted, cleaned, and restored independently — mirroring the single-pass
/// sequence in [`rewrite`] exactly via [`transform_chunk_text`] — carrying
/// the previous chunk's *transformed* tail forward as a non-emitted style
/// anchor (research.md §5). Any chunk failure, or the whole-operation
/// `request.timeout` running out partway through, abandons the entire
/// attempt and falls back to the original input — a partially transformed
/// result is never emitted (FR-015, FR-018).
fn chunked_rewrite(
    request: &RewriteRequest,
    provider: &dyn RewriteProvider,
    budget: u32,
    start: Instant,
    jev: Option<JevContext<'_>>,
) -> RewriteResult {
    let (chunks, _warnings) = chunk::split_into_chunks(&request.text, budget);

    if chunks.is_empty() {
        // Only possible for empty input, already handled above this call
        // site — defensive fallback rather than a panic if that ever drifts.
        return fallback(
            request,
            "no chunks produced".to_string(),
            start.elapsed().as_millis() as u32,
        );
    }

    let mut carry_in: Option<String> = None;
    let mut pieces: Vec<String> = Vec::with_capacity(chunks.len());

    for c in &chunks {
        let Some(remaining) = remaining_timeout(start, request.timeout) else {
            return fallback(
                request,
                "whole-operation timeout exceeded during chunked rewrite".to_string(),
                start.elapsed().as_millis() as u32,
            );
        };

        if c.atomic {
            // Protected structures (fenced code, tables, list groups) pass
            // through untouched; they are never sent to the model.
            pieces.push(c.text.clone());
            continue;
        }

        match transform_chunk_text(
            &c.text,
            &request.mode,
            carry_in.as_deref(),
            provider,
            remaining,
            request.truncation_ratio,
            jev,
        ) {
            Ok(restored) => {
                carry_in = Some(chunk::carry_over_anchor(&restored));
                // `transform_chunk_text` trims leading/trailing whitespace
                // (via `sanitize::cleanup_response`), which discards the
                // paragraph separator that chunking deliberately attached to
                // the end of `c.text` (boundary-inclusive splitting, so
                // reassembly needs no extra logic for the *source*). Without
                // reattaching it here, consecutive chunks glue together with
                // no separator at all in the *transformed* output.
                let trailing_ws = &c.text[c.text.trim_end().len()..];
                pieces.push(format!("{restored}{trailing_ws}"));
            }
            Err(reason) => {
                return fallback(request, reason, start.elapsed().as_millis() as u32);
            }
        }
    }

    let final_text = chunk::reassemble(&pieces);
    let duration_ms = start.elapsed().as_millis() as u32;
    let tokens_in = crate::tokens::count_tokens(&request.text) as u32;
    let tokens_out = crate::tokens::count_tokens(&final_text) as u32;

    let diff_path = if request.save_diff {
        save_diff_or_warn(request, &final_text)
    } else {
        None
    };

    let result = RewriteResult {
        status: Status::Transformed,
        reason: None,
        mode: request.mode.name().to_string(),
        target: request.mode.target().map(str::to_string),
        model: request.model.clone(),
        text: final_text,
        tokens_in,
        tokens_out,
        chunk_count: chunks.len(),
        duration_ms,
        diff_path,
    };

    record_metrics(request, &result);
    result
}

/// One chunk's validate→protect→prompt→generate→cleanup→restore pass. Kept
/// as its own function — rather than refactoring the single-pass sequence in
/// [`rewrite`] to share it — so the existing single-chunk behavior (US1-US3)
/// is not touched by chunking at all.
#[allow(clippy::too_many_arguments)]
fn transform_chunk_text(
    text: &str,
    mode: &Mode,
    carry_in: Option<&str>,
    provider: &dyn RewriteProvider,
    timeout: Duration,
    truncation_ratio: f32,
    jev: Option<JevContext<'_>>,
) -> Result<String, String> {
    let chunk_start = Instant::now();
    let (sanitized, spans) = sanitize::extract(text);
    let prompt = mode.prompt(&sanitized, carry_in);

    let response = provider
        .generate(&prompt, timeout)
        .map_err(|e| e.to_string())?;

    // Judge the response without an echoed style anchor, which the cleanup
    // below removes anyway and which would otherwise read as added content.
    let judged = match carry_in {
        Some(anchor) => chunk::strip_echoed_anchor(&response, anchor),
        None => response.clone(),
    };
    let verdict = judge_output(jev, &sanitized, &judged, mode, chunk_start, timeout);

    let input_had_fence = sanitized.contains("```");
    let mut cleaned = sanitize::cleanup_response_with(
        &response,
        input_had_fence,
        verdict.as_ref().map(|v| &v.commentary),
    );
    if let Some(anchor) = carry_in {
        cleaned = chunk::strip_echoed_anchor(&cleaned, anchor);
    }

    let input_tokens = crate::tokens::count_tokens(&sanitized) as u32;
    if let Some(reason) = sanitize::detect_failure(input_tokens, &cleaned, truncation_ratio)
        .or_else(|| verdict.and_then(|v| v.failure))
    {
        return Err(reason);
    }

    sanitize::restore(&cleaned, &spans)
}

/// Record a `FilterMode::Rewritten` interception for `Cli`/`Mcp` invocations
/// (FR-040). `AutoPipeline` invocations are recorded by their caller in
/// `src/filter/mod.rs`, which has the real `hook_type` context (research.md §10).
#[cfg(not(test))]
fn record_metrics(request: &RewriteRequest, result: &RewriteResult) {
    if request.origin == Origin::AutoPipeline {
        return;
    }
    let Some(path) = crate::metrics::store::metrics_path() else {
        return;
    };
    let hook_type = match request.origin {
        Origin::Cli => crate::metrics::store::HookType::Cli,
        Origin::Mcp => crate::metrics::store::HookType::Mcp,
        Origin::AutoPipeline => unreachable!(),
    };
    let mut rec = crate::metrics::store::Interception::new(
        format!("ecotokens rewrite --mode {}", result.mode),
        crate::metrics::store::CommandFamily::Generic,
        crate::filter::project_root_for_cwd(&std::env::current_dir().unwrap_or_default()),
        result.tokens_in,
        result.tokens_out,
        crate::metrics::store::FilterMode::Rewritten,
        false,
        result.duration_ms,
        None,
        None,
    )
    .with_hook_type(hook_type);
    rec.savings_pct = 0.0;
    let _ = crate::metrics::store::append_to(&path, &rec);
}

#[cfg(test)]
fn record_metrics(_request: &RewriteRequest, _result: &RewriteResult) {}
