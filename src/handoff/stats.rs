//! Injection statistics (FR-027): one row per injection or list, never the
//! content. Kept in its own small SQLite file, like the router's decisions.

use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::Serialize;

pub fn handoff_db_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ecotokens").join("handoff.db"))
}

fn open(path: &Path) -> io::Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path).map_err(io::Error::other)?;
    conn.execute_batch(
        "PRAGMA busy_timeout = 2000;
         PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS handoff_injections (
             id          INTEGER PRIMARY KEY AUTOINCREMENT,
             timestamp   TEXT NOT NULL,
             handoff_id  TEXT,
             chars       INTEGER NOT NULL,
             stale       INTEGER NOT NULL,
             source      TEXT NOT NULL,
             listed      INTEGER NOT NULL
         );",
    )
    .map_err(io::Error::other)?;
    Ok(conn)
}

/// One injection of a handoff, or of the short list when several exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionRecord {
    pub timestamp: DateTime<Utc>,
    /// `None` for a list.
    pub handoff_id: Option<String>,
    pub chars: usize,
    pub stale: bool,
    pub source: String,
    pub listed: bool,
}

pub fn record(path: &Path, rec: &InjectionRecord) -> io::Result<()> {
    let conn = open(path)?;
    conn.execute(
        "INSERT INTO handoff_injections (timestamp, handoff_id, chars, stale, source, listed)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            rec.timestamp.to_rfc3339(),
            rec.handoff_id,
            rec.chars as i64,
            rec.stale as i64,
            rec.source,
            rec.listed as i64,
        ],
    )
    .map_err(io::Error::other)?;
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub count: u64,
    pub chars_total: u64,
    pub chars_avg: u64,
    pub stale: u64,
    pub listed: u64,
}

pub fn summarize(path: &Path) -> io::Result<Summary> {
    if !path.exists() {
        return Ok(Summary::default());
    }
    let conn = open(path)?;
    let (count, chars, stale, listed): (i64, i64, i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(chars), 0), COALESCE(SUM(stale), 0), COALESCE(SUM(listed), 0)
             FROM handoff_injections",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(io::Error::other)?;
    let (count, chars) = (count.max(0) as u64, chars.max(0) as u64);
    Ok(Summary {
        count,
        chars_total: chars,
        chars_avg: chars.checked_div(count).unwrap_or(0),
        stale: stale.max(0) as u64,
        listed: listed.max(0) as u64,
    })
}
