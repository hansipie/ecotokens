use std::time::Duration;

use lazy_regex::regex;
use regex::Regex;
use serde_json::json;

use super::modes::LANGUAGES;
use crate::config::Settings;
use crate::jev::stats::Purpose;
use crate::jev::{ChoiceAnswer, JevContext, Question, Questions};

/// Conservative heuristic: does `text` look predominantly like source code
/// rather than prose? Biased toward *not* refusing — a false negative just
/// means the model sees code it will likely leave alone or mangle (caught by
/// downstream failure detection anyway); a false positive would block a
/// legitimate rewrite (Assumption §3, research.md §7).
pub fn is_predominantly_code(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let fence_re: &Regex = regex!(r"(?s)```.*?```");
    let fenced_chars: usize = fence_re.find_iter(trimmed).map(|m| m.as_str().len()).sum();
    if fenced_chars as f64 / trimmed.len() as f64 > 0.8 {
        return true;
    }

    let lines: Vec<&str> = trimmed.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return false;
    }

    // A line is "code-shaped" if it starts with a common code keyword, ends
    // with a statement/block terminator (`;`, `{`, `}`), or is a lone brace —
    // patterns that are rare in prose but define most lines of C-like, Rust,
    // Python, or JS/TS source (research.md §7).
    let code_line_re: &Regex = regex!(
        r"^\s*(fn |def |class |import |use |const |let |var |public |private |#include|from \w+ import|if |else|elif |for |while |return |struct |enum |match |impl |//|#!)|[{};]\s*$|^\s*[{}]\s*$"
    );

    let code_lines = lines.iter().filter(|l| code_line_re.is_match(l)).count();

    (code_lines as f64 / lines.len() as f64) > 0.6
}

/// Distinctive, very common short function words per language (lowercase).
/// Deliberately small and blunt — this exists only to catch the case where a
/// user asks to translate text that is already in the target language
/// (Out of Scope: general language detection, research.md/US2). Codes match
/// `modes::LANGUAGES`.
const STOPWORDS: &[(&str, &[&str])] = &[
    (
        "en",
        &[
            "the", "and", "is", "of", "to", "in", "that", "it", "for", "with", "this", "was",
            "are", "on", "as", "you", "be",
        ],
    ),
    (
        "fr",
        &[
            "le", "la", "les", "et", "est", "des", "un", "une", "que", "pour", "dans", "avec",
            "ce", "qui", "vous", "nous", "pas",
        ],
    ),
    (
        "de",
        &[
            "der", "die", "das", "und", "ist", "ein", "eine", "nicht", "mit", "für", "auf", "sich",
            "den", "zu", "sie", "ich", "wir",
        ],
    ),
    (
        "es",
        &[
            "el", "los", "las", "y", "es", "un", "una", "que", "para", "con", "en", "por", "su",
            "no", "se", "usted",
        ],
    ),
    (
        "it",
        &[
            "il", "la", "e", "è", "di", "un", "una", "che", "per", "con", "gli", "le", "sono",
            "non", "questo", "sei",
        ],
    ),
    (
        "pt",
        &[
            "o", "a", "os", "as", "e", "é", "de", "um", "uma", "que", "para", "com", "não", "se",
            "do", "você",
        ],
    ),
    (
        "nl",
        &[
            "de", "het", "een", "en", "is", "van", "dat", "niet", "voor", "met", "op", "zijn",
            "te", "in", "je",
        ],
    ),
    (
        "sv",
        &[
            "och", "det", "är", "en", "ett", "att", "för", "med", "som", "inte", "den", "de",
            "jag", "du",
        ],
    ),
    (
        "da",
        &[
            "og", "det", "er", "en", "et", "at", "for", "med", "som", "ikke", "den", "de", "jeg",
            "du",
        ],
    ),
    (
        "no",
        &[
            "og", "det", "er", "en", "et", "for", "med", "som", "ikke", "den", "de", "jeg", "du",
        ],
    ),
    (
        "fi",
        &[
            "ja", "on", "ei", "se", "että", "tämä", "olla", "hän", "niin", "kun", "minä",
        ],
    ),
    (
        "cs",
        &[
            "a", "je", "se", "na", "že", "to", "s", "pro", "ale", "jako", "já",
        ],
    ),
    (
        "pl",
        &[
            "i", "jest", "nie", "na", "że", "to", "z", "do", "się", "w", "ja",
        ],
    ),
    (
        "tr",
        &[
            "ve", "bir", "bu", "de", "da", "için", "ile", "gibi", "çok", "ben",
        ],
    ),
    (
        "ro",
        &["și", "este", "un", "o", "că", "cu", "pentru", "nu", "eu"],
    ),
    ("hu", &["és", "az", "hogy", "nem", "van", "egy", "ez", "én"]),
    (
        "id",
        &[
            "dan", "yang", "di", "ini", "itu", "tidak", "untuk", "dengan", "saya",
        ],
    ),
    (
        "vi",
        &["và", "là", "của", "có", "không", "này", "cho", "với", "tôi"],
    ),
];

