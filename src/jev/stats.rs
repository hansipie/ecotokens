//! Per-call Jev log: one row per request to the service, never the request
//! content. Lives in the router's SQLite file (`router.db`), in its own table,
//! so the interceptions store and its migrations stay untouched.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rusqlite::{params, Connection};
use serde::Serialize;

use super::{JevError, Usage};
use crate::config::Settings;

/// Overrides the database location (tests, custom setups).
pub const DB_ENV: &str = "ECOTOKENS_JEV_DB";

/// Why a Jev request was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Generic filter: which lines of a large output to keep.
    FilterLines,
    /// Rewrite: prose / code / language classification.
    Classify,
    /// Rewrite: refuse to rewrite code.
    CodeGate,
    /// Rewrite: verify a model response against the original.
    Verify,
    /// Model router: size and follow-up judgment of a prompt.
    Router,
    Other,
}

impl Purpose {
    pub const ALL: [Purpose; 6] = [
        Purpose::FilterLines,
        Purpose::Classify,
        Purpose::CodeGate,
        Purpose::Verify,
        Purpose::Router,
        Purpose::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Purpose::FilterLines => "filter_lines",
            Purpose::Classify => "classify",
            Purpose::CodeGate => "code_gate",
            Purpose::Verify => "verify",
            Purpose::Router => "router",
            Purpose::Other => "other",
        }
    }
}

/// One finished request, as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct CallRecord {
    pub purpose: Purpose,
    pub ok: bool,
    /// `timeout`, `transport`, `http`, `bad_response` or `unavailable`.
    pub error_kind: Option<&'static str>,
    pub http_status: Option<u16>,
    pub latency_ms: u64,
    pub usage: Option<Usage>,
    /// Router calls: helper agent the message was delegated to, or `self (…)`
    /// when the main session kept it.
    pub agent: Option<String>,
}

impl CallRecord {
    pub fn from_result<T>(
        purpose: Purpose,
        result: &Result<T, JevError>,
        latency_ms: u64,
        usage: Option<Usage>,
    ) -> Self {
        let (error_kind, http_status) = match result {
            Ok(_) => (None, None),
            Err(JevError::Unavailable) => (Some("unavailable"), None),
            Err(JevError::Timeout) => (Some("timeout"), None),
            Err(JevError::Transport(_)) => (Some("transport"), None),
            Err(JevError::Http(code)) => (Some("http"), Some(*code)),
            Err(JevError::BadResponse(_)) => (Some("bad_response"), None),
        };
        CallRecord {
            purpose,
            ok: result.is_ok(),
            error_kind,
            http_status,
            latency_ms,
            usage,
            agent: None,
        }
    }
}

/// Project the calls of this process belong to (a git root, like the
/// interceptions' `git_root`). Set once by whoever knows the session's working
/// directory; otherwise the process's own directory is used.
static PROJECT: OnceLock<Option<String>> = OnceLock::new();

pub fn set_project_dir(dir: &Path) {
    let _ = PROJECT.set(crate::filter::project_root_for_cwd(dir));
}

fn current_project() -> Option<String> {
    match PROJECT.get() {
        Some(p) => p.clone(),
        None => std::env::current_dir()
            .ok()
            .and_then(|d| crate::filter::project_root_for_cwd(&d)),
    }
}

/// The project a `--project` path stands for: its git root, or the path itself.
pub fn project_key(dir: &Path) -> String {
    crate::filter::project_root_for_cwd(dir).unwrap_or_else(|| dir.to_string_lossy().to_string())
}

pub fn db_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(DB_ENV).filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    crate::router::stats::router_db_path()
}

