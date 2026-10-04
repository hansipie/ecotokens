//! `ecotokens handoff` subcommands.
//!
//! Every function returns an [`Outcome`] instead of printing: `main.rs` prints
//! it and exits with its code. That keeps the commands testable and puts the
//! masking of everything they emit in one place.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use super::format::{mask_text, render, trim_to_limit};
use super::hook::{human_age, injectable_text, is_stale};
use super::skills::{are_skills_installed, install_skills, remove_skills};
use super::stats::summarize;
use super::store::{
    clean as store_clean, list_entries, mark_consumed, read_handoff, read_session_record,
    write_handoff, EntryState, ReadError,
};
use super::transcript::{extract, TranscriptFacts};
use super::{
    fmt_ts, validate_session_id, FailedAttempt, Handoff, Kind, Status, INJECT_HARD_CAP,
    MAX_CHARS_CEILING, NOT_PROVIDED,
};
use crate::config::Settings;

/// Longest abandoned hypothesis kept, in characters.
const MAX_HYPOTHESIS_CHARS: usize = 240;

/// What a command wants to print and the exit code it ends with
/// (0 success, 1 operational error, 2 invalid argument).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    pub code: i32,
    pub out: String,
    pub err: String,
}

impl Outcome {
    /// Success with human-readable output; the text is masked.
    pub fn ok(out: String) -> Self {
        Outcome {
            code: 0,
            out: mask_text(&out),
            err: String::new(),
        }
    }

    /// Success with JSON output. Strings are masked one by one before
    /// serialising, so masking can never break the JSON syntax.
    pub fn ok_json(mut value: Value) -> Self {
        mask_strings(&mut value);
        let mut out = serde_json::to_string_pretty(&value).unwrap_or_default();
        out.push('\n');
        Outcome {
            code: 0,
            out,
            err: String::new(),
        }
    }

    pub fn fail(code: i32, err: &str) -> Self {
        Outcome {
            code,
            out: String::new(),
            err: format!("{}\n", mask_text(err)),
        }
    }

    /// Adds a warning on stderr.
    pub fn warn(mut self, message: &str) -> Self {
        self.err
            .push_str(&format!("warning: {}\n", mask_text(message)));
        self
    }
}

fn mask_strings(v: &mut Value) {
    match v {
        Value::String(s) => *s = mask_text(s),
        Value::Array(items) => items.iter_mut().for_each(mask_strings),
        Value::Object(map) => map.values_mut().for_each(mask_strings),
        _ => {}
    }
}

/// The model-written fields given to `handoff set`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SetInput {
    pub objective: Option<String>,
    pub problem: Option<String>,
    pub next_steps: Option<String>,
    pub abandoned: Vec<String>,
}

fn invalid_id(id: &str) -> Outcome {
    Outcome::fail(
        2,
        &format!("error: invalid session id {id:?}: only letters, digits, `-` and `_` are allowed"),
    )
}

/// The handoff as extracted from a transcript, before any model-written field.
pub fn build_handoff(
    session: &str,
    cwd: &str,
    facts: &TranscriptFacts,
    now: DateTime<Utc>,
) -> Handoff {
    let mut h = Handoff::new(session, cwd, super::whole_seconds(now));
    h.branch = facts.branch.clone();
    h.key_files = facts.key_files.clone();
    h.failed_attempts = facts.failures.clone();
    h
}

fn summary(h: &Handoff, path: &Path, chars: usize, masked: bool) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    m.insert("session".into(), json!(h.session));
    m.insert("path".into(), json!(path.to_string_lossy()));
    m.insert("cwd".into(), json!(h.cwd));
    m.insert("branch".into(), json!(h.branch));
    m.insert(
        "key_files".into(),
        json!(h
            .key_files
            .iter()
            .map(|k| k.path.as_str())
            .collect::<Vec<_>>()),
    );
    m.insert(
        "failed_attempts".into(),
        json!(h
            .failed_attempts
            .iter()
            .map(|a| json!({"kind": a.kind.as_str(), "text": a.text}))
            .collect::<Vec<_>>()),
    );
    m.insert("chars".into(), json!(chars));
    m.insert("masked".into(), json!(masked));
    m
}