const MIN_CONFIDENCE: f64 = 0.12;
const MIN_MARGIN: f64 = 1.4;

fn tokenize_lower(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

/// Latin-script (and similar) detection via stopword frequency. Returns
/// `None` unless one language is both above a minimum hit rate and clearly
/// ahead of the runner-up — a weak or ambiguous signal must never trigger a
/// same-language no-op that skips a translation the user actually wanted.
fn detect_by_stopwords(text: &str) -> Option<&'static str> {
    let words = tokenize_lower(text);
    if words.len() < 6 {
        return None;
    }
    let total = words.len() as f64;

    let mut scores: Vec<(&'static str, f64)> = STOPWORDS
        .iter()
        .map(|(lang, list)| {
            let hits = words.iter().filter(|w| list.contains(&w.as_str())).count();
            (*lang, hits as f64 / total)
        })
        .collect();
    // `hits / total` is always a finite ratio (total > 0, guarded above), so
    // `partial_cmp` can never return `None` — but `unwrap_or` here costs
    // nothing and keeps this path panic-free even if that invariant drifts.
    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let (best_lang, best_score) = scores[0];
    let second_score = scores.get(1).map(|(_, s)| *s).unwrap_or(0.0);

    if best_score >= MIN_CONFIDENCE
        && (second_score == 0.0 || best_score / second_score >= MIN_MARGIN)
    {
        Some(best_lang)
    } else {
        None
    }
}

fn script_ratio(text: &str, ranges: &[(u32, u32)]) -> f64 {
    let alphabetic: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
    if alphabetic.is_empty() {
        return 0.0;
    }
    let matched = alphabetic
        .iter()
        .filter(|c| {
            let cp = **c as u32;
            ranges.iter().any(|(lo, hi)| cp >= *lo && cp <= *hi)
        })
        .count();
    matched as f64 / alphabetic.len() as f64
}

const CYRILLIC: &[(u32, u32)] = &[(0x0400, 0x04FF)];
const GREEK: &[(u32, u32)] = &[(0x0370, 0x03FF)];
const HEBREW: &[(u32, u32)] = &[(0x0590, 0x05FF)];
const ARABIC: &[(u32, u32)] = &[(0x0600, 0x06FF)];
const DEVANAGARI: &[(u32, u32)] = &[(0x0900, 0x097F)];
const HAN: &[(u32, u32)] = &[(0x4E00, 0x9FFF)];
const KANA: &[(u32, u32)] = &[(0x3040, 0x309F), (0x30A0, 0x30FF)];
const HANGUL: &[(u32, u32)] = &[(0xAC00, 0xD7A3)];
const THAI: &[(u32, u32)] = &[(0x0E00, 0x0E7F)];

/// Ukrainian carries a handful of Cyrillic letters absent from Russian
/// (і, ї, є, ґ); their presence is a reliable Ukrainian signal, their
/// absence defaults to Russian — the two are otherwise indistinguishable by
/// script alone.
const UKRAINIAN_ONLY_LETTERS: &[char] = &['і', 'ї', 'є', 'ґ', 'І', 'Ї', 'Є', 'Ґ'];

/// Script-based detection for languages that don't share the Latin alphabet
/// with the stopword set above. A majority (>50%) of alphabetic characters
/// in a script's range is treated as a confident signal — mixing several
/// scripts in one document is rare enough in practice that a bare majority
/// is a reasonable bar here (this heuristic exists only to avoid one
/// unnecessary model call, never to gate a real translation).
fn detect_by_script(text: &str) -> Option<&'static str> {
    if script_ratio(text, KANA) > 0.05 {
        return Some("ja");
    }
    if script_ratio(text, HAN) > 0.5 {
        return Some("zh");
    }
    if script_ratio(text, HANGUL) > 0.5 {
        return Some("ko");
    }
    if script_ratio(text, CYRILLIC) > 0.5 {
        return Some(
            if text.chars().any(|c| UKRAINIAN_ONLY_LETTERS.contains(&c)) {
                "uk"
            } else {
                "ru"
            },
        );
    }
    if script_ratio(text, GREEK) > 0.5 {
        return Some("el");
    }
    if script_ratio(text, HEBREW) > 0.5 {
        return Some("he");
    }
    if script_ratio(text, ARABIC) > 0.5 {
        return Some("ar");
    }
    if script_ratio(text, DEVANAGARI) > 0.5 {
        return Some("hi");
    }
    if script_ratio(text, THAI) > 0.5 {
        return Some("th");
    }
    None
}

