//! Outcomes of delegated jobs: what each route really cost and whether it
//! worked. Cost, tokens and time are measured automatically from the session
//! transcript ([`super::session`]); the judgment (did it work, how much did
//! the user correct, any regression) is entered by the user. The two never
//! overwrite each other, and a job nobody judged is "unknown", never a failure.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::stats;
use crate::config::Settings;

/// How much the user had to correct a job's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Correction {
    None,
    Minor,
    Major,
}

impl Correction {
    pub fn as_str(self) -> &'static str {
        match self {
            Correction::None => "none",
            Correction::Minor => "minor",
            Correction::Major => "major",
        }
    }

    pub fn parse(s: &str) -> Option<Correction> {
        [Correction::None, Correction::Minor, Correction::Major]
            .into_iter()
            .find(|c| c.as_str() == s)
    }
}

/// What the user says about a job. Fields left `None` keep their value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Judgment {
    /// `Some(true)`: verified success on the first attempt.
    pub success: Option<bool>,
    pub correction: Option<Correction>,
    pub regressions: Option<i64>,
}

impl Judgment {
    pub fn is_empty(&self) -> bool {
        self.success.is_none() && self.correction.is_none() && self.regressions.is_none()
    }
}

/// What was measured for a job; every part may be unknown.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Measurement {
    pub cost_usd: Option<f64>,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub duration_ms: Option<i64>,
    pub runs: Option<i64>,
}

#[derive(Debug)]
pub enum JudgmentError {
    /// No delegated job has this id.
    UnknownJob(i64),
    Io(io::Error),
}

impl fmt::Display for JudgmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JudgmentError::UnknownJob(id) => write!(
                f,
                "no delegated job with id {id}; list the jobs that await a judgment with \
                 `ecotokens router outcome --pending`"
            ),
            JudgmentError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for JudgmentError {}

impl From<io::Error> for JudgmentError {
    fn from(e: io::Error) -> Self {
        JudgmentError::Io(e)
    }
}

fn sql(e: rusqlite::Error) -> io::Error {
    io::Error::other(e)
}

/// Records a judgment. Idempotent; a later value overwrites an earlier one,
/// field by field, and the measurements are left alone.
pub fn record_judgment(db: &Path, id: i64, judgment: &Judgment) -> Result<(), JudgmentError> {
    let conn = stats::open(db)?;
    let decision: Option<String> = conn
        .query_row(
            "SELECT decision FROM router_decisions WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    if decision.as_deref() != Some("delegated") {
        return Err(JudgmentError::UnknownJob(id));
    }
    conn.execute(
        "INSERT INTO router_outcomes (decision_id, success, correction, regressions, judged_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(decision_id) DO UPDATE SET
             success     = COALESCE(?2, success),
             correction  = COALESCE(?3, correction),
             regressions = COALESCE(?4, regressions),
             judged_at   = ?5",
        params![
            id,
            judgment.success.map(i64::from),
            judgment.correction.map(Correction::as_str),
            judgment.regressions,
            chrono::Utc::now().to_rfc3339(),
        ],
    )
    .map_err(sql)?;
    Ok(())
}

/// Stores what was measured. Never touches the judgment; a part that is
/// unknown now does not erase one measured before.
// Used by the library crate and tests; SessionEnd attaches through `attach_on`.
#[allow(dead_code)]
pub fn attach_automatic(db: &Path, id: i64, m: &Measurement) -> io::Result<()> {
    let conn = stats::open(db)?;
    attach_on(&conn, id, m)
}

pub(crate) fn attach_on(conn: &Connection, id: i64, m: &Measurement) -> io::Result<()> {
    conn.execute(
        "INSERT INTO router_outcomes (decision_id, cost_usd, tokens_in, tokens_out, duration_ms, runs)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(decision_id) DO UPDATE SET
             cost_usd    = COALESCE(?2, cost_usd),
             tokens_in   = COALESCE(?3, tokens_in),
             tokens_out  = COALESCE(?4, tokens_out),
             duration_ms = COALESCE(?5, duration_ms),
             runs        = COALESCE(?6, runs)",
        params![id, m.cost_usd, m.tokens_in, m.tokens_out, m.duration_ms, m.runs],
    )
    .map_err(sql)?;
    Ok(())
}

/// A delegated job that nobody has judged.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pending {
    pub id: i64,
    pub date: String,
    pub route: String,
    pub category: Option<String>,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<i64>,
}

/// The route of a row; rows from the size-only router name their size.
const ROUTE_SQL: &str = "COALESCE(d.route, 'router-' || d.size, 'unknown')";