/// Trims, writes and describes `h`. Shared by `write` and `set`.
fn save(
    dir: &Path,
    settings: &Settings,
    mut h: Handoff,
) -> Result<(Handoff, std::path::PathBuf, usize, bool, Vec<String>), Outcome> {
    let trim = trim_to_limit(&mut h, settings.effective_handoff_max_chars());
    let rendered = render(&h);
    let masked = mask_text(&rendered) != rendered;
    let path = write_handoff(dir, &h)
        .map_err(|e| Outcome::fail(1, &format!("error: cannot write the handoff: {e}")))?;
    let chars = mask_text(&rendered).chars().count();
    let mut removed = trim.removed;
    if trim.over_limit {
        removed.push("limit exceeded: objective and next steps are never cut".into());
    }
    Ok((h, path, chars, masked, removed))
}

/// Step 1: extract the mechanical part of the handoff and write it.
pub fn write(
    dir: &Path,
    settings: &Settings,
    session: &str,
    cwd: Option<&str>,
    json: bool,
    now: DateTime<Utc>,
) -> Outcome {
    if !validate_session_id(session) {
        return invalid_id(session);
    }
    let record = read_session_record(dir, session);
    let cwd = cwd
        .map(str::to_string)
        .or_else(|| record.as_ref().map(|r| r.cwd.clone()))
        .unwrap_or_default();
    let facts = match &record {
        Some(r) => extract(Path::new(&r.transcript_path), &cwd),
        None => TranscriptFacts::default(),
    };
    let warning = (!facts.found).then(|| {
        format!(
            "transcript of session {session} is unavailable (no session record, or the file is gone); \
             key files and failed attempts are not provided"
        )
    });

    let h = build_handoff(session, &cwd, &facts, now);
    let (h, path, chars, masked, _) = match save(dir, settings, h) {
        Ok(saved) => saved,
        Err(outcome) => return outcome,
    };

    let outcome = if json {
        let mut m = summary(&h, &path, chars, masked);
        m.insert("transcript_found".into(), json!(facts.found));
        m.insert("skipped_lines".into(), json!(facts.skipped_lines));
        Outcome::ok_json(Value::Object(m))
    } else {
        Outcome::ok(format!(
            "Handoff written: {}\n  working directory : {}\n  key files         : {}\n  failed attempts   : {}\n\
             Next: fill the objective, problem and next steps with `ecotokens handoff set`, then run /clear.\n",
            path.display(),
            h.cwd,
            h.key_files.len(),
            h.failed_attempts.len()
        ))
    };
    match warning {
        Some(w) => outcome.warn(&w),
        None => outcome,
    }
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn collapse(s: &str) -> String {
    let one_line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= MAX_HYPOTHESIS_CHARS {
        return one_line;
    }
    let mut cut: String = one_line.chars().take(MAX_HYPOTHESIS_CHARS - 1).collect();
    cut.push('…');
    cut
}

/// The fields of the JSON object read from stdin.
fn parse_stdin(text: &str) -> Result<SetInput, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("invalid JSON on stdin: {e}"))?;
    let obj = v
        .as_object()
        .ok_or_else(|| "invalid JSON on stdin: expected an object".to_string())?;
    let text_field = |key: &str| -> Result<Option<String>, String> {
        match obj.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(format!("invalid JSON on stdin: `{key}` must be a string")),
        }
    };
    let abandoned = match obj.get("abandoned") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|i| {
                i.as_str().map(str::to_string).ok_or_else(|| {
                    "invalid JSON on stdin: `abandoned` must hold strings".to_string()
                })
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err("invalid JSON on stdin: `abandoned` must be an array".into()),
    };
    Ok(SetInput {
        objective: text_field("objective")?,
        problem: text_field("problem")?,
        next_steps: text_field("next_steps")?,
        abandoned,
    })
}

