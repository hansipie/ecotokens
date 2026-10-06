//! Handoff directory: paths, session records, atomic reads and writes.
//!
//! Every function takes the directory explicitly, so tests never touch the
//! real configuration. A session id is validated before it becomes part of a
//! path, which keeps every file inside the directory.

use std::io;
use std::path::{Path, PathBuf};

use super::format::{mask_text, parse, render};
use chrono::{DateTime, Utc};

use super::{validate_session_id, whole_seconds, Handoff, SessionRecord, Status};

/// A handoff is a few kilobytes; anything bigger is not one.
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const SESSIONS_DIR: &str = ".sessions";

/// Why a handoff could not be read.
#[derive(Debug)]
pub enum ReadError {
    /// No such file, or an id that cannot name one.
    Missing,
    /// The file exists but is not a valid handoff.
    Corrupted(String),
    Io(io::Error),
}

pub fn default_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ecotokens").join("handoff"))
}

pub fn handoff_path(dir: &Path, id: &str) -> Option<PathBuf> {
    validate_session_id(id).then(|| dir.join(format!("{id}.md")))
}

pub fn session_record_path(dir: &Path, id: &str) -> Option<PathBuf> {
    validate_session_id(id).then(|| dir.join(SESSIONS_DIR).join(format!("{id}.json")))
}

fn invalid_id(id: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("invalid session id {id:?}: only letters, digits, `-` and `_` are allowed"),
    )
}

/// Creates `dir` (mode 0700 on Unix) when it does not exist yet.
fn ensure_dir(dir: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

#[cfg(unix)]
fn make_private(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn make_private(_path: &Path) {}

fn write_private(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    crate::config::atomic_write(path, contents)?;
    make_private(path);
    Ok(())
}

/// Renders, masks and atomically writes the handoff to `<dir>/<session>.md`.
pub fn write_handoff(dir: &Path, h: &Handoff) -> io::Result<PathBuf> {
    let path = handoff_path(dir, &h.session).ok_or_else(|| invalid_id(&h.session))?;
    write_private(&path, &mask_text(&render(h)))?;
    Ok(path)
}

pub fn read_handoff(dir: &Path, id: &str) -> Result<Handoff, ReadError> {
    let path = handoff_path(dir, id).ok_or(ReadError::Missing)?;
    read_handoff_file(&path)
}

pub fn read_handoff_file(path: &Path) -> Result<Handoff, ReadError> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(ReadError::Missing),
        Err(e) => return Err(ReadError::Io(e)),
    };
    if meta.len() > MAX_FILE_BYTES {
        return Err(ReadError::Corrupted("file is too large".into()));
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::InvalidData => {
            return Err(ReadError::Corrupted("file is not valid UTF-8".into()))
        }
        Err(e) => return Err(ReadError::Io(e)),
    };
    parse(&text).map_err(|e| ReadError::Corrupted(e.to_string()))
}

fn same_dir(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryState {
    Pending,
    Consumed,
    Corrupted,
}

impl EntryState {
    pub fn as_str(self) -> &'static str {
        match self {
            EntryState::Pending => "pending",
            EntryState::Consumed => "consumed",
            EntryState::Corrupted => "corrupted",
        }
    }
}

/// One file of the handoff directory, as `list` and the hook see it.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The file stem, which is also the session id of the writer.
    pub id: String,
    pub state: EntryState,
    /// `None` for a corrupted file.
    pub handoff: Option<Handoff>,
    pub age: chrono::Duration,
    pub stale: bool,
}

fn handoff_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "md")
                && p.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(validate_session_id)
        })
        .collect()
}

