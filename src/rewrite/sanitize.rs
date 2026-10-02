use lazy_regex::regex;
use regex::Regex;
use serde_json::json;

use super::modes::Mode;
use crate::jev::stats::Purpose;
use crate::jev::{JevContext, Question, Questions};

/// A span extracted before prompting and restored afterward, so the model
/// never sees (and cannot corrupt) fenced code, inline code, URLs, emails,
/// numbers, or dates (data-model.md `ProtectedSpan`, FR-009).
#[derive(Debug, Clone)]
pub struct ProtectedSpan {
    pub sentinel: String,
    pub original: String,
    /// Retained for future diagnostics/logging; not consumed yet.
    #[allow(dead_code)]
    pub kind: SpanKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    FencedCode,
    InlineCode,
    Url,
    Email,
    Number,
    Date,
}

const SENTINEL_OPEN: char = '⟦';
const SENTINEL_CLOSE: char = '⟧';
// Escaping uses a zero-width space so an input that already contains a
// sentinel-shaped bracket cannot collide with (or be mistaken for) a real
// sentinel inserted below.
const ESCAPE_MARK: char = '\u{200B}';

fn extract_with(
    working: String,
    re: &Regex,
    kind: SpanKind,
    counter: &mut usize,
    spans: &mut Vec<ProtectedSpan>,
) -> String {
    let mut result = String::with_capacity(working.len());
    let mut last_end = 0;
    for m in re.find_iter(&working) {
        result.push_str(&working[last_end..m.start()]);
        let sentinel = format!("{SENTINEL_OPEN}ET{}{SENTINEL_CLOSE}", *counter);
        *counter += 1;
        spans.push(ProtectedSpan {
            sentinel: sentinel.clone(),
            original: m.as_str().to_string(),
            kind,
        });
        result.push_str(&sentinel);
        last_end = m.end();
    }
    result.push_str(&working[last_end..]);
    result
}

/// Extract protected spans and replace them with unique sentinels. Order
/// matters: fenced code first (so nested backticks/URLs inside it are
/// consumed whole), then inline code, then URLs/emails/dates/numbers.
pub fn extract(text: &str) -> (String, Vec<ProtectedSpan>) {
    let mut spans = Vec::new();
    let mut counter = 0usize;

    // Escape any pre-existing sentinel-shaped brackets in the input.
    let mut working = text
        .replace(SENTINEL_OPEN, &format!("{SENTINEL_OPEN}{ESCAPE_MARK}"))
        .replace(SENTINEL_CLOSE, &format!("{ESCAPE_MARK}{SENTINEL_CLOSE}"));

    let fenced_re: &Regex = regex!(r"(?s)```.*?```");
    working = extract_with(
        working,
        fenced_re,
        SpanKind::FencedCode,
        &mut counter,
        &mut spans,
    );

    let inline_re: &Regex = regex!(r"`[^`\n]+`");
    working = extract_with(
        working,
        inline_re,
        SpanKind::InlineCode,
        &mut counter,
        &mut spans,
    );

    let url_re: &Regex = regex!(r"https?://[^\s<>\)\]]+");
    working = extract_with(working, url_re, SpanKind::Url, &mut counter, &mut spans);

    let email_re: &Regex = regex!(r"[\w.+-]+@[\w-]+\.[\w.-]+");
    working = extract_with(working, email_re, SpanKind::Email, &mut counter, &mut spans);

    let date_re: &Regex = regex!(r"\b\d{4}-\d{2}-\d{2}\b|\b\d{1,2}/\d{1,2}/\d{2,4}\b");
    working = extract_with(working, date_re, SpanKind::Date, &mut counter, &mut spans);

    let number_re: &Regex = regex!(r"\b\d[\d,]*\.?\d*\b");
    working = extract_with(
        working,
        number_re,
        SpanKind::Number,
        &mut counter,
        &mut spans,
    );

    (working, spans)
}

