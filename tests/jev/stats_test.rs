use chrono::{Duration, Utc};
use ecotokens::config::Settings;
use ecotokens::jev::stats::{record, record_at, summarize, CallRecord, Purpose};
use ecotokens::jev::{JevError, Usage};

fn ok(purpose: Purpose, latency_ms: u64, input: u64, output: u64) -> CallRecord {
    CallRecord {
        purpose,
        ok: true,
        error_kind: None,
        http_status: None,
        latency_ms,
        usage: Some(Usage {
            input_tokens: input,
            output_tokens: output,
        }),
        agent: None,
    }
}

#[test]
fn missing_db_reads_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    let s = summarize(&dir.path().join("none.db"), &Settings::default(), None).unwrap();
    assert_eq!(s.calls, 0);
    assert_eq!(s.by_purpose.len(), Purpose::ALL.len());
    assert!(s.recent.is_empty());
}

#[test]
fn aggregates_by_purpose_latency_tokens_and_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("j.db");
    record(&path, &ok(Purpose::FilterLines, 100, 10, 2)).unwrap();
    record(&path, &ok(Purpose::FilterLines, 300, 30, 4)).unwrap();
    record(&path, &ok(Purpose::Router, 200, 5, 1)).unwrap();
    let failed: Result<(), JevError> = Err(JevError::Http(503));
    record(
        &path,
        &CallRecord::from_result(Purpose::Verify, &failed, 50, None),
    )
    .unwrap();

    let settings = Settings {
        jev_usd_per_mtok_input: Some(1.0),
        jev_usd_per_mtok_output: Some(2.0),
        ..Default::default()
    };
    let s = summarize(&path, &settings, None).unwrap();

    assert_eq!((s.calls, s.ok, s.fallbacks), (4, 3, 1));
    assert_eq!((s.input_tokens, s.output_tokens), (45, 7));
    assert_eq!(s.avg_latency_ms, 162);
    assert_eq!(s.p95_latency_ms, 300);
    assert_eq!(s.by_purpose["filter_lines"].calls, 2);
    assert_eq!(s.by_purpose["filter_lines"].avg_latency_ms, 200);
    assert_eq!(s.by_purpose["verify"].ok, 0);
    assert_eq!(s.errors["http 503"], 1);
    assert_eq!(s.recent[0].purpose, "verify", "newest first");
    assert_eq!(s.timeline.iter().sum::<u64>(), 4);
    let cost = s.cost_usd.unwrap();
    assert!((cost - (45.0 / 1e6 + 14.0 / 1e6)).abs() < 1e-12, "{cost}");
}

#[test]
fn since_filters_old_calls() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("j.db");
    let old = Utc::now() - Duration::days(10);
    record_at(&path, &ok(Purpose::Classify, 10, 1, 1), old).unwrap();
    record(&path, &ok(Purpose::Classify, 10, 1, 1)).unwrap();
    let since = Some(Utc::now() - Duration::days(7));
    let s = summarize(&path, &Settings::default(), since).unwrap();
    assert_eq!(s.calls, 1);
    assert_eq!(
        summarize(&path, &Settings::default(), None).unwrap().calls,
        2
    );
}

#[test]
fn router_agent_is_stored_and_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jev.db");
    let mut call = ok(Purpose::Router, 10, 1, 1);
    call.agent = Some("router-everyday".into());
    record(&path, &call).unwrap();
    record(&path, &ok(Purpose::Verify, 10, 1, 1)).unwrap();
    let s = summarize(&path, &Settings::default(), None).unwrap();
    assert_eq!(s.recent[1].agent.as_deref(), Some("router-everyday"));
    assert_eq!(s.recent[0].agent, None);
}

#[test]
fn old_database_without_agent_column_is_migrated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE jev_calls (
             id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp TEXT NOT NULL,
             purpose TEXT NOT NULL, ok INTEGER NOT NULL, error_kind TEXT,
             http_status INTEGER, latency_ms INTEGER NOT NULL DEFAULT 0,
             input_tokens INTEGER NOT NULL DEFAULT 0,
             output_tokens INTEGER NOT NULL DEFAULT 0);
         INSERT INTO jev_calls (timestamp, purpose, ok)
             VALUES ('2026-09-24T10:00:00+00:00', 'router', 1);",
    )
    .unwrap();
    drop(conn);
    let s = summarize(&path, &Settings::default(), None).unwrap();
    assert_eq!(s.calls, 1);
    assert_eq!(s.recent[0].agent, None);
}
