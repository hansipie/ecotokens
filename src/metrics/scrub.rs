//! `ecotokens clear --scrub-secrets`: mask secrets left in clear in a metrics
//! database written by an older version, and purge the old bytes from the file.
//!
//! Since the store masks every free-text column on insert (`store::insert_one`),
//! only rows written before that change can hold a secret in clear. This module
//! rewrites them in place with the same masking, then makes the previous bytes
//! disappear from `metrics.db` and its `-wal` file:
//!
//! 1. `PRAGMA secure_delete = ON` on the connection, before any write, so the
//!    cell content an `UPDATE` replaces and the overflow pages it frees are
//!    overwritten with zeros instead of being left in place.
//! 2. One `BEGIN IMMEDIATE` transaction: scan every row, mask `command`,
//!    `git_root`, `content_before` and `content_after`, and `UPDATE` only the
//!    columns whose masked value differs.
//! 3. `PRAGMA wal_checkpoint(TRUNCATE)`, `VACUUM`, `PRAGMA wal_checkpoint(TRUNCATE)`:
//!    the database is rebuilt from its live content only (free pages, including
//!    those left by an earlier `ecotokens clear`, are dropped) and the WAL is
//!    emptied to zero bytes.
//!
//! A dry run opens the database read-only and writes nothing, not even a pragma.
//! No backup is taken: a backup would keep the secrets this command removes.

use super::store::{db_init_lock, masked};
use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, ErrorCode, OpenFlags, TransactionBehavior};
use std::fmt;
use std::path::Path;
use std::time::Duration;

/// The free-text columns of `interceptions` that can carry a secret.
pub const SCRUBBED_COLUMNS: [&str; 4] = ["command", "git_root", "content_before", "content_after"];

/// How long to wait for another process (a hook) holding the database, the
/// same as every other connection of the store.
pub const DEFAULT_BUSY_TIMEOUT: Duration = Duration::from_millis(5000);

/// What a scrub found (dry run) or changed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScrubReport {
    /// Rows read from `interceptions`.
    pub rows_scanned: usize,
    /// Rows with at least one column whose masked value differs.
    pub rows_changed: usize,
    /// Per column of [`SCRUBBED_COLUMNS`], in the same order: rows where that
    /// column changed.
    pub columns: [usize; 4],
}

impl ScrubReport {
    /// `(column, rows)` for every column that changed in at least one row.
    pub fn columns_touched(&self) -> Vec<(&'static str, usize)> {
        SCRUBBED_COLUMNS
            .iter()
            .zip(self.columns)
            .filter(|(_, n)| *n > 0)
            .map(|(c, n)| (*c, n))
            .collect()
    }
}

#[derive(Debug)]
pub enum ScrubError {
    /// The database file does not exist. Nothing was created.
    Missing,
    /// Another process holds the database. Nothing was changed.
    Busy,
    /// The rows were rewritten, but the old bytes could not be purged from the
    /// file (the database was busy, or `VACUUM` failed). Running the scrub again
    /// finishes the purge.
    PurgeIncomplete { report: ScrubReport, reason: String },
    /// Any other SQLite error. Nothing was changed.
    Sqlite(String),
}

impl fmt::Display for ScrubError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScrubError::Missing => write!(f, "the metrics database does not exist"),
            ScrubError::Busy => write!(
                f,
                "the metrics database is busy (another ecotokens process is using it); \
                 nothing was changed. Try again when no agent session is running"
            ),
            ScrubError::PurgeIncomplete { report, reason } => write!(
                f,
                "{} row(s) were masked, but the old bytes could not be purged from the \
                 file ({reason}). Run `ecotokens clear --scrub-secrets` again when no \
                 agent session is running",
                report.rows_changed
            ),
            ScrubError::Sqlite(e) => write!(f, "{e}; nothing was changed"),
        }
    }
}

impl std::error::Error for ScrubError {}

fn is_busy(e: &rusqlite::Error) -> bool {
    matches!(
        e.sqlite_error_code(),
        Some(ErrorCode::DatabaseBusy) | Some(ErrorCode::DatabaseLocked)
    )
}

impl From<rusqlite::Error> for ScrubError {
    fn from(e: rusqlite::Error) -> Self {
        if is_busy(&e) {
            ScrubError::Busy
        } else {
            ScrubError::Sqlite(e.to_string())
        }
    }
}

/// Scrub `path` with the store's usual busy timeout.
pub fn scrub_secrets(path: &Path, dry_run: bool) -> Result<ScrubReport, ScrubError> {
    scrub_secrets_with_timeout(path, dry_run, DEFAULT_BUSY_TIMEOUT)
}