fn open(path: &Path) -> io::Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path).map_err(io::Error::other)?;
    conn.execute_batch(
        "PRAGMA busy_timeout = 2000;
         PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS jev_calls (
             id             INTEGER PRIMARY KEY AUTOINCREMENT,
             timestamp      TEXT NOT NULL,
             purpose        TEXT NOT NULL,
             ok             INTEGER NOT NULL,
             error_kind     TEXT,
             http_status    INTEGER,
             latency_ms     INTEGER NOT NULL DEFAULT 0,
             input_tokens   INTEGER NOT NULL DEFAULT 0,
             output_tokens  INTEGER NOT NULL DEFAULT 0
         );",
    )
    .map_err(io::Error::other)?;
    // Databases created before the `agent` / `project` columns existed: add them once.
    for column in ["agent", "project"] {
        let exists = conn
            .prepare("SELECT 1 FROM pragma_table_info('jev_calls') WHERE name = ?1")
            .and_then(|mut q| q.exists([column]))
            .map_err(io::Error::other)?;
        if !exists {
            conn.execute(
                &format!("ALTER TABLE jev_calls ADD COLUMN {column} TEXT"),
                [],
            )
            .map_err(io::Error::other)?;
        }
    }
    Ok(conn)
}

pub fn record(path: &Path, call: &CallRecord) -> io::Result<()> {
    record_at(path, call, chrono::Utc::now())
}

pub fn record_at(
    path: &Path,
    call: &CallRecord,
    at: chrono::DateTime<chrono::Utc>,
) -> io::Result<()> {
    record_in(path, call, at, current_project().as_deref())
}

/// Stores `call` as made in `project` (`None`: no project known).
pub fn record_in(
    path: &Path,
    call: &CallRecord,
    at: chrono::DateTime<chrono::Utc>,
    project: Option<&str>,
) -> io::Result<()> {
    let conn = open(path)?;
    let usage = call.usage.unwrap_or_default();
    conn.execute(
        "INSERT INTO jev_calls
             (timestamp, purpose, ok, error_kind, http_status,
              latency_ms, input_tokens, output_tokens, agent, project)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            at.to_rfc3339(),
            call.purpose.as_str(),
            call.ok,
            call.error_kind,
            call.http_status,
            call.latency_ms as i64,
            usage.input_tokens as i64,
            usage.output_tokens as i64,
            call.agent,
            project,
        ],
    )
    .map_err(io::Error::other)?;
    Ok(())
}

/// Best-effort: a stats failure must never affect the caller.
pub fn record_default(call: &CallRecord) {
    if let Some(path) = db_path() {
        let _ = record(&path, call);
    }
}

