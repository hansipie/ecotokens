//! `ecotokens clear --scrub-secrets`: rows written in clear by older versions
//! are masked in place and the old bytes disappear from `metrics.db` and its
//! `-wal` file (TEST-PLAN section G).
//!
//! Legacy rows are written with plain SQL, bypassing `insert_one` (which masks
//! since the fix), exactly as an older binary left them. The fake credentials
//! are assembled at runtime so no secret-shaped literal sits in the source.

use ecotokens::metrics::scrub::{scrub_secrets, scrub_secrets_with_timeout, ScrubError};
use ecotokens::metrics::store::{append_to, CommandFamily, FilterMode, Interception};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

// ── fixtures ─────────────────────────────────────────────────────────────

fn aws_key() -> String {
    // AKIA + 16 uppercase alphanumerics.
    format!("{}{}", ["AK", "IA"].concat(), "Z7QX".repeat(4))
}

fn github_token() -> String {
    // ghp_ + 36 alphanumerics.
    format!("{}_{}", ["gh", "p"].concat(), "a1B2c3".repeat(6))
}

const AWS_MARK: &str = "[AWS_KEY]";
const GH_MARK: &str = "[GITHUB_TOKEN]";

fn benign(command: &str) -> Interception {
    Interception::new(
        command.to_string(),
        CommandFamily::Git,
        Some("/home/u/project".to_string()),
        100,
        40,
        FilterMode::Filtered,
        false,
        3,
        Some("plain before".to_string()),
        Some("plain after".to_string()),
    )
}

/// Create the store's schema (WAL mode, through the normal write path) with one
/// benign row, as a real installation would have.
fn init_store(db: &Path) {
    append_to(db, &benign("git status")).unwrap();
}

/// Insert a row the way an older version did: no masking at all.
fn insert_legacy(
    conn: &Connection,
    id: &str,
    command: &str,
    git_root: Option<&str>,
    before: Option<&str>,
    after: Option<&str>,
) {
    conn.execute(
        "INSERT INTO interceptions
         (id, timestamp, command, command_family, git_root, tokens_before, tokens_after,
          savings_pct, mode, redacted, duration_ms, content_before, content_after, hook_type)
         VALUES (?1, '2026-01-01T00:00:00+00:00', ?2, 'generic', ?3, 10, 5, 50.0,
                 'filtered', 0, 1, ?4, ?5, 'pre_tool_use')",
        params![id, command, git_root, before, after],
    )
    .unwrap();
}

struct Legacy {
    aws: String,
    ghp: String,
    cmd_aws: String,
    cmd_ghp: String,
    root_aws: String,
    after_ghp: String,
}

/// One store with: a benign row, a secret in `command`, a secret in
/// `command` and `git_root` (NULL contents), a secret in `content_after`, and
/// a legacy row without any secret. Written with a fresh connection that is
/// closed, so the rows end up in `metrics.db` itself.
fn seed_legacy(db: &Path) -> Legacy {
    init_store(db);
    let aws = aws_key();
    let ghp = github_token();
    let l = Legacy {
        cmd_aws: format!("echo {aws}"),
        cmd_ghp: format!("echo {ghp} | gh auth login --with-token"),
        root_aws: format!("/tmp/{aws}/repo"),
        after_ghp: format!("line one\nremote: {ghp}\nline three"),
        aws,
        ghp,
    };
    let conn = Connection::open(db).unwrap();
    insert_legacy(&conn, "aws-cmd", &l.cmd_aws, None, None, None);
    insert_legacy(
        &conn,
        "ghp-cmd-root",
        &l.cmd_ghp,
        Some(&l.root_aws),
        None,
        None,
    );
    insert_legacy(
        &conn,
        "ghp-after",
        "cat notes.txt",
        Some("/home/u/project"),
        Some("unchanged before"),
        Some(&l.after_ghp),
    );
    insert_legacy(
        &conn,
        "legacy-benign",
        "ls -la",
        Some("/home/u/project"),
        Some("total 0"),
        Some("total 0"),
    );
    l
}

type Row = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn rows(db: &Path) -> Vec<Row> {
    let conn = Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, command, git_root, content_before, content_after
             FROM interceptions ORDER BY id",
        )
        .unwrap();
    let out = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    out
}

fn row(db: &Path, id: &str) -> Row {
    rows(db).into_iter().find(|r| r.0 == id).unwrap()
}

fn wal(db: &Path) -> PathBuf {
    let mut p = db.as_os_str().to_owned();
    p.push("-wal");
    PathBuf::from(p)
}

