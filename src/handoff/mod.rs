//! Session handoff: a short structured state file written by `/handoff` and
//! re-injected by a `SessionStart` hook after `/clear` or a compaction.

use chrono::{DateTime, SecondsFormat, Timelike, Utc};
use serde::{Deserialize, Serialize};

/// First line of every managed skill file; `handoff off` only removes files
/// that carry it. A macro so that it can also be used inside `concat!`.
macro_rules! marker {
    () => {
        "<!-- managed by ecotokens handoff: `ecotokens handoff off` removes this file -->"
    };
}

pub mod cli;
pub mod format;
pub mod hook;
pub mod skills;
pub mod stats;
pub mod store;
pub mod transcript;

/// Text written in a model-written section that has not been filled.
pub const NOT_PROVIDED: &str = "not provided";

pub const MARKER: &str = marker!();

/// Claude Code replaces injected hook output longer than this by a file path
/// and a short preview.
pub const INJECT_HARD_CAP: usize = 10_000;
/// Upper bound of the `handoff_max_chars` setting.
pub const MAX_CHARS_CEILING: usize = 9_000;
/// Upper bound of the fixed text put in front of an injected handoff.
pub const HEADER_MAX_CHARS: usize = 400;

/// Longest accepted session id; real ones are 36-character UUIDs.
const MAX_SESSION_ID_LEN: usize = 128;

/// A session id becomes a file name, so it is restricted to a safe alphabet.
pub fn validate_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_SESSION_ID_LEN
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// RFC 3339 UTC with second precision, the format of every timestamp in the file.
pub fn fmt_ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// Drops sub-second precision so a timestamp survives a render/parse round trip.
pub fn whole_seconds(t: DateTime<Utc>) -> DateTime<Utc> {
    t.with_nanosecond(0).unwrap_or(t)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Consumed,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Consumed => "consumed",
        }
    }

    pub fn parse(s: &str) -> Option<Status> {
        match s {
            "pending" => Some(Status::Pending),
            "consumed" => Some(Status::Consumed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    TestOrBuild,
    Command,
    EditReverted,
    EditRewritten,
    Hypothesis,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::TestOrBuild => "test_or_build",
            Kind::Command => "command",
            Kind::EditReverted => "edit_reverted",
            Kind::EditRewritten => "edit_rewritten",
            Kind::Hypothesis => "hypothesis",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "test_or_build" => Some(Kind::TestOrBuild),
            "command" => Some(Kind::Command),
            "edit_reverted" => Some(Kind::EditReverted),
            "edit_rewritten" => Some(Kind::EditRewritten),
            "hypothesis" => Some(Kind::Hypothesis),
            _ => None,
        }
    }

    /// Mechanical kinds are extracted from the transcript; hypotheses are
    /// written by the model.
    pub fn is_mechanical(self) -> bool {
        !matches!(self, Kind::Hypothesis)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FailedAttempt {
    pub kind: Kind,
    pub text: String,
}

impl FailedAttempt {
    pub fn new(kind: Kind, text: &str) -> Self {
        FailedAttempt {
            kind,
            text: text.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "use", content = "count", rename_all = "lowercase")]
pub enum KeyFileUse {
    Modified,
    Read(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyFile {
    pub path: String,
    pub usage: KeyFileUse,
}

impl KeyFile {
    pub fn modified(path: &str) -> Self {
        KeyFile {
            path: path.to_string(),
            usage: KeyFileUse::Modified,
        }
    }

    pub fn read(path: &str, count: u32) -> Self {
        KeyFile {
            path: path.to_string(),
            usage: KeyFileUse::Read(count),
        }
    }
}

/// The structured state of one session (see `specs/011-session-handoff/data-model.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handoff {
    pub session: String,
    pub cwd: String,
    pub branch: Option<String>,
    pub created: DateTime<Utc>,
    pub status: Status,
    pub consumed: Option<DateTime<Utc>>,
    /// Header keys this version does not know, kept as written.
    pub header_extra: Vec<(String, String)>,
    pub objective: Option<String>,
    pub problem: Option<String>,
    pub key_files: Vec<KeyFile>,
    /// Lines a user added inside `Key files` that are not entries.
    pub key_files_notes: Vec<String>,
    pub failed_attempts: Vec<FailedAttempt>,
    /// Lines a user added inside `Failed attempts` that are not entries.
    pub failed_notes: Vec<String>,
    pub next_steps: Option<String>,
    /// `##` sections this version does not know, kept as written.
    pub extra_sections: Vec<(String, String)>,
}

impl Handoff {
    pub fn new(session: &str, cwd: &str, created: DateTime<Utc>) -> Self {
        Handoff {
            session: session.to_string(),
            cwd: cwd.to_string(),
            branch: None,
            created,
            status: Status::Pending,
            consumed: None,
            header_extra: Vec::new(),
            objective: None,
            problem: None,
            key_files: Vec::new(),
            key_files_notes: Vec::new(),
            failed_attempts: Vec::new(),
            failed_notes: Vec::new(),
            next_steps: None,
            extra_sections: Vec::new(),
        }
    }
}

/// Written by the hook at every `SessionStart`, so that `/handoff` can find
/// the transcript of its own session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub transcript_path: String,
    pub cwd: String,
    pub source: String,
    pub recorded: DateTime<Utc>,
}
