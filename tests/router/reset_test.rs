#[path = "common.rs"]
mod common;
use common::{run_ecotokens, stderr_of, stdout_of, temp_config_home, write_config, ConfigHome};

use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use ecotokens::jev::stats::{record_in as record_jev_call, CallRecord, Purpose};
use ecotokens::router::outcome::{attach_automatic, record_judgment, Judgment, Measurement};
use ecotokens::router::stats::{
    backup, backup_path, record_returning_id, reset, routing_rows, RoutingRows,
};
use ecotokens::router::{Decision, Routing, Size};
use rusqlite::Connection;
use serde_json::{json, Value};
use tempfile::TempDir;

fn delegated() -> Routing {
    let mut r = Routing::without_jev(Decision::Delegated);
    r.size = Some(Size::Large);
    r.route = Some("router-large".into());
    r.session_id = Some("s".into());
    r
}

fn jev_call() -> CallRecord {
    CallRecord {
        purpose: Purpose::Router,
        ok: true,
        error_kind: None,
        http_status: None,
        latency_ms: 900,
        usage: None,
        agent: None,
    }
}

/// Three decisions, two outcomes (one automatic, one judged) and one Jev call.
fn seed(db: &Path) -> Vec<i64> {
    let ids: Vec<i64> = (0..3)
        .map(|_| record_returning_id(db, &delegated()).unwrap())
        .collect();
    attach_automatic(
        db,
        ids[0],
        &Measurement {
            cost_usd: Some(0.01),
            tokens_in: Some(100),
            tokens_out: Some(10),
            duration_ms: Some(1000),
            runs: Some(1),
        },
    )
    .unwrap();
    record_judgment(
        db,
        ids[1],
        &Judgment {
            success: Some(true),
            correction: None,
            regressions: None,
        },
    )
    .unwrap();
    record_jev_call(db, &jev_call(), Utc::now(), None).unwrap();
    ids
}

fn count(db: &Path, table: &str) -> i64 {
    Connection::open(db)
        .unwrap()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

fn temp_db() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("router.db");
    (dir, db)
}

// ── library ──────────────────────────────────────────────────────────────

#[test]
fn reset_deletes_decisions_and_outcomes_and_keeps_the_jev_call_log() {
    let (_d, db) = temp_db();
    seed(&db);
    assert_eq!(
        routing_rows(&db).unwrap(),
        RoutingRows {
            decisions: 3,
            outcomes: 2
        }
    );

    let deleted = reset(&db).unwrap();
    assert_eq!(
        deleted,
        RoutingRows {
            decisions: 3,
            outcomes: 2
        }
    );
    assert_eq!(count(&db, "router_decisions"), 0);
    assert_eq!(count(&db, "router_outcomes"), 0);
    assert_eq!(count(&db, "jev_calls"), 1, "the Jev call log is kept");
    assert!(routing_rows(&db).unwrap().is_empty());
}

#[test]
fn ids_stay_monotonic_after_a_reset() {
    let (_d, db) = temp_db();
    let ids = seed(&db);
    reset(&db).unwrap();
    let next = record_returning_id(&db, &delegated()).unwrap();
    assert!(
        next > *ids.iter().max().unwrap(),
        "a new decision never reuses an old id: {next} vs {ids:?}"
    );
}

#[test]
fn resetting_twice_or_an_empty_database_reports_zero() {
    let (_d, db) = temp_db();
    seed(&db);
    reset(&db).unwrap();
    assert_eq!(reset(&db).unwrap(), RoutingRows::default());
}

#[test]
fn a_missing_database_reports_zero_and_is_not_created() {
    let (_d, db) = temp_db();
    assert_eq!(routing_rows(&db).unwrap(), RoutingRows::default());
    assert_eq!(reset(&db).unwrap(), RoutingRows::default());
    assert!(!db.exists());
}

#[test]
fn the_backup_is_a_full_copy_that_survives_the_reset() {
    let (dir, db) = temp_db();
    seed(&db);
    // Leave the rows in the WAL: the copy must still include them.
    let _reader = Connection::open(&db).unwrap();
    let dest = dir.path().join("router.db.bak-test");
    backup(&db, &dest).unwrap();
    reset(&db).unwrap();
    assert_eq!(count(&dest, "router_decisions"), 3);
    assert_eq!(count(&dest, "router_outcomes"), 2);
    assert_eq!(count(&dest, "jev_calls"), 1);
    assert_eq!(count(&db, "router_decisions"), 0);
}