/// Lightweight source-language detection, scoped only to rejecting a
/// same-language translation request (Out of Scope: general language
/// detection). Returns `None` — meaning "unknown, proceed with the model
/// call" — whenever the signal isn't strong, which is the safe default: a
/// missed detection costs one avoidable model call, a false one would
/// silently skip a translation the user actually wanted.
pub fn detect_source_language(text: &str) -> Option<&'static str> {
    detect_by_script(text).or_else(|| detect_by_stopwords(text))
}

// ── Content classification (US6 auto-pipeline gating, research.md §7) ──────

/// Coarse content shape, used only to gate the *automatic* rewrite stage.
/// FR-035/SC-011 require code, stack traces, error messages, diffs, and
/// structured data to pass through the pipeline untouched — only `Prose` is
/// ever eligible for automatic transformation. Deliberately biased toward
/// *not* `Prose` when uncertain: a missed transformation is invisible, a
/// corrupted code block or trace is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, allow(dead_code))]
pub enum ContentKind {
    Prose,
    Code,
    Structured,
    Diagnostic,
}

#[cfg_attr(test, allow(dead_code))]
fn is_diagnostic(text: &str) -> bool {
    if text.contains("Traceback (most recent call last)") {
        return true;
    }
    let panic_re: &Regex = regex!(r"thread '.*' panicked");
    if panic_re.is_match(text) {
        return true;
    }
    // Stack-frame lines: Python/JS "at foo (file:line)"-ish, Java
    // "at pkg.Class.method(File.java:42)", Rust "at src/main.rs:42:5".
    let stack_frame_re: &Regex = regex!(r"(?m)^\s*at .*(\(.*:\d+(:\d+)?\)|:\d+)\s*$");
    stack_frame_re.is_match(text)
}

#[cfg_attr(test, allow(dead_code))]
fn is_structured_data(text: &str) -> bool {
    let trimmed = text.trim();
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
    {
        return true;
    }

    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return false;
    }

    // Unified diff markers.
    let diff_marker_re: &Regex = regex!(r"^(\+\+\+ |--- |@@ |diff --git )");
    if lines.iter().any(|l| diff_marker_re.is_match(l)) {
        return true;
    }

    // TOML/YAML-ish: most non-empty lines are `key: value`, `key = value`,
    // or a `[section]` header.
    let kv_re: &Regex = regex!(r"^[\w.\-]+\s*[:=]\s*\S");
    let section_re: &Regex = regex!(r"^\[[\w.\-]+\]$");
    let kv_lines = lines
        .iter()
        .filter(|l| kv_re.is_match(l.trim()) || section_re.is_match(l.trim()))
        .count();
    (kv_lines as f64 / lines.len() as f64) > 0.6
}