fn read_or_empty(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap_or_default()
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    let n = needle.as_bytes();
    haystack.windows(n.len()).any(|w| w == n)
}

/// True when `secret`, or a 12-byte tail of it (a partly overwritten copy
/// still leaks), appears in the raw bytes of the database or its WAL.
fn on_disk(db: &Path, secret: &str) -> bool {
    let tail = &secret[secret.len() - 12..];
    [read_or_empty(db), read_or_empty(&wal(db))]
        .iter()
        .any(|bytes| contains(bytes, secret) || contains(bytes, tail))
}

fn temp_db() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("metrics.db");
    (dir, db)
}

// ── library ──────────────────────────────────────────────────────────────

#[test]
fn dry_run_reports_the_rows_and_columns_and_writes_nothing() {
    let (_d, db) = temp_db();
    let l = seed_legacy(&db);
    let db_before = std::fs::read(&db).unwrap();
    let wal_existed = wal(&db).exists();
    let wal_before = read_or_empty(&wal(&db));
    let mtime = std::fs::metadata(&db).unwrap().modified().unwrap();

    let report = scrub_secrets(&db, true).unwrap();
    assert_eq!(report.rows_scanned, 5);
    assert_eq!(report.rows_changed, 3);
    assert_eq!(
        report.columns_touched(),
        vec![("command", 2), ("git_root", 1), ("content_after", 1)]
    );

    assert_eq!(
        std::fs::read(&db).unwrap(),
        db_before,
        "metrics.db unchanged"
    );
    assert_eq!(std::fs::metadata(&db).unwrap().modified().unwrap(), mtime);
    if wal_existed {
        assert_eq!(read_or_empty(&wal(&db)), wal_before, "-wal unchanged");
    } else {
        assert!(
            read_or_empty(&wal(&db)).is_empty(),
            "no WAL content written"
        );
    }
    assert_eq!(row(&db, "aws-cmd").1.unwrap(), l.cmd_aws);
    assert!(on_disk(&db, &l.aws));
}

#[test]
fn scrub_masks_only_the_secret_and_leaves_other_rows_identical() {
    let (_d, db) = temp_db();
    let l = seed_legacy(&db);
    let before = rows(&db);
    assert!(
        on_disk(&db, &l.aws) && on_disk(&db, &l.ghp),
        "seeded in clear"
    );

    let report = scrub_secrets(&db, false).unwrap();
    assert_eq!(report.rows_scanned, 5);
    assert_eq!(report.rows_changed, 3);
    assert_eq!(report.columns, [2, 1, 0, 1]);

    let r = row(&db, "aws-cmd");
    assert_eq!(r.1.unwrap(), l.cmd_aws.replace(&l.aws, AWS_MARK));
    assert_eq!((r.2, r.3, r.4), (None, None, None), "NULLs stay NULL");

    let r = row(&db, "ghp-cmd-root");
    assert_eq!(r.1.unwrap(), l.cmd_ghp.replace(&l.ghp, GH_MARK));
    assert_eq!(r.2.unwrap(), l.root_aws.replace(&l.aws, AWS_MARK));

    let r = row(&db, "ghp-after");
    assert_eq!(r.1.as_deref(), Some("cat notes.txt"));
    assert_eq!(r.3.as_deref(), Some("unchanged before"));
    assert_eq!(r.4.unwrap(), l.after_ghp.replace(&l.ghp, GH_MARK));

    // Rows without a secret are untouched.
    let after = rows(&db);
    let benign_ids = ["legacy-benign"];
    let is_benign =
        |r: &&Row| benign_ids.contains(&r.0.as_str()) || r.1.as_deref() == Some("git status");
    let b: Vec<_> = before.iter().filter(is_benign).collect();
    let a: Vec<_> = after.iter().filter(is_benign).collect();
    assert_eq!(b.len(), 2);
    assert_eq!(a, b);
    assert_eq!(after.len(), before.len(), "no row added or removed");

    // The old bytes are gone from the file and the WAL.
    assert!(!on_disk(&db, &l.aws), "AWS key still in metrics.db or -wal");
    assert!(
        !on_disk(&db, &l.ghp),
        "GitHub token still in metrics.db or -wal"
    );
}

#[test]
fn a_second_scrub_changes_nothing() {
    let (_d, db) = temp_db();
    seed_legacy(&db);
    assert_eq!(scrub_secrets(&db, false).unwrap().rows_changed, 3);
    let snapshot = rows(&db);
    let again = scrub_secrets(&db, false).unwrap();
    assert_eq!(again.rows_scanned, 5);
    assert_eq!(again.rows_changed, 0);
    assert!(again.columns_touched().is_empty());
    assert_eq!(rows(&db), snapshot);
    assert_eq!(scrub_secrets(&db, true).unwrap().rows_changed, 0);
}

