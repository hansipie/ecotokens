use ecotokens::config::Settings;
use ecotokens::jev::Usage;
use ecotokens::router::stats::{cost_usd, record, summarize};
use ecotokens::router::{Decision, Routing, Size};
use rusqlite::Connection;
use std::path::Path;
use tempfile::TempDir;

fn routing(decision: Decision, size: Option<Size>, tokens: Option<(u64, u64)>) -> Routing {
    Routing {
        size,
        confidence: size.map(|_| 0.9),
        followup_prob: size.map(|_| 0.1),
        usage: tokens.map(|(i, o)| Usage {
            input_tokens: i,
            output_tokens: o,
        }),
        latency_ms: if tokens.is_some() { 2000 } else { 0 },
        ..Routing::without_jev(decision)
    }
}

#[test]
fn summary_counts_sizes_decisions_and_tokens() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("router.db");
    let rows = [
        routing(Decision::Delegated, Some(Size::Tiny), Some((500, 60))),
        routing(Decision::Delegated, Some(Size::Tiny), Some((500, 60))),
        routing(Decision::SelfUnsure, Some(Size::Large), Some((500, 60))),
        routing(Decision::SelfFollowup, Some(Size::Tiny), Some((500, 60))),
        routing(Decision::Skipped, None, None),
        routing(Decision::Error, None, None),
    ];
    for r in &rows {
        record(&db, r).unwrap();
    }
    let s = summarize(&db, &Settings::default()).unwrap();
    assert_eq!(s.messages, 6);
    assert_eq!(s.by_size["tiny"].picked, 3);
    assert_eq!(s.by_size["tiny"].delegated, 2);
    assert_eq!(s.by_size["large"].picked, 1);
    assert_eq!(s.by_size["large"].delegated, 0);
    assert_eq!(s.by_size["hardest"].picked, 0);
    assert_eq!(s.by_decision["delegated"], 2);
    assert_eq!(s.by_decision["skipped"], 1);
    assert_eq!(s.jev_requests, 5);
    assert_eq!(s.input_tokens, 2000);
    assert_eq!(s.output_tokens, 240);
    assert_eq!(s.cost_usd, None);
}

#[test]
fn empty_store_reads_as_zero() {
    let dir = TempDir::new().unwrap();
    let s = summarize(&dir.path().join("none.db"), &Settings::default()).unwrap();
    assert_eq!(s.messages, 0);
    assert_eq!(s.by_size.len(), 4);
}

#[test]
fn cost_needs_a_configured_price() {
    let mut settings = Settings::default();
    assert_eq!(cost_usd(1_000_000, 1_000_000, &settings), None);
    settings.jev_usd_per_mtok_input = Some(0.5);
    settings.jev_usd_per_mtok_output = Some(2.0);
    let cost = cost_usd(2_000_000, 500_000, &settings).unwrap();
    assert!((cost - 2.0).abs() < 1e-9, "{cost}");
}

fn columns(db: &Path, table: &str) -> Vec<String> {
    let conn = Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    let names = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    names
}

const NEW_COLUMNS: [&str; 10] = [
    "session_id",
    "nature",
    "risk",
    "ambiguity",
    "verifiability",
    "rule",
    "route",
    "raised",
    "policy_version",
    "models",
];

const OLD_SCHEMA: &str = "CREATE TABLE router_decisions (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp      TEXT NOT NULL,
    decision       TEXT NOT NULL,
    size           TEXT,
    confidence     REAL,
    followup_prob  REAL,
    input_tokens   INTEGER NOT NULL DEFAULT 0,
    output_tokens  INTEGER NOT NULL DEFAULT 0,
    latency_ms     INTEGER NOT NULL DEFAULT 0
);
INSERT INTO router_decisions (timestamp, decision, size, confidence, followup_prob,
    input_tokens, output_tokens, latency_ms)
VALUES ('2026-01-01T00:00:00Z', 'delegated', 'tiny', 0.9, 0.1, 500, 60, 1800);";