/// Step 2: fill the model-written fields of an existing handoff.
pub fn set(
    dir: &Path,
    settings: &Settings,
    session: &str,
    input: SetInput,
    stdin_json: Option<&str>,
    json: bool,
    now: DateTime<Utc>,
) -> Outcome {
    let _ = now;
    if !validate_session_id(session) {
        return invalid_id(session);
    }
    let from_stdin = match stdin_json.map(parse_stdin).transpose() {
        Ok(v) => v.unwrap_or_default(),
        Err(e) => return Outcome::fail(2, &format!("error: {e}")),
    };
    let mut h = match read_handoff(dir, session) {
        Ok(h) => h,
        Err(ReadError::Missing) => {
            return Outcome::fail(
                1,
                &format!(
                    "error: no handoff for session {session}; run `ecotokens handoff write --session {session}` first"
                ),
            )
        }
        Err(ReadError::Corrupted(why)) => {
            return Outcome::fail(1, &format!("error: the handoff of session {session} is corrupted: {why}"))
        }
        Err(ReadError::Io(e)) => return Outcome::fail(1, &format!("error: cannot read the handoff: {e}")),
    };

    if let Some(o) = non_empty(input.objective.or(from_stdin.objective)) {
        h.objective = Some(o);
    }
    if let Some(p) = non_empty(input.problem.or(from_stdin.problem)) {
        h.problem = Some(p);
    }
    if let Some(n) = non_empty(input.next_steps.or(from_stdin.next_steps)) {
        h.next_steps = Some(n);
    }
    let abandoned: Vec<String> = if input.abandoned.is_empty() {
        from_stdin.abandoned
    } else {
        input.abandoned
    }
    .iter()
    .map(|a| collapse(a))
    .filter(|a| !a.is_empty())
    .collect();
    if !abandoned.is_empty() {
        h.failed_attempts.retain(|a| a.kind != Kind::Hypothesis);
        h.failed_attempts.extend(
            abandoned
                .iter()
                .map(|a| FailedAttempt::new(Kind::Hypothesis, a)),
        );
    }

    let (h, path, chars, masked, removed) = match save(dir, settings, h) {
        Ok(saved) => saved,
        Err(outcome) => return outcome,
    };
    if json {
        let mut m = summary(&h, &path, chars, masked);
        m.insert("trimmed".into(), json!(removed));
        Outcome::ok_json(Value::Object(m))
    } else {
        let mut out = format!("Handoff updated: {}\n", path.display());
        if !removed.is_empty() {
            out.push_str(&format!(
                "  trimmed to fit the limit: {}\n",
                removed.join("; ")
            ));
        }
        out.push_str("Run /clear to continue from it.\n");
        Outcome::ok(out)
    }
}

/// Turns the feature on: hook, skills and settings (idempotent).
pub fn on(
    settings: &mut Settings,
    settings_path: &Path,
    skills_dir: &Path,
    stale_hours: Option<u64>,
    max_chars: Option<usize>,
) -> Outcome {
    if let Some(n) = max_chars.filter(|n| *n == 0 || *n > MAX_CHARS_CEILING) {
        return Outcome::fail(
            2,
            &format!(
                "error: --max-chars {n} is out of range: use 1 to {MAX_CHARS_CEILING} \
                 (Claude Code truncates injected hook output above {INJECT_HARD_CAP} characters)"
            ),
        );
    }
    if stale_hours == Some(0) {
        return Outcome::fail(2, "error: --stale-hours must be at least 1");
    }
    if let Err(e) = crate::install::install_handoff_hook(settings_path) {
        return Outcome::fail(1, &format!("error: cannot install the hook: {e}"));
    }
    let (written, kept) = match install_skills(skills_dir) {
        Ok(r) => r,
        Err(e) => return Outcome::fail(1, &format!("error: cannot install the skills: {e}")),
    };
    settings.handoff_enabled = true;
    if let Some(h) = stale_hours {
        settings.handoff_stale_hours = h;
    }
    if let Some(n) = max_chars {
        settings.handoff_max_chars = n;
    }

    let mut out = String::from("handoff: ON\n");
    out.push_str(&format!(
        "  hook   : SessionStart → ecotokens hook-handoff ({})\n",
        settings_path.display()
    ));
    for p in &written {
        out.push_str(&format!("  skill  : {}\n", p.display()));
    }
    out.push_str(&format!(
        "  limits : stale after {} h · at most {} characters\n",
        settings.handoff_stale_hours,
        settings.effective_handoff_max_chars()
    ));
    out.push_str("Restart Claude Code so it loads the hook and the skills.\n");
    let mut outcome = Outcome::ok(out);
    for p in &kept {
        outcome = outcome.warn(&format!(
            "{} exists and was not written by ecotokens; left untouched, so that command will not be available",
            p.display()
        ));
    }
    outcome
}