#[test]
fn a_missing_database_is_reported_and_not_created() {
    let (_d, db) = temp_db();
    assert!(matches!(scrub_secrets(&db, true), Err(ScrubError::Missing)));
    assert!(matches!(
        scrub_secrets(&db, false),
        Err(ScrubError::Missing)
    ));
    assert!(!db.exists());
    assert!(!wal(&db).exists());
}

#[test]
fn an_empty_store_or_an_empty_file_reports_zero() {
    let (_d, db) = temp_db();
    init_store(&db);
    Connection::open(&db)
        .unwrap()
        .execute("DELETE FROM interceptions", [])
        .unwrap();
    let r = scrub_secrets(&db, false).unwrap();
    assert_eq!((r.rows_scanned, r.rows_changed), (0, 0));

    let (_d2, empty) = temp_db();
    std::fs::write(&empty, b"").unwrap();
    let r = scrub_secrets(&empty, false).unwrap();
    assert_eq!((r.rows_scanned, r.rows_changed), (0, 0));
    assert_eq!(std::fs::read(&empty).unwrap(), b"", "left untouched");
}

#[test]
fn a_secret_only_in_the_wal_is_purged_from_it() {
    let (_d, db) = temp_db();
    init_store(&db);
    // An idle connection held open keeps SQLite from checkpointing on close:
    // the legacy row stays in the WAL only, as with a hook still running.
    let holder = Connection::open(&db).unwrap();
    // A first read makes the connection actually attach to the WAL.
    let _: i64 = holder
        .query_row("SELECT COUNT(*) FROM interceptions", [], |r| r.get(0))
        .unwrap();
    let aws = aws_key();
    {
        let writer = Connection::open(&db).unwrap();
        writer
            .execute_batch("PRAGMA wal_autocheckpoint = 0;")
            .unwrap();
        insert_legacy(
            &writer,
            "wal-only",
            &format!("echo {aws}"),
            None,
            None,
            None,
        );
    }
    assert!(contains(&read_or_empty(&wal(&db)), &aws), "seeded in -wal");
    assert!(
        !contains(&std::fs::read(&db).unwrap(), &aws),
        "not in db yet"
    );

    let report = scrub_secrets(&db, false).unwrap();
    assert_eq!(report.rows_changed, 1);
    assert_eq!(row(&db, "wal-only").1.unwrap(), format!("echo {AWS_MARK}"));
    assert!(wal(&db).exists(), "the holder keeps the WAL file open");
    assert!(!on_disk(&db, &aws), "secret still in metrics.db or -wal");
    drop(holder);
}

#[test]
fn bytes_left_by_an_earlier_delete_are_purged_too() {
    let (_d, db) = temp_db();
    init_store(&db);
    let aws = aws_key();
    {
        let conn = Connection::open(&db).unwrap();
        insert_legacy(&conn, "doomed", &format!("echo {aws}"), None, None, None);
    }
    // What `ecotokens clear` does: a plain DELETE, no secure_delete.
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch("PRAGMA secure_delete = OFF;").unwrap();
        conn.execute("DELETE FROM interceptions WHERE id = 'doomed'", [])
            .unwrap();
    }
    assert!(on_disk(&db, &aws), "a DELETE leaves the bytes behind");

    let report = scrub_secrets(&db, false).unwrap();
    assert_eq!(report.rows_changed, 0, "no live row holds the secret");
    assert!(!on_disk(&db, &aws), "VACUUM drops the free pages");
}

#[test]
fn huge_content_is_masked_and_its_overflow_pages_purged() {
    let (_d, db) = temp_db();
    init_store(&db);
    let ghp = github_token();
    let filler = "x".repeat(1024 * 1024);
    let content = format!("{filler}\n{ghp}\n{filler}");
    {
        let conn = Connection::open(&db).unwrap();
        insert_legacy(&conn, "huge", "cat big.log", None, None, Some(&content));
    }
    assert!(on_disk(&db, &ghp));

    let report = scrub_secrets(&db, false).unwrap();
    assert_eq!(report.columns, [0, 0, 0, 1]);
    assert_eq!(
        row(&db, "huge").4.unwrap(),
        content.replace(&ghp, GH_MARK),
        "only the token is replaced"
    );
    assert!(!on_disk(&db, &ghp));
}