/// Scrub `path`, waiting at most `busy_timeout` for another process holding it.
///
/// With `dry_run` the database is opened read-only and the report says what
/// would change. Otherwise the changed columns are rewritten in one
/// transaction and the file is compacted (see the module documentation). A
/// missing database is an error and is never created; a database without the
/// `interceptions` table reports zero rows and is left untouched.
pub fn scrub_secrets_with_timeout(
    path: &Path,
    dry_run: bool,
    busy_timeout: Duration,
) -> Result<ScrubReport, ScrubError> {
    if !path.is_file() {
        return Err(ScrubError::Missing);
    }
    let access = if dry_run {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };
    let mut conn = {
        let _guard = db_init_lock();
        Connection::open_with_flags(path, access | OpenFlags::SQLITE_OPEN_NO_MUTEX)?
    };
    conn.busy_timeout(busy_timeout)?;

    if !has_interceptions(&conn)? {
        return Ok(ScrubReport::default());
    }

    if dry_run {
        let (report, _) = scan(&conn)?;
        return Ok(report);
    }

    // Before any write: replaced cell content and freed pages are zeroed.
    let secure: i64 = conn.query_row("PRAGMA secure_delete = ON", [], |r| r.get(0))?;
    if secure != 1 {
        return Err(ScrubError::Sqlite(
            "PRAGMA secure_delete could not be enabled".into(),
        ));
    }

    // IMMEDIATE takes the write lock up front: the scan and the updates see
    // the same rows, and a busy database fails here, before anything changed.
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (report, changes) = scan(&tx)?;
    for change in &changes {
        for (column, value) in &change.columns {
            tx.execute(
                &format!("UPDATE interceptions SET {column} = ?1 WHERE rowid = ?2"),
                params![value, change.rowid],
            )?;
        }
    }
    tx.commit()?;

    purge(&conn).map_err(|e| ScrubError::PurgeIncomplete {
        report: report.clone(),
        reason: e,
    })?;
    Ok(report)
}

fn has_interceptions(conn: &Connection) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'interceptions'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
}

/// The new values of one row: only the columns whose masked value differs.
struct RowChange {
    rowid: i64,
    columns: Vec<(&'static str, String)>,
}

/// Read every row and mask its free-text columns. Rows are streamed: only the
/// changed values are kept in memory.
fn scan(conn: &Connection) -> rusqlite::Result<(ScrubReport, Vec<RowChange>)> {
    let mut report = ScrubReport::default();
    let mut changes = Vec::new();
    let mut stmt = conn.prepare(&format!(
        "SELECT rowid, {} FROM interceptions",
        SCRUBBED_COLUMNS.join(", ")
    ))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        report.rows_scanned += 1;
        let rowid: i64 = row.get(0)?;
        let mut columns = Vec::new();
        for (i, column) in SCRUBBED_COLUMNS.iter().enumerate() {
            // NULL, numbers and blobs are left alone: only text can carry a
            // typed secret, and rewriting other types would change them.
            let ValueRef::Text(bytes) = row.get_ref(i + 1)? else {
                continue;
            };
            let original = String::from_utf8_lossy(bytes);
            let new = masked(&original);
            if new != original {
                report.columns[i] += 1;
                columns.push((*column, new));
            }
        }
        if !columns.is_empty() {
            report.rows_changed += 1;
            changes.push(RowChange { rowid, columns });
        }
    }
    Ok((report, changes))
}

/// Empty the WAL, rebuild the file from its live content, empty the WAL again.
fn purge(conn: &Connection) -> Result<(), String> {
    checkpoint_truncate(conn)?;
    conn.execute_batch("VACUUM").map_err(|e| {
        if is_busy(&e) {
            "the database is busy".to_string()
        } else {
            format!("VACUUM failed: {e}")
        }
    })?;
    checkpoint_truncate(conn)
}

fn checkpoint_truncate(conn: &Connection) -> Result<(), String> {
    // Returns (busy, wal frames, frames checkpointed); busy = 1 when a reader
    // or writer kept the checkpoint from completing within the busy timeout.
    let busy: i64 = conn
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
        .map_err(|e| {
            if is_busy(&e) {
                "the database is busy".to_string()
            } else {
                format!("WAL checkpoint failed: {e}")
            }
        })?;
    if busy != 0 {
        return Err("the database is busy".to_string());
    }
    Ok(())
}
