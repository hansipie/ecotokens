//! Markdown rendering, parsing and size trimming of the handoff file.
//!
//! The grammar is documented in `specs/011-session-handoff/contracts/handoff-file.md`.

use std::fmt;

use super::{
    fmt_ts, parse_ts, FailedAttempt, Handoff, KeyFile, KeyFileUse, Kind, Status, NOT_PROVIDED,
};

const TITLE: &str = "# Handoff";
const SECTIONS: [&str; 6] = [
    "Header",
    "Objective",
    "Problem",
    "Key files",
    "Failed attempts",
    "Next steps",
];

/// Masks recognised secrets. Every text this feature writes or emits goes
/// through here (Constitution Principle V).
pub fn mask_text(text: &str) -> String {
    crate::masking::patterns::mask(text).0
}

// ── Rendering ───────────────────────────────────────────────────────────────

fn free_text(section: &str, text: Option<&str>) -> String {
    let body = text
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or(NOT_PROVIDED);
    format!("## {section}\n{body}\n")
}

fn key_file_line(k: &KeyFile) -> String {
    match k.usage {
        KeyFileUse::Modified => format!("- {} (modified)", k.path),
        KeyFileUse::Read(n) => format!("- {} (read ×{n})", k.path),
    }
}

fn list_section(section: &str, lines: Vec<String>, notes: &[String]) -> String {
    let mut out = format!("## {section}\n");
    if lines.is_empty() && notes.is_empty() {
        out.push_str(NOT_PROVIDED);
        out.push('\n');
    }
    for l in lines.iter().chain(notes) {
        out.push_str(l);
        out.push('\n');
    }
    out
}

pub fn render(h: &Handoff) -> String {
    let mut header = String::from("## Header\n");
    header.push_str(&format!("- session: {}\n", h.session));
    header.push_str(&format!("- cwd: {}\n", h.cwd));
    if let Some(b) = &h.branch {
        header.push_str(&format!("- branch: {b}\n"));
    }
    header.push_str(&format!("- created: {}\n", fmt_ts(h.created)));
    header.push_str(&format!("- status: {}\n", h.status.as_str()));
    if let Some(c) = h.consumed {
        header.push_str(&format!("- consumed: {}\n", fmt_ts(c)));
    }
    for (k, v) in &h.header_extra {
        header.push_str(&format!("- {k}: {v}\n"));
    }

    let mut parts = vec![
        format!("{TITLE}\n"),
        header,
        free_text("Objective", h.objective.as_deref()),
        free_text("Problem", h.problem.as_deref()),
        list_section(
            "Key files",
            h.key_files.iter().map(key_file_line).collect(),
            &h.key_files_notes,
        ),
        list_section(
            "Failed attempts",
            h.failed_attempts
                .iter()
                .map(|a| format!("- [{}] {}", a.kind.as_str(), a.text))
                .collect(),
            &h.failed_notes,
        ),
        free_text("Next steps", h.next_steps.as_deref()),
    ];
    for (name, body) in &h.extra_sections {
        parts.push(format!("## {name}\n{body}\n"));
    }
    parts.join("\n")
}

// ── Parsing ─────────────────────────────────────────────────────────────────

/// The file is not a valid handoff (see "Parse failure" in the file contract).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

fn err<T>(msg: &str) -> Result<T, ParseError> {
    Err(ParseError(msg.to_string()))
}

fn body_text(lines: &[&str]) -> String {
    let start = lines.iter().position(|l| !l.trim().is_empty());
    let end = lines.iter().rposition(|l| !l.trim().is_empty());
    match (start, end) {
        (Some(s), Some(e)) => lines[s..=e].join("\n"),
        _ => String::new(),
    }
}

fn model_text(lines: &[&str]) -> Option<String> {
    let text = body_text(lines);
    (!text.is_empty() && text != NOT_PROVIDED).then_some(text)
}

fn parse_key_file(line: &str) -> Option<KeyFile> {
    let rest = line.strip_prefix("- ")?;
    if let Some(path) = rest.strip_suffix(" (modified)") {
        return Some(KeyFile::modified(path));
    }
    let inner = rest.strip_suffix(')')?;
    let (path, count) = inner.rsplit_once(" (read ×")?;
    Some(KeyFile::read(path, count.parse().ok()?))
}

fn parse_failed(line: &str) -> Option<FailedAttempt> {
    let rest = line.strip_prefix("- [")?;
    let (kind, text) = rest.split_once("] ")?;
    Some(FailedAttempt {
        kind: Kind::parse(kind)?,
        text: text.to_string(),
    })
}

