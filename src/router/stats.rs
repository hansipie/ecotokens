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

/// Columns added to `router_decisions` after the first release, in order.
/// Added one by one, so a database of any older release is brought up to date.
const ADDED_COLUMNS: [(&str, &str); 10] = [
    ("session_id", "TEXT"),
    ("nature", "TEXT"),
    ("risk", "TEXT"),
    ("ambiguity", "REAL"),
    ("verifiability", "REAL"),
    ("rule", "TEXT"),
    ("route", "TEXT"),
    ("raised", "INTEGER"),
    ("policy_version", "TEXT"),
    ("models", "TEXT"),
];

/// Policy attributed to rows written before the policy router existed.
pub const LEGACY_POLICY: &str = "legacy-size";

/// Tier → model alias in force, stored with every routed decision.
fn models_json() -> String {
    let map: BTreeMap<&str, &str> = Size::ALL
        .into_iter()
        .map(|s| (s.as_str(), s.model_alias()))
        .collect();
    serde_json::to_string(&map).unwrap_or_default()
}

fn is_busy(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(f, _)
            if matches!(
                f.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
    )
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(router_decisions)")?;
    let existing: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    for (column, ty) in ADDED_COLUMNS {
        if existing.iter().any(|c| c == column) {
            continue;
        }
        match conn.execute(
            &format!("ALTER TABLE router_decisions ADD COLUMN {column} {ty}"),
            [],
        ) {
            Ok(_) => {}
            // Another process migrated between our check and our ALTER.
            Err(e) if e.to_string().contains("duplicate column name") => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn init(conn: &Connection) -> rusqlite::Result<()> {
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
    )?;
    migrate(conn)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS router_outcomes (
             decision_id  INTEGER PRIMARY KEY,
             cost_usd     REAL,
             tokens_in    INTEGER,
             tokens_out   INTEGER,
             duration_ms  INTEGER,
             runs         INTEGER,
             success      INTEGER,
             correction   TEXT,
             regressions  INTEGER,
             judged_at    TEXT,
             reminded     INTEGER NOT NULL DEFAULT 0,
             judged_by    TEXT
         );",
    )?;
    migrate_outcomes(conn)
}

/// Databases created before automatic verdicts lack `judged_by`; a judged row
/// without it was entered by the user.
fn migrate_outcomes(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(router_outcomes)")?;
    let has_column = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|c| c == "judged_by");
    drop(stmt);
    if has_column {
        return Ok(());
    }
    match conn.execute("ALTER TABLE router_outcomes ADD COLUMN judged_by TEXT", []) {
        Ok(_) => Ok(()),
        // Another process migrated between our check and our ALTER.
        Err(e) if e.to_string().contains("duplicate column name") => Ok(()),
        Err(e) => Err(e),
    }
}

/// Opens the database, creating and migrating it. Concurrent first opens by
/// several hook processes can collide on the journal-mode switch or the
/// migration; the setup is idempotent, so a busy result is retried briefly.
pub(crate) fn open(path: &Path) -> io::Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut attempt = 0;
    loop {
        let conn = Connection::open(path).map_err(io::Error::other)?;
        match init(&conn) {
            Ok(()) => return Ok(conn),
            Err(e) if is_busy(&e) && attempt < 40 => {
                attempt += 1;
                drop(conn);
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
            Err(e) => return Err(io::Error::other(e)),
        }
    }
}

/// Whether a decision leaves anything but its timestamp and kind behind.
fn is_gate_decision(decision: Decision) -> bool {
    matches!(decision, Decision::Private | Decision::NotAllowed)
}

/// Writes one decision and returns its row id. Private and not-allowed
/// decisions keep only the timestamp and the kind: no card, no session id,
/// nothing that could say what was asked or where.
pub fn record_returning_id(path: &Path, routing: &Routing) -> io::Result<i64> {
    record_at(path, routing, chrono::Utc::now())
}

/// [`record_returning_id`] with an explicit timestamp (tests, replays).
pub fn record_at(
    path: &Path,
    routing: &Routing,
    at: chrono::DateTime<chrono::Utc>,
) -> io::Result<i64> {
    let conn = open(path)?;
    let now = at.to_rfc3339();
    if is_gate_decision(routing.decision) {
        conn.execute(
            "INSERT INTO router_decisions (timestamp, decision) VALUES (?1, ?2)",
            params![now, routing.decision.as_str()],
        )
        .map_err(io::Error::other)?;
        return Ok(conn.last_insert_rowid());
    }
    let usage = routing.usage.unwrap_or_default();
    let card = routing.card.as_ref();
    conn.execute(
        "INSERT INTO router_decisions
             (timestamp, decision, size, confidence, followup_prob,
              input_tokens, output_tokens, latency_ms,
              session_id, nature, risk, ambiguity, verifiability,
              rule, route, raised, policy_version, models)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            now,
            routing.decision.as_str(),
            routing.size.map(Size::as_str),
            routing.confidence,
            routing.followup_prob,
            usage.input_tokens as i64,
            usage.output_tokens as i64,
            routing.latency_ms as i64,
            routing.session_id,
            card.and_then(|c| c.nature).map(|n| n.value.as_str()),
            card.and_then(|c| c.risk).map(|r| r.value.as_str()),
            card.and_then(|c| c.ambiguity),
            card.and_then(|c| c.verifiability),
            routing.rule,
            routing.route,
            routing.route.as_ref().map(|_| routing.raised as i64),
            routing.policy_version,
            routing.route.as_ref().map(|_| models_json()),
        ],
    )
    .map_err(io::Error::other)?;
    Ok(conn.last_insert_rowid())
}