/// A soft, permissive sanity check — this is what stands between "uncertain"
/// and `Prose`, so it is intentionally cheap to fail: short fragments,
/// symbol-heavy text, and anything without ordinary sentence punctuation are
/// rejected rather than guessed at.
#[cfg_attr(test, allow(dead_code))]
fn looks_like_prose(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 5 {
        return false;
    }
    let alpha_words = words
        .iter()
        .filter(|w| {
            let alpha = w.chars().filter(|c| c.is_alphabetic()).count();
            alpha * 2 >= w.chars().count().max(1)
        })
        .count();
    let alpha_ratio = alpha_words as f64 / words.len() as f64;
    let has_sentence_punct = text.contains('.') || text.contains('?') || text.contains('!');
    alpha_ratio > 0.7 && has_sentence_punct
}

/// Classify `text` for the automatic pipeline stage. Order matters: the
/// narrowest, highest-confidence non-prose signals are checked first, and
/// anything that doesn't clear the permissive prose sanity check falls back
/// to `Structured` — a safe non-prose bucket — rather than `Prose`.
#[cfg_attr(test, allow(dead_code))]
pub fn classify(text: &str) -> ContentKind {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return ContentKind::Structured;
    }
    if is_diagnostic(trimmed) {
        return ContentKind::Diagnostic;
    }
    if is_structured_data(trimmed) {
        return ContentKind::Structured;
    }
    if is_predominantly_code(trimmed) {
        return ContentKind::Code;
    }
    if looks_like_prose(trimmed) {
        ContentKind::Prose
    } else {
        ContentKind::Structured
    }
}

// ── Jev-backed variants (heuristics above remain the fallback) ─────────────

const KIND_Q: &str = "kind";
const LANGUAGE_Q: &str = "language";
const LANGUAGE_UNCLEAR: &str = "mixed_or_unclear";

/// Exact, certain signals that need no judgment: checked before any Jev call.
#[cfg_attr(test, allow(dead_code))]
fn deterministic_kind(trimmed: &str) -> Option<ContentKind> {
    if trimmed.is_empty() {
        return Some(ContentKind::Structured);
    }
    let panic_re: &Regex = regex!(r"thread '.*' panicked");
    if trimmed.contains("Traceback (most recent call last)") || panic_re.is_match(trimmed) {
        return Some(ContentKind::Diagnostic);
    }
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
    {
        return Some(ContentKind::Structured);
    }
    let diff_marker_re: &Regex = regex!(r"(?m)^(\+\+\+ |--- |@@ |diff --git )");
    if diff_marker_re.is_match(trimmed) {
        return Some(ContentKind::Structured);
    }
    None
}

fn kind_question() -> Question {
    Question::choice(
        "What kind of content is `text` mostly made of?",
        [
            (
                "prose",
                "Natural-language sentences or paragraphs written for a human reader \
                 (documentation, explanations, messages), possibly with brief inline code",
            ),
            (
                "source_code",
                "Program source code in any language, even if it contains comments",
            ),
            (
                "stack_trace_or_error",
                "Compiler, runtime, or test-failure output, error messages, or a stack trace",
            ),
            (
                "structured_data",
                "Configuration, key/value pairs, tables, logs, command listings, or other \
                 machine-oriented records",
            ),
            (
                "mixed",
                "A substantial mix of prose with code, errors, or data, where rewriting the \
                 whole would risk corrupting the non-prose parts",
            ),
        ],
    )
}