/// Pure inverse of [`extract`]. Validates that every sentinel appears exactly
/// once before substituting — zero or multiple occurrences is a
/// transformation failure that must trigger fail-open, never silent
/// corruption (data-model.md `ProtectedSpan` invariant, FR-017).
pub fn restore(text: &str, spans: &[ProtectedSpan]) -> Result<String, String> {
    for span in spans {
        let count = text.matches(&span.sentinel).count();
        if count != 1 {
            return Err(format!(
                "sentinel {} appeared {count} time(s) in the response (expected exactly 1)",
                span.sentinel
            ));
        }
    }

    let mut result = text.to_string();
    for span in spans {
        result = result.replacen(&span.sentinel, &span.original, 1);
    }

    let result = result
        .replace(
            &format!("{SENTINEL_OPEN}{ESCAPE_MARK}"),
            &SENTINEL_OPEN.to_string(),
        )
        .replace(
            &format!("{ESCAPE_MARK}{SENTINEL_CLOSE}"),
            &SENTINEL_CLOSE.to_string(),
        );

    Ok(result)
}

fn preamble_regex() -> &'static Regex {
    regex!(r"(?i)^(here('s| is)|sure[,!]?|certainly[,!]?|i've|below is)\b.*[:.]?\s*$")
}

/// Strip a leading conversational preamble, a wrapping fence not present in
/// the input, and trailing commentary — the two most common real failure
/// modes of small local models (research.md §6, FR-010).
// Used by the library crate and tests; unused in the binary.
#[allow(dead_code)]
pub fn cleanup_response(response: &str, input_had_fence: bool) -> String {
    cleanup_response_with(response, input_had_fence, None)
}

/// [`cleanup_response`] where the first/last-line commentary decision comes
/// from a Jev verdict instead of the English-only [`preamble_regex`]. A line
/// is only stripped when it is still the first/last line at that step and
/// matches the line Jev judged, so the fence handling stays unchanged.
pub fn cleanup_response_with(
    response: &str,
    input_had_fence: bool,
    commentary: Option<&Commentary>,
) -> String {
    let re = preamble_regex();
    let is_first_commentary = |line: &str| match commentary {
        Some(c) => c.first.as_deref() == Some(line),
        None => re.is_match(line),
    };
    let is_last_commentary = |line: &str| match commentary {
        Some(c) => c.last.as_deref() == Some(line),
        None => re.is_match(line),
    };
    let mut text = response.trim().to_string();

    if let Some(nl) = text.find('\n') {
        let first_line = text[..nl].trim();
        if !first_line.is_empty() && is_first_commentary(first_line) {
            text = text[nl + 1..].trim_start().to_string();
        }
    }

    if !input_had_fence {
        let trimmed = text.trim();
        if let Some(inner) = trimmed
            .strip_prefix("```")
            .and_then(|s| s.strip_suffix("```"))
        {
            let mut inner = inner.to_string();
            if let Some(nl) = inner.find('\n') {
                let first_line = inner[..nl].trim();
                if !first_line.is_empty() && first_line.chars().all(|c| c.is_alphanumeric()) {
                    inner = inner[nl + 1..].to_string();
                }
            }
            text = inner.trim().to_string();
        }
    }

    if let Some(nl) = text.rfind('\n') {
        let last_line = text[nl + 1..].trim();
        if !last_line.is_empty() && is_last_commentary(last_line) {
            text = text[..nl].trim_end().to_string();
        }
    }

    text.trim().to_string()
}

/// Structural failure detection: empty response, or a response shorter than
/// `truncation_ratio` of the input — truncation or accidental summarization
/// (research.md §6, FR-011). Sentinel integrity is checked separately by
/// [`restore`].
pub fn detect_failure(input_tokens: u32, response: &str, truncation_ratio: f32) -> Option<String> {
    if response.trim().is_empty() {
        return Some("model returned an empty response".to_string());
    }
    if input_tokens == 0 {
        return None;
    }
    let output_tokens = crate::tokens::count_tokens(response) as u32;
    let ratio = output_tokens as f32 / input_tokens as f32;
    if ratio < truncation_ratio {
        return Some(format!(
            "response is only {output_tokens} tokens vs {input_tokens} input tokens \
            ({:.0}% < {:.0}% threshold) — likely truncated",
            ratio * 100.0,
            truncation_ratio * 100.0
        ));
    }
    None
}

// ── Jev verification (optional; structural checks above always run) ────────

/// First/last response lines that Jev judged to be model commentary rather
/// than part of the transformed text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Commentary {
    pub first: Option<String>,
    pub last: Option<String>,
}