#[test]
fn a_busy_database_fails_fast_and_changes_nothing() {
    let (_d, db) = temp_db();
    let l = seed_legacy(&db);
    let before = rows(&db);

    // Another process holding the write lock (a hook in the middle of a write).
    let mut writer = Connection::open(&db).unwrap();
    let tx = writer
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();

    let started = Instant::now();
    let err = scrub_secrets_with_timeout(&db, false, Duration::from_millis(200)).unwrap_err();
    assert!(matches!(err, ScrubError::Busy), "{err}");
    assert!(err.to_string().contains("busy"));
    assert!(started.elapsed() < Duration::from_secs(3), "did not hang");

    // A dry run still works next to a writer (WAL readers are not blocked).
    let preview = scrub_secrets_with_timeout(&db, true, Duration::from_millis(200)).unwrap();
    assert_eq!(preview.rows_changed, 3);

    tx.rollback().unwrap();
    drop(writer);
    assert_eq!(rows(&db), before, "nothing changed");
    assert!(on_disk(&db, &l.aws));
}

#[test]
fn a_reader_blocking_the_purge_is_reported_and_a_rerun_finishes_it() {
    let (_d, db) = temp_db();
    let l = seed_legacy(&db);
    // Content larger than a page lives in overflow pages: without
    // secure_delete the page that held the key keeps it after the UPDATE.
    let big = "x".repeat(3000);
    {
        let conn = Connection::open(&db).unwrap();
        let content = format!("{big}{}{big}", l.aws);
        insert_legacy(&conn, "overflow", "cat big", None, None, Some(&content));
    }

    // A reader holding a snapshot keeps the checkpoint from completing.
    let reader = Connection::open(&db).unwrap();
    reader.execute_batch("BEGIN;").unwrap();
    let _: i64 = reader
        .query_row("SELECT COUNT(*) FROM interceptions", [], |r| r.get(0))
        .unwrap();

    let err = scrub_secrets_with_timeout(&db, false, Duration::from_millis(200)).unwrap_err();
    match &err {
        ScrubError::PurgeIncomplete { report, .. } => assert_eq!(report.rows_changed, 4),
        other => panic!("expected PurgeIncomplete, got {other}"),
    }
    assert!(err.to_string().contains("again"), "{err}");

    reader.execute_batch("COMMIT;").unwrap();
    drop(reader);
    // Even before a re-run, a plain checkpoint (as any later hook's connection
    // does) must not bring the secret back into metrics.db: secure_delete zeroed
    // the replaced cells in the page images the scrub committed.
    let conn = Connection::open(&db).unwrap();
    let _: i64 = conn
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
        .unwrap();
    assert!(!on_disk(&db, &l.aws), "secure_delete left the AWS key");
    assert!(!on_disk(&db, &l.ghp), "secure_delete left the GitHub token");
    drop(conn);
    // The rows are already masked: the re-run changes none and purges the bytes.
    assert_eq!(
        row(&db, "aws-cmd").1.unwrap(),
        l.cmd_aws.replace(&l.aws, AWS_MARK)
    );
    let again = scrub_secrets(&db, false).unwrap();
    assert_eq!(again.rows_changed, 0);
    assert!(!on_disk(&db, &l.aws));
    assert!(!on_disk(&db, &l.ghp));
}

// ── command line ─────────────────────────────────────────────────────────

struct Home {
    dir: TempDir,
}

impl Home {
    fn new() -> Self {
        Home {
            dir: TempDir::new().unwrap(),
        }
    }
    fn db(&self) -> PathBuf {
        self.dir.path().join("ecotokens").join("metrics.db")
    }
}