/// Delegated jobs without a verdict, oldest first. No message text exists to
/// show: none is ever stored.
pub fn pending(db: &Path) -> io::Result<Vec<Pending>> {
    if !db.exists() {
        return Ok(Vec::new());
    }
    let conn = stats::open(db)?;
    let query = format!(
        "SELECT d.id, d.timestamp, {ROUTE_SQL}, d.nature, o.cost_usd, o.duration_ms
         FROM router_decisions d LEFT JOIN router_outcomes o ON o.decision_id = d.id
         WHERE d.decision = 'delegated' AND o.success IS NULL
         ORDER BY d.id"
    );
    let mut stmt = conn.prepare(&query).map_err(sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Pending {
                id: r.get(0)?,
                date: r.get(1)?,
                route: r.get(2)?,
                category: r.get(3)?,
                cost_usd: r.get(4)?,
                duration_ms: r.get(5)?,
            })
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(rows)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CorrectionCounts {
    pub none: u64,
    pub minor: u64,
    pub major: u64,
}

/// Figures for one route or one category.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct GroupStats {
    pub delegated: u64,
    /// Jobs with a verdict (success or failure).
    pub judged: u64,
    /// `None` until a job is judged: unknown outcomes are never counted.
    pub success_rate: Option<f64>,
    pub avg_tokens: Option<f64>,
    /// Helper cost, `None` unless some job was measured and priced.
    pub avg_cost_usd: Option<f64>,
    /// Helper time: from the delegation to the end of the last helper run.
    pub avg_duration_ms: Option<f64>,
    pub avg_runs: Option<f64>,
    pub correction: CorrectionCounts,
    pub regressions: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct OutcomeCounts {
    pub unknown: u64,
    pub judged: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Aggregates {
    pub by_route: BTreeMap<String, GroupStats>,
    pub by_category: BTreeMap<String, GroupStats>,
    pub outcomes: OutcomeCounts,
}

#[derive(Default)]
struct Acc {
    stats: GroupStats,
    successes: u64,
    tokens: (f64, u64),
    cost: (f64, u64),
    duration: (f64, u64),
    runs: (f64, u64),
}

impl Acc {
    fn finish(mut self) -> GroupStats {
        let avg = |(sum, n): (f64, u64)| (n > 0).then(|| sum / n as f64);
        self.stats.success_rate =
            (self.stats.judged > 0).then(|| self.successes as f64 / self.stats.judged as f64);
        self.stats.avg_tokens = avg(self.tokens);
        self.stats.avg_cost_usd = avg(self.cost);
        self.stats.avg_duration_ms = avg(self.duration);
        self.stats.avg_runs = avg(self.runs);
        self.stats
    }
}

/// Per-route and per-category figures over every delegated job.
pub fn aggregates(db: &Path, _settings: &Settings) -> io::Result<Aggregates> {
    let mut out = Aggregates::default();
    if !db.exists() {
        return Ok(out);
    }
    let conn = stats::open(db)?;
    let query = format!(
        "SELECT {ROUTE_SQL}, COALESCE(d.nature, 'unknown'),
                o.success, o.correction, o.regressions,
                o.tokens_in, o.tokens_out, o.cost_usd, o.duration_ms, o.runs
         FROM router_decisions d LEFT JOIN router_outcomes o ON o.decision_id = d.id
         WHERE d.decision = 'delegated'
         ORDER BY d.id"
    );
    let mut stmt = conn.prepare(&query).map_err(sql)?;
    let mut by_route: BTreeMap<String, Acc> = BTreeMap::new();
    let mut by_category: BTreeMap<String, Acc> = BTreeMap::new();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, Option<f64>>(7)?,
                r.get::<_, Option<i64>>(8)?,
                r.get::<_, Option<i64>>(9)?,
            ))
        })
        .map_err(sql)?;
    for row in rows {
        let (route, category, success, correction, regressions, tin, tout, cost, ms, runs) =
            row.map_err(sql)?;
        match success {
            Some(_) => out.outcomes.judged += 1,
            None => out.outcomes.unknown += 1,
        }
        for acc in [
            by_route.entry(route).or_default(),
            by_category.entry(category).or_default(),
        ] {
            acc.stats.delegated += 1;
            if let Some(s) = success {
                acc.stats.judged += 1;
                acc.successes += (s != 0) as u64;
            }
            match correction.as_deref().and_then(Correction::parse) {
                Some(Correction::None) => acc.stats.correction.none += 1,
                Some(Correction::Minor) => acc.stats.correction.minor += 1,
                Some(Correction::Major) => acc.stats.correction.major += 1,
                Option::None => {}
            }
            acc.stats.regressions += regressions.unwrap_or(0);
            if let Some(tin) = tin {
                acc.tokens.0 += (tin + tout.unwrap_or(0)) as f64;
                acc.tokens.1 += 1;
            }
            if let Some(cost) = cost {
                acc.cost.0 += cost;
                acc.cost.1 += 1;
            }
            if let Some(ms) = ms {
                acc.duration.0 += ms as f64;
                acc.duration.1 += 1;
            }
            if let Some(runs) = runs {
                acc.runs.0 += runs as f64;
                acc.runs.1 += 1;
            }
        }
    }
    out.by_route = by_route.into_iter().map(|(k, v)| (k, v.finish())).collect();
    out.by_category = by_category
        .into_iter()
        .map(|(k, v)| (k, v.finish()))
        .collect();
    Ok(out)
}