pub fn record(path: &Path, routing: &Routing) -> io::Result<()> {
    record_returning_id(path, routing).map(|_| ())
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SizeCount {
    /// Messages Jev put in this size.
    pub picked: u64,
    /// Of those, messages actually handed to the helper.
    pub delegated: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PolicyCount {
    pub messages: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Summary {
    pub messages: u64,
    pub by_decision: BTreeMap<String, u64>,
    pub by_size: BTreeMap<String, SizeCount>,
    /// Messages per policy version; rows written before the policy router
    /// count as `legacy-size`. Private and not-allowed rows carry no policy.
    pub by_policy: BTreeMap<String, PolicyCount>,
    /// Requests that reached Jev (every decision except skipped / jev_down
    /// and the two gate decisions).
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
            "SELECT timestamp, decision, size, input_tokens, output_tokens, latency_ms,
                    policy_version
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
                r.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(io::Error::other)?;
    let mut latency_total = 0u64;
    for row in rows {
        let (ts, decision, size, input, output, latency, policy) = row.map_err(io::Error::other)?;
        s.messages += 1;
        if s.first.is_none() {
            s.first = Some(ts);
        }
        *s.by_decision.entry(decision.clone()).or_default() += 1;
        let decision = Decision::parse(&decision);
        if !decision.is_some_and(is_gate_decision) {
            let version = policy.unwrap_or_else(|| LEGACY_POLICY.to_string());
            s.by_policy.entry(version).or_default().messages += 1;
        }
        if !matches!(
            decision,
            Some(Decision::Skipped | Decision::JevDown | Decision::Private | Decision::NotAllowed)
        ) {
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

// ── Reset ───────────────────────────────────────────────────────────────

/// Rows held by, or removed from, the two routing tables.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct RoutingRows {
    pub decisions: u64,
    pub outcomes: u64,
}

impl RoutingRows {
    pub fn is_empty(&self) -> bool {
        self.decisions == 0 && self.outcomes == 0
    }
}

fn count_rows(conn: &Connection) -> rusqlite::Result<RoutingRows> {
    let count = |table: &str| {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
            r.get::<_, i64>(0)
        })
        .map(|n| n.max(0) as u64)
    };
    Ok(RoutingRows {
        decisions: count("router_decisions")?,
        outcomes: count("router_outcomes")?,
    })
}

/// Rows in `router_decisions` and `router_outcomes`. A missing database reads
/// as zero and is not created.
pub fn routing_rows(path: &Path) -> io::Result<RoutingRows> {
    if !path.exists() {
        return Ok(RoutingRows::default());
    }
    let conn = open(path)?;
    count_rows(&conn).map_err(io::Error::other)
}

/// `<db>.bak-<UTC timestamp>` next to the database.
pub fn backup_path(path: &Path, at: chrono::DateTime<chrono::Utc>) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_else(|| "router.db".into());
    name.push(format!(".bak-{}", at.format("%Y%m%d-%H%M%S")));
    path.with_file_name(name)
}

/// Writes a consistent copy of the database to `dest` with `VACUUM INTO`,
/// which reads one snapshot of the database and its WAL, so a copy taken
/// while a hook is writing is never torn. Refuses to overwrite `dest`.
pub fn backup(path: &Path, dest: &Path) -> io::Result<()> {
    if dest.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("backup file already exists: {}", dest.display()),
        ));
    }
    let conn = open(path)?;
    conn.execute("VACUUM INTO ?1", params![dest.to_string_lossy()])
        .map_err(io::Error::other)?;
    Ok(())
}

/// Deletes every row of `router_outcomes`, then of `router_decisions`, in one
/// transaction, and returns how many rows each lost. `jev_calls` and every
/// other table are left untouched; a missing database is not created.
///
/// The `AUTOINCREMENT` sequence of `router_decisions` is kept, so ids stay
/// monotonic: a job id seen before the reset (in `router outcome --pending`,
/// a dossier or a session still running) never names a new decision. Deleted
/// content is overwritten (`secure_delete`) and the WAL is checkpointed, so
/// the old rows do not linger in the file.
pub fn reset(path: &Path) -> io::Result<RoutingRows> {
    if !path.exists() {
        return Ok(RoutingRows::default());
    }
    let mut conn = open(path)?;
    conn.execute_batch("PRAGMA secure_delete = ON;")
        .map_err(io::Error::other)?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(io::Error::other)?;
    let outcomes = tx
        .execute("DELETE FROM router_outcomes", [])
        .map_err(io::Error::other)? as u64;
    let decisions = tx
        .execute("DELETE FROM router_decisions", [])
        .map_err(io::Error::other)? as u64;
    tx.commit().map_err(io::Error::other)?;
    // Best effort: a concurrent reader may keep part of the WAL alive.
    let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    Ok(RoutingRows {
        decisions,
        outcomes,
    })
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