pub fn parse(text: &str) -> Result<Handoff, ParseError> {
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some(TITLE) {
        return err("missing `# Handoff` title");
    }

    // Split into (heading, body lines), keeping the first occurrence of each
    // known section and treating everything else as an extra section.
    let mut sections: Vec<(String, Vec<&str>)> = Vec::new();
    for line in lines {
        if let Some(name) = line.strip_prefix("## ") {
            sections.push((name.trim().to_string(), Vec::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push(line);
        }
    }

    // One slot per known section, in the order of SECTIONS.
    let mut known: [Option<Vec<&str>>; SECTIONS.len()] = Default::default();
    let mut extra_sections = Vec::new();
    for (name, body) in sections {
        let slot = SECTIONS
            .iter()
            .position(|s| *s == name)
            .and_then(|i| known.get_mut(i))
            .filter(|slot| slot.is_none());
        match slot {
            Some(slot) => *slot = Some(body),
            None => extra_sections.push((name, body_text(&body))),
        }
    }
    let [Some(header), objective, problem, key_files_body, failed_body, next_steps] = known else {
        return err("missing `## Header` section");
    };
    let objective = objective.unwrap_or_default();
    let problem = problem.unwrap_or_default();
    let next_steps = next_steps.unwrap_or_default();
    let key_files_body = key_files_body.unwrap_or_default();
    let failed_body = failed_body.unwrap_or_default();

    let mut session = None;
    let mut cwd = None;
    let mut branch = None;
    let mut created = None;
    let mut status = None;
    let mut consumed = None;
    let mut header_extra = Vec::new();
    for line in &header {
        let Some(entry) = line.strip_prefix("- ") else {
            continue;
        };
        let Some((key, value)) = entry.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "session" => session = Some(value.to_string()),
            "cwd" => cwd = Some(value.to_string()),
            "branch" => branch = Some(value.to_string()).filter(|b| !b.is_empty()),
            "created" => match parse_ts(value) {
                Some(t) => created = Some(t),
                None => return err("`created` is not an RFC 3339 timestamp"),
            },
            "status" => match Status::parse(value) {
                Some(s) => status = Some(s),
                None => return err("`status` must be `pending` or `consumed`"),
            },
            "consumed" => match parse_ts(value) {
                Some(t) => consumed = Some(t),
                None => return err("`consumed` is not an RFC 3339 timestamp"),
            },
            _ => header_extra.push((key.to_string(), value.to_string())),
        }
    }
    let Some(session) = session.filter(|s| !s.is_empty()) else {
        return err("header lacks `session`");
    };
    let Some(cwd) = cwd.filter(|s| !s.is_empty()) else {
        return err("header lacks `cwd`");
    };
    let Some(created) = created else {
        return err("header lacks `created`");
    };
    let Some(status) = status else {
        return err("header lacks `status`");
    };

    let mut key_files = Vec::new();
    let mut key_files_notes = Vec::new();
    for line in key_files_body.iter().filter(|l| !l.trim().is_empty()) {
        if line.trim() == NOT_PROVIDED {
            continue;
        }
        match parse_key_file(line) {
            Some(k) => key_files.push(k),
            None => key_files_notes.push(line.to_string()),
        }
    }
    let mut failed_attempts = Vec::new();
    let mut failed_notes = Vec::new();
    for line in failed_body.iter().filter(|l| !l.trim().is_empty()) {
        if line.trim() == NOT_PROVIDED {
            continue;
        }
        match parse_failed(line) {
            Some(a) => failed_attempts.push(a),
            None => failed_notes.push(line.to_string()),
        }
    }

    Ok(Handoff {
        session,
        cwd,
        branch,
        created,
        status,
        consumed,
        header_extra,
        objective: model_text(&objective),
        problem: model_text(&problem),
        key_files,
        key_files_notes,
        failed_attempts,
        failed_notes,
        next_steps: model_text(&next_steps),
        extra_sections,
    })
}

// ── Trimming ────────────────────────────────────────────────────────────────

/// What `trim_to_limit` did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Trim {
    /// Human-readable list of what was removed or cut.
    pub removed: Vec<String>,
    /// The limit could not be met without cutting Objective or Next steps,
    /// which are never cut.
    pub over_limit: bool,
}

fn size(h: &Handoff) -> usize {
    render(h).chars().count()
}

/// Brings the rendered handoff under `max_chars` characters: oldest failed
/// attempts first, then the lowest-ranked key files, then the Problem text.
/// Objective and Next steps are never cut.
pub fn trim_to_limit(h: &mut Handoff, max_chars: usize) -> Trim {
    let mut trim = Trim::default();

    while size(h) > max_chars {
        // Mechanical entries are ordered newest first, so the last one is the
        // oldest; the model's hypotheses come after them and go last.
        let idx = h
            .failed_attempts
            .iter()
            .rposition(|a| a.kind.is_mechanical())
            .or_else(|| h.failed_attempts.len().checked_sub(1));
        let Some(idx) = idx else { break };
        let a = h.failed_attempts.remove(idx);
        trim.removed.push(format!("failed attempt: {}", a.text));
    }

    while size(h) > max_chars {
        let Some(k) = h.key_files.pop() else { break };
        trim.removed.push(format!("key file: {}", k.path));
    }

    if size(h) > max_chars {
        trim_problem(h, max_chars, &mut trim);
    }

    trim.over_limit = size(h) > max_chars;
    trim
}

fn trim_problem(h: &mut Handoff, max_chars: usize, trim: &mut Trim) {
    let Some(problem) = h.problem.clone() else {
        return;
    };
    let original = problem.clone();
    let mut lines: Vec<String> = problem
        .trim_end_matches('…')
        .lines()
        .map(str::to_string)
        .collect();

    while size(h) > max_chars && lines.len() > 1 {
        lines.pop();
        h.problem = Some(format!("{}…", lines.join("\n")));
    }
    if size(h) > max_chars {
        // One line left: cut it by characters, never inside a character.
        let line = lines.first().cloned().unwrap_or_default();
        let excess = size(h) - max_chars;
        let keep = line.chars().count().saturating_sub(excess + 1);
        let cut: String = line.chars().take(keep).collect();
        h.problem = Some(format!("{cut}…"));
    }
    if h.problem.as_deref() != Some(original.as_str()) {
        trim.removed.push("problem: cut".to_string());
    }
}