/// Jev's reading of one model response.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Verdict {
    pub commentary: Commentary,
    /// `Some(reason)` when the response must not be used.
    pub failure: Option<String>,
}

const FAITHFUL_Q: &str = "faithful";
const IN_TARGET_Q: &str = "in_target";
const FIRST_LINE_Q: &str = "first_line_commentary";
const LAST_LINE_Q: &str = "last_line_commentary";

fn faithful_instructions(mode: &Mode) -> String {
    let what = match mode {
        Mode::Paraphrase => "a paraphrase of `original`",
        Mode::Tone { .. } => "a rewrite of `original` in a different tone",
        Mode::ReadingLevel { .. } => "a rewrite of `original` for a different reading level",
        Mode::Translate { .. } => "a translation of `original`",
    };
    format!(
        "`output` should be {what}. Ignoring style, wording, and language, does `output` \
         convey all the meaning of `original` without adding new information, answering it, \
         or leaving parts out? Ignore a leading or trailing line of assistant commentary. \
         Tokens like ⟦ET0⟧ are placeholders and must be treated as opaque."
    )
}

fn language_name(code: &str) -> &str {
    super::modes::LANGUAGES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, name)| *name)
        .unwrap_or(code)
}

fn commentary_question(which: &str) -> Question {
    Question::noul_with(
        format!(
            "Is `{which}` a remark by an assistant about the text (such as \"Here is the \
             rewritten text:\" or \"Let me know if you need changes\"), rather than part \
             of the text itself?"
        ),
        "The line talks about the task or the text, addressed to the requester",
        "The line is content belonging to the text",
    )
}

/// Ask Jev whether `response` is a faithful transformation of `original`
/// (and, for `translate`, in the target language), and which first/last
/// line is commentary. `None` when the call fails or an answer is missing:
/// the caller then runs today's regex cleanup and structural checks only.
pub fn judge_response(
    original: &str,
    response: &str,
    mode: &Mode,
    ctx: JevContext<'_>,
    timeout: std::time::Duration,
) -> Option<Verdict> {
    let trimmed = response.trim();
    let lines: Vec<&str> = trimmed.lines().map(str::trim).collect();
    let multi_line = lines.len() > 1;
    let first = lines.first().copied().unwrap_or("");
    let last = lines.last().copied().unwrap_or("");

    let mut questions: Questions = Questions::new();
    questions.insert(
        FAITHFUL_Q.to_string(),
        Question::noul(faithful_instructions(mode)),
    );
    if let Mode::Translate { target } = mode {
        questions.insert(
            IN_TARGET_Q.to_string(),
            Question::noul(format!(
                "Is `output` written in {}? Ignore placeholders, code, names, and numbers.",
                language_name(target)
            )),
        );
    }
    if multi_line {
        questions.insert(FIRST_LINE_Q.to_string(), commentary_question("first_line"));
        questions.insert(LAST_LINE_Q.to_string(), commentary_question("last_line"));
    }

    let state = json!({
        "mode": mode.name(),
        "target": mode.target(),
        "original": original,
        "output": trimmed,
        "first_line": first,
        "last_line": last,
    });
    let answers = ctx
        .ask_for(Purpose::Verify, state, questions, timeout)
        .ok()?;
    let s = ctx.settings;

    let faithful = answers.noul(FAITHFUL_Q)?;
    let mut failure = None;
    if faithful < s.jev_verify_fail_below {
        failure = Some(format!(
            "Jev judged the response unfaithful to the input (p={faithful:.2})"
        ));
    }
    if let Mode::Translate { target } = mode {
        let in_target = answers.noul(IN_TARGET_Q)?;
        if failure.is_none() && in_target < s.jev_verify_fail_below {
            failure = Some(format!(
                "Jev judged the response not to be in the target language '{target}' \
                 (p={in_target:.2})"
            ));
        }
    }

    let mut commentary = Commentary::default();
    if multi_line {
        if answers.noul(FIRST_LINE_Q)? >= s.jev_commentary_min_prob {
            commentary.first = Some(first.to_string());
        }
        if answers.noul(LAST_LINE_Q)? >= s.jev_commentary_min_prob {
            commentary.last = Some(last.to_string());
        }
    }

    Some(Verdict {
        commentary,
        failure,
    })
}