fn language_question() -> Question {
    let mut options: Vec<(String, String)> = LANGUAGES
        .iter()
        .map(|(code, name)| (code.to_string(), capitalize(name)))
        .collect();
    options.push((
        LANGUAGE_UNCLEAR.to_string(),
        "Several languages in comparable amounts, or the language cannot be determined".to_string(),
    ));
    Question::choice(
        "Which natural language is the prose in `text` written in? Ignore code, \
         identifiers, and file paths.",
        options,
    )
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

#[cfg_attr(test, allow(dead_code))]
fn kind_from_answer(answer: &ChoiceAnswer, s: &Settings) -> ContentKind {
    match answer.choice.as_str() {
        "prose" if answer.prob("prose") >= s.jev_prose_min_prob => ContentKind::Prose,
        "source_code" => ContentKind::Code,
        "stack_trace_or_error" => ContentKind::Diagnostic,
        _ => ContentKind::Structured,
    }
}

fn language_from_answer(answer: &ChoiceAnswer, s: &Settings) -> Option<&'static str> {
    if answer.choice == LANGUAGE_UNCLEAR || answer.confidence < s.jev_language_min_confidence {
        return None;
    }
    LANGUAGES
        .iter()
        .find(|(code, _)| *code == answer.choice)
        .map(|(code, _)| *code)
}

/// [`classify`] with a Jev judgment in place of the ratio heuristics. Exact
/// signals (JSON, diffs, Python tracebacks, Rust panics) never reach Jev;
/// no judge, a failed call, or a missing answer falls back to [`classify`].
/// `Prose` requires `jev_prose_min_prob`, keeping the "not prose when
/// uncertain" bias explicit.
#[cfg_attr(test, allow(dead_code))]
pub fn classify_with(text: &str, jev: Option<JevContext<'_>>) -> ContentKind {
    let trimmed = text.trim();
    if let Some(kind) = deterministic_kind(trimmed) {
        return kind;
    }
    let Some(ctx) = jev else {
        return classify(text);
    };
    let questions: Questions = [(KIND_Q.to_string(), kind_question())].into();
    match ctx.ask_for(
        Purpose::Classify,
        json!({ "text": trimmed }),
        questions,
        ctx.timeout(None),
    ) {
        Ok(answers) => answers
            .choice(KIND_Q)
            .map(|a| kind_from_answer(a, ctx.settings))
            .unwrap_or_else(|| classify(text)),
        Err(_) => classify(text),
    }
}

/// What the rewrite entry point needs to know before calling the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewriteGate {
    /// Refuse the rewrite: the input is source code.
    pub is_code: bool,
    /// Detected source language (only computed when `want_language`).
    pub language: Option<&'static str>,
}

/// One Jev request answering both the code refusal and (for `translate`)
/// the source language, in place of [`is_predominantly_code`] and
/// [`detect_source_language`]. Each answer falls back independently to its
/// heuristic when missing; no judge or a failed call uses both heuristics.
pub fn rewrite_gate(
    text: &str,
    want_language: bool,
    jev: Option<JevContext<'_>>,
    remaining: Option<Duration>,
) -> RewriteGate {
    let heuristic_code = || is_predominantly_code(text);
    let heuristic_lang = || {
        if want_language {
            detect_source_language(text)
        } else {
            None
        }
    };

    let Some(ctx) = jev else {
        return RewriteGate {
            is_code: heuristic_code(),
            language: heuristic_lang(),
        };
    };

    let mut questions: Questions = [(KIND_Q.to_string(), kind_question())].into();
    if want_language {
        questions.insert(LANGUAGE_Q.to_string(), language_question());
    }
    let answers = ctx
        .ask_for(
            Purpose::CodeGate,
            json!({ "text": text.trim() }),
            questions,
            ctx.timeout(remaining),
        )
        .ok();

    let is_code = answers
        .as_ref()
        .and_then(|a| a.choice(KIND_Q))
        .map(|a| {
            a.choice == "source_code" && a.prob("source_code") >= ctx.settings.jev_code_min_prob
        })
        .unwrap_or_else(heuristic_code);
    let language = if want_language {
        match answers.as_ref().and_then(|a| a.choice(LANGUAGE_Q)) {
            Some(a) => language_from_answer(a, ctx.settings),
            None => heuristic_lang(),
        }
    } else {
        None
    };
    RewriteGate { is_code, language }
}
