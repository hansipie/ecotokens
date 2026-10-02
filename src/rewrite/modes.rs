use super::RewriteError;

/// A transformation mode with its (validated) target, if any. The single
/// vocabulary shared by CLI flags, MCP parameters, config keys, and docs
/// (data-model.md, Constitution Principle V).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Paraphrase,
    Tone { target: String },
    ReadingLevel { target: String },
    Translate { target: String },
}

const MAX_FREE_FORM_TARGET_LEN: usize = 40;

/// Closed language vocabulary for `translate` (research.md, FR-005). Accepts
/// both ISO 639-1 codes and common English names, case-insensitively;
/// normalizes to the code so the value stored in `Mode::Translate` is stable.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "english"),
    ("fr", "french"),
    ("de", "german"),
    ("es", "spanish"),
    ("it", "italian"),
    ("pt", "portuguese"),
    ("nl", "dutch"),
    ("ru", "russian"),
    ("zh", "chinese"),
    ("ja", "japanese"),
    ("ko", "korean"),
    ("ar", "arabic"),
    ("hi", "hindi"),
    ("pl", "polish"),
    ("tr", "turkish"),
    ("sv", "swedish"),
    ("da", "danish"),
    ("no", "norwegian"),
    ("fi", "finnish"),
    ("cs", "czech"),
    ("el", "greek"),
    ("he", "hebrew"),
    ("id", "indonesian"),
    ("th", "thai"),
    ("vi", "vietnamese"),
    ("uk", "ukrainian"),
    ("ro", "romanian"),
    ("hu", "hungarian"),
];

fn normalize_language(raw: &str) -> Option<&'static str> {
    let needle = raw.trim().to_ascii_lowercase();
    LANGUAGES
        .iter()
        .find(|(code, name)| *code == needle || *name == needle)
        .map(|(code, _)| *code)
}

/// Non-empty, single-line, ≤ 40 chars, no control characters. The target is
/// interpolated into the model prompt, so an unbounded value is a
/// prompt-injection vector — matters more for MCP, where an agent may derive
/// it from untrusted content (contracts/cli-rewrite.md, FR-005).
fn validate_free_form_target(raw: &str) -> Result<(), RewriteError> {
    if raw.is_empty() {
        return Err(RewriteError::InvalidTarget("target is empty".into()));
    }
    if raw.len() > MAX_FREE_FORM_TARGET_LEN {
        return Err(RewriteError::InvalidTarget(format!(
            "target exceeds {MAX_FREE_FORM_TARGET_LEN} characters"
        )));
    }
    if raw.contains('\n') || raw.contains('\r') {
        return Err(RewriteError::InvalidTarget(
            "target must be a single line".into(),
        ));
    }
    if raw.chars().any(|c| c.is_control()) {
        return Err(RewriteError::InvalidTarget(
            "target must not contain control characters".into(),
        ));
    }
    Ok(())
}

impl Mode {
    /// Parse a mode name plus optional target, enforcing mode/target
    /// compatibility (data-model.md `RewriteRequest.Validation`).
    pub fn parse(mode: &str, target: Option<&str>) -> Result<Mode, RewriteError> {
        match mode {
            "paraphrase" => {
                if target.is_some() {
                    return Err(RewriteError::ForbiddenTarget { mode: "paraphrase" });
                }
                Ok(Mode::Paraphrase)
            }
            "tone" => {
                let t = target.ok_or(RewriteError::MissingTarget { mode: "tone" })?;
                validate_free_form_target(t)?;
                Ok(Mode::Tone {
                    target: t.to_string(),
                })
            }
            "reading-level" => {
                let t = target.ok_or(RewriteError::MissingTarget {
                    mode: "reading-level",
                })?;
                validate_free_form_target(t)?;
                Ok(Mode::ReadingLevel {
                    target: t.to_string(),
                })
            }
            "translate" => {
                let t = target.ok_or(RewriteError::MissingTarget { mode: "translate" })?;
                let code = normalize_language(t)
                    .ok_or_else(|| RewriteError::UnrecognizedLanguage(t.to_string()))?;
                Ok(Mode::Translate {
                    target: code.to_string(),
                })
            }
            other => Err(RewriteError::InvalidMode(other.to_string())),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Mode::Paraphrase => "paraphrase",
            Mode::Tone { .. } => "tone",
            Mode::ReadingLevel { .. } => "reading-level",
            Mode::Translate { .. } => "translate",
        }
    }

    pub fn target(&self) -> Option<&str> {
        match self {
            Mode::Paraphrase => None,
            Mode::Tone { target } | Mode::ReadingLevel { target } | Mode::Translate { target } => {
                Some(target.as_str())
            }
        }
    }

    /// Build the prompt sent to the local model. `carry_in` is the transformed
    /// tail of the previous chunk, used as a non-emitted style anchor for
    /// cross-chunk coherence (research.md §5, wired in by US4).
    pub fn prompt(&self, text: &str, carry_in: Option<&str>) -> String {
        let instruction = match self {
            Mode::Paraphrase => "Paraphrase the following text, preserving its full meaning. \
                Output only the paraphrased text, with no preamble or commentary."
                .to_string(),
            Mode::Tone { target } => format!(
                "Rewrite the following text in a '{target}' tone, preserving its full meaning. \
                Output only the rewritten text, with no preamble or commentary."
            ),
            Mode::ReadingLevel { target } => format!(
                "Rewrite the following text for a '{target}' reading level, preserving its full \
                meaning. Output only the rewritten text, with no preamble or commentary."
            ),
            Mode::Translate { target } => format!(
                "Translate the following text into the language with code '{target}'. \
                Preserve proper nouns, numbers, and dates. \
                Output only the translated text, with no preamble or commentary."
            ),
        };

        let anchor = carry_in
            .map(|a| {
                format!(
                    "\n\nContext (already-transformed end of the previous section, for style \
                    continuity only — do NOT repeat any of it in your output):\n{a}\n"
                )
            })
            .unwrap_or_default();

        format!("{instruction}{anchor}\n\nText:\n{text}")
    }
}