/// The handoffs of the directory, newest first, corrupted files last. With a
/// `cwd`, valid handoffs written for another working directory are left out
/// (a corrupted file cannot be attributed, so it is always shown).
pub fn list_entries(
    dir: &Path,
    cwd: Option<&str>,
    stale_hours: u64,
    now: DateTime<Utc>,
) -> Vec<Entry> {
    let stale_after =
        chrono::Duration::hours(i64::try_from(stale_hours).unwrap_or(i64::MAX / 3600));
    let mut entries: Vec<Entry> = handoff_files(dir)
        .into_iter()
        .filter_map(|path| {
            let id = path.file_stem()?.to_str()?.to_string();
            match read_handoff_file(&path) {
                Ok(h) => {
                    if cwd.is_some_and(|c| !same_dir(&h.cwd, c)) {
                        return None;
                    }
                    let age = (now - h.created).max(chrono::Duration::zero());
                    Some(Entry {
                        id,
                        state: match h.status {
                            Status::Pending => EntryState::Pending,
                            Status::Consumed => EntryState::Consumed,
                        },
                        stale: age > stale_after,
                        age,
                        handoff: Some(h),
                    })
                }
                Err(_) => Some(Entry {
                    id,
                    state: EntryState::Corrupted,
                    handoff: None,
                    age: chrono::Duration::zero(),
                    stale: false,
                }),
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        let key = |e: &Entry| e.handoff.as_ref().map(|h| h.created);
        (a.state == EntryState::Corrupted)
            .cmp(&(b.state == EntryState::Corrupted))
            .then_with(|| key(b).cmp(&key(a)))
            .then_with(|| a.id.cmp(&b.id))
    });
    entries
}

/// Marks a pending handoff consumed (no change when it already is).
pub fn mark_consumed(dir: &Path, id: &str, now: DateTime<Utc>) -> io::Result<()> {
    let mut h = read_handoff(dir, id).map_err(|e| match e {
        ReadError::Missing => io::Error::new(io::ErrorKind::NotFound, "no such handoff"),
        ReadError::Corrupted(why) => io::Error::new(io::ErrorKind::InvalidData, why),
        ReadError::Io(e) => e,
    })?;
    if h.status == Status::Consumed {
        return Ok(());
    }
    h.status = Status::Consumed;
    h.consumed = Some(whole_seconds(now));
    write_handoff(dir, &h).map(|_| ())
}

/// What `clean` removed (or would remove) and how many files it left.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cleaned {
    pub removed: Vec<PathBuf>,
    pub kept: usize,
}

fn modified_at(path: &Path) -> Option<DateTime<Utc>> {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .map(DateTime::<Utc>::from)
}

/// Deletes handoffs and session records older than `retention_days`, consumed
/// or not, and consumed handoffs whose consumption is at least
/// `consumed_retention_hours` old (0 removes every consumed handoff). A file
/// that cannot be parsed is aged by its modification time.
pub fn clean(
    dir: &Path,
    retention_days: u64,
    consumed_retention_hours: u64,
    now: DateTime<Utc>,
    dry_run: bool,
) -> io::Result<Cleaned> {
    let retention =
        chrono::Duration::days(i64::try_from(retention_days).unwrap_or(i64::MAX / 86_400_000));
    let consumed_retention = chrono::Duration::hours(
        i64::try_from(consumed_retention_hours).unwrap_or(i64::MAX / 3_600_000),
    );
    let mut cleaned = Cleaned::default();
    let mut consider = |path: PathBuf, expired: bool| {
        if expired && (dry_run || std::fs::remove_file(&path).is_ok()) {
            cleaned.removed.push(path);
        } else {
            cleaned.kept += 1;
        }
    };
    let older_than_retention =
        |written: Option<DateTime<Utc>>| written.is_some_and(|t| now - t > retention);
    for path in handoff_files(dir) {
        let expired = match read_handoff_file(&path) {
            Ok(h) => {
                let used_up = h.status == Status::Consumed
                    && now - h.consumed.unwrap_or(h.created) >= consumed_retention;
                used_up || older_than_retention(Some(h.created))
            }
            Err(_) => older_than_retention(modified_at(&path)),
        };
        consider(path, expired);
    }
    if let Ok(entries) = std::fs::read_dir(dir.join(SESSIONS_DIR)) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let is_record = entry.file_type().is_ok_and(|t| t.is_file())
                && path.extension().is_some_and(|x| x == "json");
            if !is_record {
                continue;
            }
            let written = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str::<SessionRecord>(&t).ok())
                .map(|r| r.recorded)
                .or_else(|| modified_at(&path));
            consider(path, older_than_retention(written));
        }
    }
    cleaned.removed.sort();
    Ok(cleaned)
}

pub fn write_session_record(dir: &Path, rec: &SessionRecord) -> io::Result<()> {
    let path =
        session_record_path(dir, &rec.session_id).ok_or_else(|| invalid_id(&rec.session_id))?;
    let json = serde_json::to_string_pretty(rec).map_err(io::Error::other)?;
    write_private(&path, &json)
}

pub fn read_session_record(dir: &Path, id: &str) -> Option<SessionRecord> {
    let path = session_record_path(dir, id)?;
    if std::fs::metadata(&path).ok()?.len() > MAX_FILE_BYTES {
        return None;
    }
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}
