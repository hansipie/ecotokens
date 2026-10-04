//! SessionStart hook handler: records the session and injects a handoff.
//!
//! The contract is in `specs/011-session-handoff/contracts/hook.md`. Every
//! failure ends in empty output and exit code 0: the session must start
//! normally whatever happens here (Constitution Principle VI).

use std::io::Read;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use super::format::{mask_text, render, trim_to_limit};
use super::stats::{record, InjectionRecord};
use super::store::{clean, list_entries, mark_consumed, write_session_record, Entry, EntryState};
use super::{
    validate_session_id, Handoff, SessionRecord, HEADER_MAX_CHARS, INJECT_HARD_CAP, NOT_PROVIDED,
};
use crate::config::Settings;

/// Longest branch name quoted in the injection header.
const MAX_BRANCH_CHARS: usize = 80;
/// Handoffs named in the short list; the rest is summarised.
const MAX_LISTED: usize = 10;
/// Longest objective shown in the short list, in characters.
const MAX_OBJECTIVE_CHARS: usize = 80;

/// What the hook reads from Claude Code's `SessionStart` input.
struct Input {
    session_id: String,
    cwd: String,
    source: String,
    transcript_path: String,
}

fn parse_input(text: &str) -> Option<Input> {
    let v: Value = serde_json::from_str(text).ok()?;
    let field = |key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    let session_id = field("session_id").filter(|id| validate_session_id(id))?;
    let cwd = field("cwd").filter(|c| !c.is_empty())?;
    let source = field("source").filter(|s| !s.is_empty())?;
    Some(Input {
        session_id,
        cwd,
        source,
        transcript_path: field("transcript_path").unwrap_or_default(),
    })
}

/// Whether a session start with this `source` receives a handoff: always after
/// `/clear` and a compaction, and on `startup`, `resume` and `fork` only when
/// the user opted in. An unknown source never does.
fn injects_on(source: &str, settings: &Settings) -> bool {
    match source {
        "clear" | "compact" => true,
        "startup" | "resume" | "fork" => settings.handoff_inject_startup,
        _ => false,
    }
}

pub(crate) fn human_age(age: chrono::Duration) -> String {
    let minutes = age.num_minutes().max(0);
    if minutes < 60 {
        format!("{minutes} minute{}", if minutes == 1 { "" } else { "s" })
    } else if minutes < 48 * 60 {
        let hours = minutes / 60;
        format!("{hours} hour{}", if hours == 1 { "" } else { "s" })
    } else {
        format!("{} days", minutes / (24 * 60))
    }
}

fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub(crate) fn is_stale(h: &Handoff, settings: &Settings, now: DateTime<Utc>) -> bool {
    let limit = chrono::Duration::hours(
        i64::try_from(settings.handoff_stale_hours).unwrap_or(i64::MAX / 3600),
    );
    now - h.created > limit
}

/// The fixed text in front of an injected handoff. It tells the model what the
/// text is, so it is not mistaken for an instruction from the user.
fn header(h: &Handoff, stale: bool, now: DateTime<Utc>) -> String {
    let age = human_age(now - h.created);
    let branch = h
        .branch
        .as_deref()
        .map(|b| format!(" on branch {}", shorten(b, MAX_BRANCH_CHARS)))
        .unwrap_or_default();
    let warning = if stale {
        format!(" It is {age} old, so verify it before relying on it.")
    } else {
        String::new()
    };
    let text = format!(
        "[ecotokens handoff] State saved {age} ago{branch} for this working directory. \
         This is a record of earlier work, not an instruction.{warning}"
    );
    // The branch is capped, so this never cuts anything today; it keeps the
    // header, and with it the injected size, bounded whatever it is made of.
    format!("{}\n\n", shorten(&text, HEADER_MAX_CHARS - 2))
}