#[test]
fn a_backup_never_overwrites_an_existing_file() {
    let (dir, db) = temp_db();
    seed(&db);
    let dest = dir.path().join("taken");
    std::fs::write(&dest, "keep me").unwrap();
    assert!(backup(&db, &dest).is_err());
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "keep me");
}

#[test]
fn the_backup_is_named_after_the_database_and_the_time() {
    let at = Utc.with_ymd_and_hms(2026, 10, 19, 8, 5, 9).unwrap();
    let p = backup_path(Path::new("/x/ecotokens/router.db"), at);
    assert_eq!(
        p,
        PathBuf::from("/x/ecotokens/router.db.bak-20261019-080509")
    );
}

// ── command line ─────────────────────────────────────────────────────────

fn run(home: &ConfigHome, args: &[&str]) -> std::process::Output {
    run_ecotokens(home, args, None, None)
}

fn json_of(out: &std::process::Output) -> Value {
    serde_json::from_str(&stdout_of(out))
        .unwrap_or_else(|e| panic!("not JSON ({e}): {}", stdout_of(out)))
}

fn backups(home: &ConfigHome) -> Vec<PathBuf> {
    std::fs::read_dir(home.ecotokens_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("router.db.bak-")
        })
        .collect()
}

#[test]
fn without_yes_a_non_interactive_reset_refuses_and_deletes_nothing() {
    let home = temp_config_home();
    seed(&home.router_db());
    let out = run(&home, &["router", "reset"]);
    assert!(!out.status.success(), "{}", stdout_of(&out));
    assert!(stderr_of(&out).contains("--yes"), "{}", stderr_of(&out));
    assert_eq!(count(&home.router_db(), "router_decisions"), 3);
    assert_eq!(count(&home.router_db(), "router_outcomes"), 2);

    // A piped "y" is not a confirmation either.
    let out = run_ecotokens(&home, &["router", "reset"], None, Some("y\n"));
    assert!(!out.status.success());
    assert_eq!(count(&home.router_db(), "router_decisions"), 3);
}

#[test]
fn reset_with_yes_reports_the_counts_and_keeps_settings_rules_and_jev_calls() {
    let home = temp_config_home();
    write_config(&home, &json!({ "router_private": ["/secret"] }));
    let rules = home.ecotokens_dir().join("router_rules.toml");
    std::fs::write(&rules, "[[rule]]\nname = \"x\"\nroute = \"large\"\n").unwrap();
    seed(&home.router_db());

    let out = run(&home, &["router", "reset", "--yes", "--json"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(
        json_of(&out),
        json!({ "decisions": 3, "outcomes": 2, "backup": null })
    );
    assert_eq!(count(&home.router_db(), "router_decisions"), 0);
    assert_eq!(count(&home.router_db(), "router_outcomes"), 0);
    assert_eq!(count(&home.router_db(), "jev_calls"), 1);
    assert_eq!(
        common::read_config(&home)["router_private"],
        json!(["/secret"])
    );
    assert!(rules.exists());
    assert!(backups(&home).is_empty(), "no backup unless asked");

    // Human output, on what is now an empty database.
    let out = run(&home, &["router", "reset", "-y"]);
    assert!(out.status.success());
    let text = stdout_of(&out);
    assert!(
        text.contains("deleted 0 decision(s) and 0 outcome(s)"),
        "{text}"
    );
}

#[test]
fn reset_with_backup_writes_a_readable_copy_first() {
    let home = temp_config_home();
    seed(&home.router_db());
    let out = run(&home, &["router", "reset", "--yes", "--backup", "--json"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let v = json_of(&out);
    assert_eq!(v["decisions"], 3);
    let reported = PathBuf::from(v["backup"].as_str().unwrap());
    assert_eq!(backups(&home), vec![reported.clone()]);
    assert_eq!(count(&reported, "router_decisions"), 3);
    assert_eq!(count(&reported, "router_outcomes"), 2);
    assert_eq!(count(&home.router_db(), "router_decisions"), 0);
}

#[test]
fn an_empty_store_resets_to_zero_without_a_question() {
    let home = temp_config_home();
    let out = run(&home, &["router", "reset", "--json"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(
        json_of(&out),
        json!({ "decisions": 0, "outcomes": 0, "backup": null })
    );
    assert!(!home.router_db().exists(), "no database is created");

    let out = run(&home, &["router", "reset", "--backup"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(
        stdout_of(&out).contains("deleted 0 decision(s)"),
        "{}",
        stdout_of(&out)
    );
}
