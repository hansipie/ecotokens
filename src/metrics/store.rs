use chrono::Utc;
use rusqlite::{params, Connection, Row};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

// Sérialise les ouvertures de connexion pour éviter la race condition sur
// PRAGMA journal_mode = WAL : rusqlite's busy handler est instable (#ignore),
// et deux threads qui initialisent le même fichier DB simultanément peuvent
// obtenir SQLITE_BUSY immédiatement même avec busy_timeout.
static CONN_INIT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

const CONTENT_MAX_CHARS: usize = 4096;

fn truncate_content(s: String) -> String {
    if s.chars().count() <= CONTENT_MAX_CHARS {
        s
    } else {
        let t: String = s.chars().take(CONTENT_MAX_CHARS).collect();
        t + "\n…[truncated]"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HookType {
    #[default]
    PreToolUse,
    PostToolUse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CommandFamily {
    Git,
    Cargo,
    Cpp,
    Fs,
    Markdown,
    Python,
    ConfigFile,
    Go,
    Js,
    Gh,
    Container,
    Grep,
    Aws,
    Network,
    Db,
    Generic,
    NativeRead,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FilterMode {
    Filtered,
    Passthrough,
    Summarized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interception {
    pub id: String,
    pub timestamp: String,
    pub command: String,
    pub command_family: CommandFamily,
    pub git_root: Option<String>,
    pub tokens_before: u32,
    pub tokens_after: u32,
    pub savings_pct: f32,
    pub mode: FilterMode,
    pub redacted: bool,
    pub duration_ms: u32,
    #[serde(default)]
    pub content_before: Option<String>,
    #[serde(default)]
    pub content_after: Option<String>,
    #[serde(default)]
    pub hook_type: HookType,
}

impl Interception {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command: String,
        command_family: CommandFamily,
        git_root: Option<String>,
        tokens_before: u32,
        tokens_after: u32,
        mode: FilterMode,
        redacted: bool,
        duration_ms: u32,
        content_before: Option<String>,
        content_after: Option<String>,
    ) -> Self {
        let savings_pct = if mode == FilterMode::Passthrough || tokens_before == 0 {
            0.0
        } else {
            ((1.0 - tokens_after as f64 / tokens_before as f64) * 100.0) as f32
        };
        Interception {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            command,
            command_family,
            git_root,
            tokens_before,
            tokens_after,
            savings_pct,
            mode,
            redacted,
            duration_ms,
            content_before: content_before.map(truncate_content),
            content_after: content_after.map(truncate_content),
            hook_type: HookType::PreToolUse,
        }
    }

    pub fn with_hook_type(mut self, hook_type: HookType) -> Self {
        self.hook_type = hook_type;
        self
    }
}

// ── Enum ↔ TEXT helpers ───────────────────────────────────────────────────────

fn enum_to_str<T: Serialize>(val: &T) -> String {
    serde_json::to_string(val)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

fn str_to_enum<T: DeserializeOwned>(s: &str) -> io::Result<T> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

// ── Schema ────────────────────────────────────────────────────────────────────

fn create_schema(conn: &Connection) -> io::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS interceptions (
            id              TEXT    NOT NULL PRIMARY KEY,
            timestamp       TEXT    NOT NULL,
            command         TEXT    NOT NULL,
            command_family  TEXT    NOT NULL,
            git_root        TEXT,
            tokens_before   INTEGER NOT NULL,
            tokens_after    INTEGER NOT NULL,
            savings_pct     REAL    NOT NULL,
            mode            TEXT    NOT NULL,
            redacted        INTEGER NOT NULL,
            duration_ms     INTEGER NOT NULL,
            content_before  TEXT,
            content_after   TEXT,
            hook_type       TEXT    NOT NULL DEFAULT 'pre_tool_use'
        );
        CREATE INDEX IF NOT EXISTS idx_timestamp      ON interceptions(timestamp);
        CREATE INDEX IF NOT EXISTS idx_command_family ON interceptions(command_family);
        CREATE INDEX IF NOT EXISTS idx_git_root       ON interceptions(git_root);",
    )
    .map_err(io::Error::other)
}

// ── Connection ────────────────────────────────────────────────────────────────

fn open_conn(path: &Path) -> io::Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _guard = CONN_INIT_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap();
    let conn = Connection::open(path).map_err(io::Error::other)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;",
    )
    .map_err(io::Error::other)?;
    create_schema(&conn)?;
    migrate_from_jsonl_if_needed(path, &conn)?;
    Ok(conn)
}

// ── Migration JSONL → SQLite ──────────────────────────────────────────────────

fn migrate_from_jsonl_if_needed(db_path: &Path, conn: &Connection) -> io::Result<()> {
    let jsonl_path = db_path.with_extension("jsonl");
    let migrating_path = db_path.with_extension("jsonl.migrating");
    let migrated_path = db_path.with_extension("jsonl.migrated");

    // Claim atomique via rename : sur POSIX, rename() est atomique sur le même
    // filesystem. Un seul processus peut réussir ; les autres reçoivent NotFound
    // et retournent immédiatement.
    //
    // Crash recovery : si .migrating existe déjà (processus crashé entre les
    // deux renames), on reprend la migration depuis ce fichier. Les INSERT OR
    // IGNORE rendent l'opération idempotente.
    let source = if migrating_path.exists() {
        migrating_path.clone()
    } else {
        match std::fs::rename(&jsonl_path, &migrating_path) {
            Ok(()) => migrating_path.clone(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        }
    };

    let content = match std::fs::read_to_string(&source) {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    let tx = conn.unchecked_transaction().map_err(io::Error::other)?;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(item) = serde_json::from_str::<Interception>(line) {
            insert_one(&tx, &item)?;
        }
    }

    tx.commit().map_err(io::Error::other)?;

    // Archivage final — ignorer NotFound : deux processus en crash-recovery
    // peuvent tous deux arriver ici ; le second rename échoue sans dommage.
    if let Err(e) = std::fs::rename(&source, &migrated_path) {
        if e.kind() != io::ErrorKind::NotFound {
            return Err(e);
        }
    }

    Ok(())
}

// ── Row mapping ───────────────────────────────────────────────────────────────

fn row_to_interception(row: &Row) -> rusqlite::Result<Interception> {
    let command_family: String = row.get(3)?;
    let mode: String = row.get(8)?;
    let redacted: i32 = row.get(9)?;
    let hook_type: String = row.get(13)?;

    Ok(Interception {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        command: row.get(2)?,
        command_family: str_to_enum(&command_family).map_err(|e| {
            rusqlite::Error::InvalidColumnType(3, e.to_string(), rusqlite::types::Type::Text)
        })?,
        git_root: row.get(4)?,
        tokens_before: row.get::<_, i64>(5)? as u32,
        tokens_after: row.get::<_, i64>(6)? as u32,
        savings_pct: row.get::<_, f64>(7)? as f32,
        mode: str_to_enum(&mode).map_err(|e| {
            rusqlite::Error::InvalidColumnType(8, e.to_string(), rusqlite::types::Type::Text)
        })?,
        redacted: redacted != 0,
        duration_ms: row.get::<_, i64>(10)? as u32,
        content_before: row.get(11)?,
        content_after: row.get(12)?,
        hook_type: str_to_enum(&hook_type).map_err(|e| {
            rusqlite::Error::InvalidColumnType(13, e.to_string(), rusqlite::types::Type::Text)
        })?,
    })
}

// ── Insert ────────────────────────────────────────────────────────────────────

fn insert_one(conn: &Connection, i: &Interception) -> io::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO interceptions
         (id, timestamp, command, command_family, git_root,
          tokens_before, tokens_after, savings_pct, mode, redacted,
          duration_ms, content_before, content_after, hook_type)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        params![
            i.id,
            i.timestamp,
            i.command,
            enum_to_str(&i.command_family),
            i.git_root,
            i.tokens_before as i64,
            i.tokens_after as i64,
            i.savings_pct as f64,
            enum_to_str(&i.mode),
            i.redacted as i32,
            i.duration_ms as i64,
            i.content_before,
            i.content_after,
            enum_to_str(&i.hook_type),
        ],
    )
    .map_err(io::Error::other)?;
    Ok(())
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn metrics_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ecotokens").join("metrics.db"))
}