fn run(home: &Home, args: &[&str], stdin: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ecotokens"));
    cmd.arg("clear")
        .args(args)
        .env("HOME", home.dir.path())
        .env("XDG_CONFIG_HOME", home.dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match stdin {
        Some(_) => cmd.stdin(Stdio::piped()),
        None => cmd.stdin(Stdio::null()),
    };
    let mut child = cmd.spawn().expect("failed to run ecotokens clear");
    if let Some(input) = stdin {
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

fn stdout_of(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr_of(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn seeded_home() -> (Home, Legacy) {
    let home = Home::new();
    std::fs::create_dir_all(home.db().parent().unwrap()).unwrap();
    let l = seed_legacy(&home.db());
    (home, l)
}

#[test]
fn cli_dry_run_reports_and_leaves_the_file_byte_identical() {
    let (home, l) = seeded_home();
    let before = std::fs::read(home.db()).unwrap();
    let out = run(&home, &["--scrub-secrets", "--dry-run"], None);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let text = stdout_of(&out);
    assert!(text.contains("Scanned 5 interception(s)"), "{text}");
    assert!(
        text.contains(
            "Would mask secrets in 3 row(s): command (2), git_root (1), content_after (1)"
        ),
        "{text}"
    );
    assert!(text.contains("nothing was written"), "{text}");
    assert!(!text.contains(&l.aws) && !text.contains(&l.ghp));
    assert_eq!(std::fs::read(home.db()).unwrap(), before);
}

#[test]
fn cli_without_yes_and_without_a_terminal_refuses_and_changes_nothing() {
    let (home, l) = seeded_home();
    let before = std::fs::read(home.db()).unwrap();

    let out = run(&home, &["--scrub-secrets"], None);
    assert_eq!(out.status.code(), Some(1), "{}", stdout_of(&out));
    assert!(stderr_of(&out).contains("--yes"), "{}", stderr_of(&out));
    assert_eq!(std::fs::read(home.db()).unwrap(), before);

    // A piped "y" is not a confirmation either.
    let out = run(&home, &["--scrub-secrets"], Some("y\n"));
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(std::fs::read(home.db()).unwrap(), before);
    assert!(on_disk(&home.db(), &l.aws));
}

#[test]
fn cli_with_yes_scrubs_purges_and_is_idempotent() {
    let (home, l) = seeded_home();
    let out = run(&home, &["--scrub-secrets", "--yes"], None);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let text = stdout_of(&out);
    assert!(
        text.contains("Masked secrets in 3 row(s): command (2), git_root (1), content_after (1)"),
        "{text}"
    );
    assert!(text.contains("VACUUM"), "{text}");
    assert!(text.contains("filesystem snapshots"), "{text}");
    assert!(!on_disk(&home.db(), &l.aws));
    assert!(!on_disk(&home.db(), &l.ghp));
    assert_eq!(
        row(&home.db(), "aws-cmd").1.unwrap(),
        format!("echo {AWS_MARK}")
    );

    let out = run(&home, &["--scrub-secrets", "-y"], None);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(
        stdout_of(&out).contains("0 row(s) changed"),
        "{}",
        stdout_of(&out)
    );
}

#[test]
fn cli_nothing_to_change_needs_no_confirmation() {
    let home = Home::new();
    std::fs::create_dir_all(home.db().parent().unwrap()).unwrap();
    init_store(&home.db());
    let out = run(&home, &["--scrub-secrets"], None);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("0 row(s) changed"));
}

#[test]
fn cli_missing_database_is_reported_and_not_created() {
    let home = Home::new();
    for args in [
        &["--scrub-secrets", "--dry-run"][..],
        &["--scrub-secrets", "-y"],
    ] {
        let out = run(&home, args, None);
        assert!(out.status.success(), "{}", stderr_of(&out));
        assert!(
            stdout_of(&out).contains("nothing to scrub"),
            "{}",
            stdout_of(&out)
        );
    }
    assert!(!home.db().exists());
}

#[test]
fn cli_legacy_jsonl_file_is_pointed_out_and_left_alone() {
    let (home, _l) = seeded_home();
    let legacy = home.db().with_extension("jsonl.migrated");
    std::fs::write(&legacy, "{}\n").unwrap();
    let out = run(&home, &["--scrub-secrets", "--dry-run"], None);
    assert!(out.status.success());
    assert!(
        stdout_of(&out).contains("metrics.jsonl.migrated"),
        "{}",
        stdout_of(&out)
    );
    assert_eq!(std::fs::read_to_string(&legacy).unwrap(), "{}\n");
}

#[test]
fn cli_busy_database_fails_with_a_clear_error_instead_of_hanging() {
    let (home, l) = seeded_home();
    let mut writer = Connection::open(home.db()).unwrap();
    let tx = writer
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let started = Instant::now();
    let out = run(&home, &["--scrub-secrets", "--yes"], None);
    assert_eq!(out.status.code(), Some(1), "{}", stdout_of(&out));
    assert!(stderr_of(&out).contains("busy"), "{}", stderr_of(&out));
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "gave up after the busy timeout"
    );
    tx.rollback().unwrap();
    drop(writer);
    assert_eq!(row(&home.db(), "aws-cmd").1.unwrap(), l.cmd_aws);
}

#[test]
fn cli_scrub_conflicts_with_the_deletion_filters() {
    let home = Home::new();
    for args in [
        &["--scrub-secrets", "--all"][..],
        &["--scrub-secrets", "--family", "git"],
        &["--scrub-secrets", "--older-than", "30d"],
        &["--dry-run", "--all"],
    ] {
        let out = run(&home, args, None);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", stderr_of(&out));
    }
}