/// A row as read back, for the recent-calls log.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CallRow {
    pub timestamp: String,
    pub purpose: String,
    pub ok: bool,
    pub error_kind: Option<String>,
    pub http_status: Option<u16>,
    pub latency_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Router calls: helper agent selected (e.g. `router-everyday`), or
    /// `self (unsure)` / `self (followup)` when not delegated.
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PurposeStats {
    pub calls: u64,
    pub ok: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub avg_latency_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct JevSummary {
    pub calls: u64,
    pub ok: u64,
    /// Failed calls, i.e. the caller used its heuristic instead.
    pub fallbacks: u64,
    pub by_purpose: BTreeMap<String, PurposeStats>,
    /// Failed calls by kind (`http` is split by status: `http 503`).
    pub errors: BTreeMap<String, u64>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: Option<f64>,
    pub avg_latency_ms: u64,
    pub p95_latency_ms: u64,
    /// Calls per bucket, oldest first, for the sparkline.
    pub timeline: Vec<u64>,
    /// Most recent first.
    pub recent: Vec<CallRow>,
    pub first: Option<String>,
}

pub const TIMELINE_BUCKETS: usize = 40;
pub const RECENT_LIMIT: usize = 200;

/// Summary of the calls made at or after `since` (`None` = all of them),
/// in `project` only when one is given (calls recorded before projects were
/// stored have none and are then left out).
pub fn summarize(
    path: &Path,
    settings: &Settings,
    since: Option<chrono::DateTime<chrono::Utc>>,
    project: Option<&str>,
) -> io::Result<JevSummary> {
    let mut s = JevSummary::default();
    for p in Purpose::ALL {
        s.by_purpose
            .insert(p.as_str().into(), PurposeStats::default());
    }
    s.timeline = vec![0; TIMELINE_BUCKETS];
    if !path.exists() {
        s.cost_usd = crate::router::stats::cost_usd(0, 0, settings);
        return Ok(s);
    }
    let conn = open(path)?;
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, purpose, ok, error_kind, http_status,
                    latency_ms, input_tokens, output_tokens, agent
             FROM jev_calls WHERE ?1 IS NULL OR project = ?1 ORDER BY id",
        )
        .map_err(io::Error::other)?;
    let rows = stmt
        .query_map([project], |r| {
            Ok(CallRow {
                timestamp: r.get(0)?,
                purpose: r.get(1)?,
                ok: r.get(2)?,
                error_kind: r.get(3)?,
                http_status: r.get(4)?,
                latency_ms: r.get::<_, i64>(5)?.max(0) as u64,
                input_tokens: r.get::<_, i64>(6)?.max(0) as u64,
                output_tokens: r.get::<_, i64>(7)?.max(0) as u64,
                agent: r.get(8)?,
            })
        })
        .map_err(io::Error::other)?;

    let mut calls = Vec::new();
    for row in rows {
        let row = row.map_err(io::Error::other)?;
        let keep = match since {
            None => true,
            Some(t) => chrono::DateTime::parse_from_rfc3339(&row.timestamp)
                .map(|d| d >= t)
                .unwrap_or(true),
        };
        if keep {
            calls.push(row);
        }
    }

    let mut latencies = Vec::with_capacity(calls.len());
    let mut latency_by_purpose: BTreeMap<String, u64> = BTreeMap::new();
    for c in &calls {
        s.calls += 1;
        if s.first.is_none() {
            s.first = Some(c.timestamp.clone());
        }
        let p = s.by_purpose.entry(c.purpose.clone()).or_default();
        p.calls += 1;
        p.input_tokens += c.input_tokens;
        p.output_tokens += c.output_tokens;
        *latency_by_purpose.entry(c.purpose.clone()).or_default() += c.latency_ms;
        if c.ok {
            s.ok += 1;
            p.ok += 1;
        } else {
            s.fallbacks += 1;
            let kind = c.error_kind.as_deref().unwrap_or("error");
            let key = match c.http_status {
                Some(code) => format!("{kind} {code}"),
                None => kind.to_string(),
            };
            *s.errors.entry(key).or_default() += 1;
        }
        s.input_tokens += c.input_tokens;
        s.output_tokens += c.output_tokens;
        latencies.push(c.latency_ms);
    }
    for (name, total) in latency_by_purpose {
        if let Some(p) = s.by_purpose.get_mut(&name) {
            p.avg_latency_ms = total / p.calls.max(1);
        }
    }
    if !latencies.is_empty() {
        s.avg_latency_ms = latencies.iter().sum::<u64>() / latencies.len() as u64;
        latencies.sort_unstable();
        let idx = ((latencies.len() * 95).div_ceil(100)).saturating_sub(1);
        s.p95_latency_ms = latencies[idx.min(latencies.len() - 1)];
    }
    s.timeline = timeline(&calls);
    s.cost_usd = crate::router::stats::cost_usd(s.input_tokens, s.output_tokens, settings);
    calls.reverse();
    calls.truncate(RECENT_LIMIT);
    s.recent = calls;
    Ok(s)
}

/// Spread the calls' timestamps over [`TIMELINE_BUCKETS`] equal buckets
/// between the first and last call.
fn timeline(calls: &[CallRow]) -> Vec<u64> {
    let mut out = vec![0u64; TIMELINE_BUCKETS];
    let times: Vec<i64> = calls
        .iter()
        .filter_map(|c| chrono::DateTime::parse_from_rfc3339(&c.timestamp).ok())
        .map(|d| d.timestamp())
        .collect();
    let (Some(&min), Some(&max)) = (times.iter().min(), times.iter().max()) else {
        return out;
    };
    let span = (max - min).max(1);
    for t in times {
        let idx = ((t - min) as u128 * TIMELINE_BUCKETS as u128 / (span as u128 + 1)) as usize;
        out[idx.min(TIMELINE_BUCKETS - 1)] += 1;
    }
    out
}