/// Append one Interception to the SQLite store at `path`.
pub fn append_to(path: &Path, interception: &Interception) -> io::Result<()> {
    let conn = open_conn(path)?;
    insert_one(&conn, interception)
}

/// Read all Interceptions from the SQLite store at `path`.
pub fn read_from(path: &Path) -> io::Result<Vec<Interception>> {
    let legacy_jsonl_path = path.with_extension("jsonl");
    let migrating_path = path.with_extension("jsonl.migrating");
    if !path.exists() && !legacy_jsonl_path.exists() && !migrating_path.exists() {
        return Ok(vec![]);
    }
    let conn = open_conn(path)?;
    let mut stmt = conn
        .prepare("SELECT id, timestamp, command, command_family, git_root, tokens_before, tokens_after, savings_pct, mode, redacted, duration_ms, content_before, content_after, hook_type FROM interceptions ORDER BY timestamp ASC")
        .map_err(io::Error::other)?;
    let items = stmt
        .query_map([], row_to_interception)
        .map_err(io::Error::other)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(items)
}

/// Append to default metrics path (~/.config/ecotokens/metrics.db).
#[allow(dead_code)]
pub fn append(interception: &Interception) -> io::Result<()> {
    let path = metrics_path()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "cannot resolve config dir"))?;
    append_to(&path, interception)
}

/// Read from default metrics path.
#[allow(dead_code)]
pub fn read_all() -> io::Result<Vec<Interception>> {
    match metrics_path() {
        Some(p) => read_from(&p),
        None => Ok(vec![]),
    }
}

/// Atomically replace all interceptions at `path` with `items`.
///
/// Implemented as a single transaction (DELETE all + INSERT batch) which is
/// equivalent to the previous atomic rename approach.
pub fn write_to(path: &Path, items: &[Interception]) -> io::Result<()> {
    let conn = open_conn(path)?;
    let tx = conn.unchecked_transaction().map_err(io::Error::other)?;
    tx.execute("DELETE FROM interceptions", [])
        .map_err(io::Error::other)?;
    for item in items {
        insert_one(&tx, item)?;
    }
    tx.commit().map_err(io::Error::other)
}