#[test]
fn a_fresh_database_has_the_new_columns_and_the_outcomes_table() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("router.db");
    record(&db, &routing(Decision::Delegated, Some(Size::Tiny), None)).unwrap();
    let cols = columns(&db, "router_decisions");
    for c in NEW_COLUMNS {
        assert!(
            cols.contains(&c.to_string()),
            "missing column {c}: {cols:?}"
        );
    }
    let outcome_cols = columns(&db, "router_outcomes");
    for c in [
        "decision_id",
        "cost_usd",
        "tokens_in",
        "tokens_out",
        "duration_ms",
        "runs",
        "success",
        "correction",
        "regressions",
        "judged_at",
        "reminded",
    ] {
        assert!(outcome_cols.contains(&c.to_string()), "missing {c}");
    }
}

#[test]
fn an_old_database_gains_the_columns_and_keeps_its_rows() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("router.db");
    Connection::open(&db)
        .unwrap()
        .execute_batch(OLD_SCHEMA)
        .unwrap();

    record(&db, &routing(Decision::SelfUnsure, Some(Size::Large), None)).unwrap();

    let cols = columns(&db, "router_decisions");
    for c in NEW_COLUMNS {
        assert!(cols.contains(&c.to_string()), "missing column {c}");
    }
    let s = summarize(&db, &Settings::default()).unwrap();
    assert_eq!(s.messages, 2, "the old row is kept");
    assert_eq!(s.by_size["tiny"].delegated, 1);
    assert_eq!(s.input_tokens, 500);
    let conn = Connection::open(&db).unwrap();
    let (decision, latency): (String, i64) = conn
        .query_row(
            "SELECT decision, latency_ms FROM router_decisions WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((decision.as_str(), latency), ("delegated", 1800));
}

#[test]
fn opening_the_same_old_database_concurrently_never_fails() {
    for round in 0..5 {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join(format!("router{round}.db"));
        Connection::open(&db)
            .unwrap()
            .execute_batch(OLD_SCHEMA)
            .unwrap();
        let handles: Vec<_> = (0..6)
            .map(|_| {
                let db = db.clone();
                std::thread::spawn(move || {
                    record(&db, &routing(Decision::Delegated, Some(Size::Tiny), None))
                })
            })
            .collect();
        for h in handles {
            h.join()
                .unwrap()
                .expect("concurrent migration must not fail");
        }
        let s = summarize(&db, &Settings::default()).unwrap();
        assert_eq!(s.messages, 7);
    }
}

#[test]
fn rows_without_a_policy_version_are_reported_as_legacy_size() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("router.db");
    Connection::open(&db)
        .unwrap()
        .execute_batch(OLD_SCHEMA)
        .unwrap();
    let mut with_policy = routing(Decision::Delegated, Some(Size::Tiny), None);
    with_policy.policy_version = Some("2026.1".into());
    record(&db, &with_policy).unwrap();

    let s = summarize(&db, &Settings::default()).unwrap();
    assert_eq!(s.by_policy["legacy-size"].messages, 1);
    assert_eq!(s.by_policy["2026.1"].messages, 1);
}

#[test]
fn new_columns_are_written_and_the_old_ones_keep_their_meaning() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("router.db");
    let mut r = routing(Decision::Delegated, Some(Size::Large), Some((500, 60)));
    r.rule = Some("bounded-bug".into());
    r.route = Some("router-large".into());
    r.raised = true;
    r.policy_version = Some("2026.1".into());
    r.session_id = Some("abc".into());
    record(&db, &r).unwrap();
    let conn = Connection::open(&db).unwrap();
    let row: (String, String, i64, String, String, i64, i64) = conn
        .query_row(
            "SELECT rule, route, raised, policy_version, session_id, input_tokens, latency_ms
             FROM router_decisions",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        row,
        (
            "bounded-bug".into(),
            "router-large".into(),
            1,
            "2026.1".into(),
            "abc".into(),
            500,
            2000
        )
    );
}
