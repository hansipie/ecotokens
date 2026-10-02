//! Router decisions log: one row per routed message, never the message
//! itself. Kept in its own SQLite file so the interceptions store and its
//! migrations stay untouched.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::Serialize;

use super::{Decision, Routing, Size};
use crate::config::Settings;

pub fn router_db_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ecotokens").join("router.db"))
}

fn open(path: &Path) -> io::Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path).map_err(io::Error::other)?;
    conn.execute_batch(
        "PRAGMA busy_timeout = 2000;
         PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS router_decisions (
             id             INTEGER PRIMARY KEY AUTOINCREMENT,
             timestamp      TEXT NOT NULL,
             decision       TEXT NOT NULL,
             size           TEXT,
             confidence     REAL,
             followup_prob  REAL,
             input_tokens   INTEGER NOT NULL DEFAULT 0,
             output_tokens  INTEGER NOT NULL DEFAULT 0,
             latency_ms     INTEGER NOT NULL DEFAULT 0
         );",
    )
    .map_err(io::Error::other)?;
    Ok(conn)
}

pub fn record(path: &Path, routing: &Routing) -> io::Result<()> {
    let conn = open(path)?;
    let usage = routing.usage.unwrap_or_default();
    conn.execute(
        "INSERT INTO router_decisions
             (timestamp, decision, size, confidence, followup_prob,
              input_tokens, output_tokens, latency_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            chrono::Utc::now().to_rfc3339(),
            routing.decision.as_str(),
            routing.size.map(Size::as_str),
            routing.confidence,
            routing.followup_prob,
            usage.input_tokens as i64,
            usage.output_tokens as i64,
            routing.latency_ms as i64,
        ],
    )
    .map_err(io::Error::other)?;
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SizeCount {
    /// Messages Jev put in this size.
    pub picked: u64,
    /// Of those, messages actually handed to the helper.
    pub delegated: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Summary {
    pub messages: u64,
    pub by_decision: BTreeMap<String, u64>,
    pub by_size: BTreeMap<String, SizeCount>,
    /// Requests that reached Jev (every decision except skipped / jev_down).
    pub jev_requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// `None` until a price is configured.
    pub cost_usd: Option<f64>,
    pub avg_latency_ms: u64,
    pub first: Option<String>,
}

pub fn summarize(path: &Path, settings: &Settings) -> io::Result<Summary> {
    let mut s = Summary::default();
    for size in Size::ALL {
        s.by_size.insert(size.as_str().into(), SizeCount::default());
    }
    if !path.exists() {
        s.cost_usd = cost_usd(0, 0, settings);
        return Ok(s);
    }
    let conn = open(path)?;
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, decision, size, input_tokens, output_tokens, latency_ms
             FROM router_decisions ORDER BY id",
        )
        .map_err(io::Error::other)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })
        .map_err(io::Error::other)?;
    let mut latency_total = 0u64;
    for row in rows {
        let (ts, decision, size, input, output, latency) = row.map_err(io::Error::other)?;
        s.messages += 1;
        if s.first.is_none() {
            s.first = Some(ts);
        }
        *s.by_decision.entry(decision.clone()).or_default() += 1;
        let decision = Decision::parse(&decision);
        if !matches!(decision, Some(Decision::Skipped | Decision::JevDown)) {
            s.jev_requests += 1;
            latency_total += latency.max(0) as u64;
        }
        if let Some(count) = size.and_then(|sz| s.by_size.get_mut(&sz)) {
            count.picked += 1;
            if decision == Some(Decision::Delegated) {
                count.delegated += 1;
            }
        }
        s.input_tokens += input.max(0) as u64;
        s.output_tokens += output.max(0) as u64;
    }
    s.avg_latency_ms = latency_total.checked_div(s.jev_requests).unwrap_or(0);
    s.cost_usd = cost_usd(s.input_tokens, s.output_tokens, settings);
    Ok(s)
}

/// Dollar estimate, only when at least one price is configured.
pub fn cost_usd(input_tokens: u64, output_tokens: u64, settings: &Settings) -> Option<f64> {
    if settings.jev_usd_per_mtok_input.is_none() && settings.jev_usd_per_mtok_output.is_none() {
        return None;
    }
    let input = settings.jev_usd_per_mtok_input.unwrap_or(0.0);
    let output = settings.jev_usd_per_mtok_output.unwrap_or(0.0);
    Some(input_tokens as f64 / 1e6 * input + output_tokens as f64 / 1e6 * output)
}