/// Turns the feature off: removes the hook and the skills, keeps the saved
/// handoffs.
pub fn off(settings: &mut Settings, settings_path: &Path, skills_dir: &Path) -> Outcome {
    if let Err(e) = crate::install::uninstall_handoff_hook(settings_path) {
        return Outcome::fail(1, &format!("error: cannot remove the hook: {e}"));
    }
    let removed = match remove_skills(skills_dir) {
        Ok(r) => r,
        Err(e) => return Outcome::fail(1, &format!("error: cannot remove the skills: {e}")),
    };
    settings.handoff_enabled = false;
    let mut out = String::from("handoff: OFF\n");
    out.push_str(&format!(
        "  hook   : removed from {}\n",
        settings_path.display()
    ));
    for p in &removed {
        out.push_str(&format!("  skill  : removed {}\n", p.display()));
    }
    out.push_str("Saved handoffs were kept; `ecotokens handoff clean` removes the old ones.\n");
    Outcome::ok(out)
}

/// Setup, thresholds, what is saved for this directory and what was injected.
#[allow(clippy::too_many_arguments)]
pub fn status(
    settings: &Settings,
    settings_path: &Path,
    skills_dir: &Path,
    dir: &Path,
    cwd: &str,
    stats_db: Option<&Path>,
    json: bool,
    now: DateTime<Utc>,
) -> Outcome {
    let hook = crate::install::is_handoff_hook_installed(settings_path);
    let skills = are_skills_installed(skills_dir);
    let entries = list_entries(dir, Some(cwd), settings.handoff_stale_hours, now);
    let count = |state: EntryState| entries.iter().filter(|e| e.state == state).count();
    let (pending, consumed) = (count(EntryState::Pending), count(EntryState::Consumed));
    let injections = stats_db.and_then(|p| summarize(p).ok()).unwrap_or_default();

    if json {
        return Outcome::ok_json(json!({
            "enabled": settings.handoff_enabled,
            "hook_installed": hook,
            "skills_installed": skills,
            "max_chars": settings.effective_handoff_max_chars(),
            "stale_hours": settings.handoff_stale_hours,
            "retention_days": settings.handoff_retention_days,
            "inject_startup": settings.handoff_inject_startup,
            "pending": pending,
            "consumed": consumed,
            "injections": injections,
        }));
    }
    let yes_no = |b: bool| if b { "yes" } else { "no" };
    let mut out = format!(
        "handoff    : {}\nsetup      : hook {} · skills {}\n\
         thresholds : stale after {} h · at most {} characters · kept {} days · startup injection {}\n\
         here       : {pending} pending · {consumed} consumed\n",
        if settings.handoff_enabled { "ON" } else { "OFF" },
        yes_no(hook),
        yes_no(skills),
        settings.handoff_stale_hours,
        settings.effective_handoff_max_chars(),
        settings.handoff_retention_days,
        if settings.handoff_inject_startup { "on" } else { "off" },
    );
    if injections.count > 0 {
        out.push_str(&format!(
            "injections : {} (average {} characters, {} stale)\n",
            injections.count, injections.chars_avg, injections.stale
        ));
    }
    if entries.is_empty() {
        out.push_str("\nNo handoff yet. Run /handoff in a Claude Code session.\n");
    }
    Outcome::ok(out)
}

fn one_line(text: Option<&str>) -> Option<String> {
    text.and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty()))
        .filter(|l| *l != NOT_PROVIDED)
        .map(str::to_string)
}