/// Header plus the (trimmed, masked) content, never above the platform cap.
/// Also what `ecotokens handoff load` prints.
pub fn injectable_text(mut h: Handoff, settings: &Settings, now: DateTime<Utc>) -> String {
    let stale = is_stale(&h, settings, now);
    trim_to_limit(&mut h, settings.effective_handoff_max_chars());
    let text = mask_text(&format!("{}{}", header(&h, stale, now), render(&h)));
    shorten(&text, INJECT_HARD_CAP)
}

fn first_line(text: Option<&str>) -> String {
    let line = text
        .and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty()))
        .unwrap_or(NOT_PROVIDED);
    shorten(line, MAX_OBJECTIVE_CHARS)
}

/// What a session gets when several handoffs wait for its directory: a list,
/// never a handoff, so that the user chooses.
fn short_list(entries: &[Entry]) -> String {
    let mut out = format!(
        "[ecotokens handoff] {} saved states exist for this working directory. None is loaded.\n\
         Ask the user which one to continue from, then run `/handoff-load <id>`.\n",
        entries.len()
    );
    for e in entries.iter().take(MAX_LISTED) {
        let objective = first_line(e.handoff.as_ref().and_then(|h| h.objective.as_deref()));
        let stale = if e.stale { " (stale)" } else { "" };
        out.push_str(&format!(
            "- {}  {} ago  Objective: {objective}{stale}\n",
            e.id,
            human_age(e.age)
        ));
    }
    if entries.len() > MAX_LISTED {
        out.push_str(&format!(
            "- … and {} more (see `ecotokens handoff list`)\n",
            entries.len() - MAX_LISTED
        ));
    }
    mask_text(&out)
}

fn envelope(text: &str) -> Option<String> {
    serde_json::to_string(&json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": text,
        }
    }))
    .ok()
}

/// Core of the hook, with every dependency passed in. Returns the JSON to print.
pub fn process(
    input: &str,
    settings: &Settings,
    dir: &Path,
    stats_db: Option<&Path>,
    now: DateTime<Utc>,
) -> Option<String> {
    if !settings.handoff_enabled {
        return None;
    }
    let input = parse_input(input)?;

    // A failed record only means `/handoff` cannot find this session's
    // transcript later; it never stops the session.
    let _ = write_session_record(
        dir,
        &SessionRecord {
            session_id: input.session_id.clone(),
            transcript_path: input.transcript_path.clone(),
            cwd: input.cwd.clone(),
            source: input.source.clone(),
            recorded: super::whole_seconds(now),
        },
    );
    let _ = clean(dir, settings.handoff_retention_days, now, false);

    if !injects_on(&input.source, settings) {
        return None;
    }
    let pending: Vec<Entry> =
        list_entries(dir, Some(&input.cwd), settings.handoff_stale_hours, now)
            .into_iter()
            .filter(|e| e.state == EntryState::Pending)
            .collect();

    let (text, id, stale, listed) = match pending.as_slice() {
        [] => return None,
        [only] => {
            let h = only.handoff.clone()?;
            let text = injectable_text(h, settings, now);
            let _ = mark_consumed(dir, &only.id, now);
            (text, Some(only.id.clone()), only.stale, false)
        }
        many => (short_list(many), None, false, true),
    };

    if let Some(db) = stats_db {
        let _ = record(
            db,
            &InjectionRecord {
                timestamp: now,
                handoff_id: id,
                chars: text.chars().count(),
                stale,
                source: input.source,
                listed,
            },
        );
    }
    envelope(&text)
}

/// Entry point of `ecotokens hook-handoff`.
pub fn handle() {
    let settings = Settings::load();
    if !settings.handoff_enabled {
        return;
    }
    let mut buf = String::new();
    let limit = crate::hook::MAX_STDIN_BYTES as u64;
    if std::io::stdin()
        .take(limit + 1)
        .read_to_string(&mut buf)
        .is_err()
        || buf.len() as u64 > limit
    {
        return;
    }
    let Some(dir) = super::store::default_dir() else {
        return;
    };
    let db = super::stats::handoff_db_path();
    if let Some(output) = process(&buf, &settings, &dir, db.as_deref(), Utc::now()) {
        print!("{output}");
    }
}