/// The handoffs of this directory (or of every directory with `all`).
pub fn list(
    dir: &Path,
    settings: &Settings,
    cwd: &str,
    all: bool,
    json: bool,
    now: DateTime<Utc>,
) -> Outcome {
    let entries = list_entries(
        dir,
        (!all).then_some(cwd),
        settings.handoff_stale_hours,
        now,
    );
    if json {
        let rows: Vec<Value> = entries
            .iter()
            .map(|e| match &e.handoff {
                Some(h) => json!({
                    "id": e.id,
                    "created": fmt_ts(h.created),
                    "age_hours": e.age.num_hours(),
                    "status": e.state.as_str(),
                    "stale": e.stale,
                    "objective": one_line(h.objective.as_deref()),
                    "cwd": h.cwd,
                }),
                None => json!({
                    "id": e.id,
                    "created": null,
                    "age_hours": null,
                    "status": e.state.as_str(),
                    "stale": false,
                    "objective": null,
                    "cwd": null,
                }),
            })
            .collect();
        return Outcome::ok_json(Value::Array(rows));
    }
    if entries.is_empty() {
        return Outcome::ok(if all {
            "No handoff yet. Run /handoff in a Claude Code session.\n".into()
        } else {
            "No handoff for this directory.\n".into()
        });
    }
    let mut out = format!("{:<38} {:<10} {:<10} OBJECTIVE\n", "ID", "AGE", "STATUS");
    for e in &entries {
        let status = if e.stale && e.state == EntryState::Pending {
            "stale".to_string()
        } else {
            e.state.as_str().to_string()
        };
        let (objective, place) = match &e.handoff {
            Some(h) => (
                one_line(h.objective.as_deref()).unwrap_or_else(|| NOT_PROVIDED.into()),
                format!("  [{}]", h.cwd),
            ),
            None => ("(unreadable file)".to_string(), String::new()),
        };
        let age = if e.handoff.is_some() {
            human_age(e.age)
        } else {
            "-".into()
        };
        out.push_str(&format!(
            "{:<38} {:<10} {:<10} {objective}{}\n",
            e.id,
            age,
            status,
            if all { place } else { String::new() }
        ));
    }
    Outcome::ok(out)
}

fn available_ids(dir: &Path, settings: &Settings, now: DateTime<Utc>) -> String {
    let ids: Vec<String> = list_entries(dir, None, settings.handoff_stale_hours, now)
        .into_iter()
        .filter(|e| e.state != EntryState::Corrupted)
        .map(|e| e.id)
        .collect();
    if ids.is_empty() {
        "no handoff is saved".to_string()
    } else {
        format!("available ids: {}", ids.join(", "))
    }
}

/// Prints the injectable text of a handoff and marks it consumed.
pub fn load(dir: &Path, settings: &Settings, id: &str, json: bool, now: DateTime<Utc>) -> Outcome {
    let unknown = || {
        Outcome::fail(
            1,
            &format!(
                "error: no handoff {id:?}; {}",
                available_ids(dir, settings, now)
            ),
        )
    };
    if !validate_session_id(id) {
        return unknown();
    }
    let h = match read_handoff(dir, id) {
        Ok(h) => h,
        Err(ReadError::Missing) => return unknown(),
        Err(ReadError::Corrupted(why)) => {
            return Outcome::fail(1, &format!("error: the handoff {id} is corrupted: {why}"))
        }
        Err(ReadError::Io(e)) => {
            return Outcome::fail(1, &format!("error: cannot read the handoff {id}: {e}"))
        }
    };
    let stale = is_stale(&h, settings, now);
    let was_pending = h.status == Status::Pending;
    let text = injectable_text(h, settings, now);
    let consumed_now = was_pending && mark_consumed(dir, id, now).is_ok();
    if json {
        Outcome::ok_json(json!({
            "id": id,
            "stale": stale,
            "consumed_now": consumed_now,
            "text": text,
        }))
    } else {
        Outcome::ok(format!("{text}\n"))
    }
}

/// Deletes handoffs and session records past the retention.
pub fn clean(
    dir: &Path,
    settings: &Settings,
    dry_run: bool,
    json: bool,
    now: DateTime<Utc>,
) -> Outcome {
    let cleaned = match store_clean(dir, settings.handoff_retention_days, now, dry_run) {
        Ok(c) => c,
        Err(e) => {
            return Outcome::fail(
                1,
                &format!("error: cannot clean the handoff directory: {e}"),
            )
        }
    };
    if json {
        return Outcome::ok_json(json!({
            "removed": cleaned.removed.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
            "kept": cleaned.kept,
            "dry_run": dry_run,
        }));
    }
    if cleaned.removed.is_empty() {
        return Outcome::ok("Nothing to remove.\n".into());
    }
    let verb = if dry_run { "Would remove" } else { "Removed" };
    let mut out = format!("{verb} {} file(s):\n", cleaned.removed.len());
    for p in &cleaned.removed {
        out.push_str(&format!("  {}\n", p.display()));
    }
    Outcome::ok(out)
}
