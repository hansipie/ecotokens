use chrono::{Duration, Utc};
use ecotokens::config::Settings;
use ecotokens::jev::stats::{
    project_key, record, record_at, record_in, summarize, CallRecord, Purpose,
};
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
    let s = summarize(
        &dir.path().join("none.db"),
        &Settings::default(),
        None,
        None,
    )
    .unwrap();
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
    let s = summarize(&path, &settings, None, None).unwrap();

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
    let s = summarize(&path, &Settings::default(), since, None).unwrap();
    assert_eq!(s.calls, 1);
    assert_eq!(
        summarize(&path, &Settings::default(), None, None)
            .unwrap()
            .calls,
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
    let s = summarize(&path, &Settings::default(), None, None).unwrap();
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
    let s = summarize(&path, &Settings::default(), None, None).unwrap();
    assert_eq!(s.calls, 1);
    assert_eq!(s.recent[0].agent, None);
}

#[test]
fn summarize_keeps_one_project_when_asked() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jev.db");
    let at = Utc::now();
    record_in(&path, &ok(Purpose::Router, 10, 5, 1), at, Some("/p")).unwrap();
    record_in(&path, &ok(Purpose::FilterLines, 20, 5, 1), at, Some("/q")).unwrap();
    record_in(&path, &ok(Purpose::Router, 30, 5, 1), at, None).unwrap();

    let s = summarize(&path, &Settings::default(), None, Some("/p")).unwrap();
    assert_eq!(s.calls, 1);
    assert_eq!(s.by_purpose["router"].calls, 1);
    assert_eq!(s.by_purpose["filter_lines"].calls, 0);
    assert_eq!(
        summarize(&path, &Settings::default(), None, None)
            .unwrap()
            .calls,
        3,
        "no project: every call"
    );
}

#[test]
fn old_database_without_project_column_is_migrated_and_old_calls_have_no_project() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE jev_calls (
             id INTEGER PRIMARY KEY AUTOINCREMENT, timestamp TEXT NOT NULL,
             purpose TEXT NOT NULL, ok INTEGER NOT NULL, error_kind TEXT,
             http_status INTEGER, latency_ms INTEGER NOT NULL DEFAULT 0,
             input_tokens INTEGER NOT NULL DEFAULT 0,
             output_tokens INTEGER NOT NULL DEFAULT 0, agent TEXT);
         INSERT INTO jev_calls (timestamp, purpose, ok)
             VALUES ('2026-09-24T10:00:00+00:00', 'router', 1);",
    )
    .unwrap();
    drop(conn);

    record_in(
        &path,
        &ok(Purpose::Router, 10, 5, 1),
        Utc::now(),
        Some("/p"),
    )
    .unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    let projects: Vec<Option<String>> = conn
        .prepare("SELECT project FROM jev_calls ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(projects, [None, Some("/p".to_string())]);

    let s = summarize(&path, &Settings::default(), None, Some("/p")).unwrap();
    assert_eq!(s.calls, 1, "the old call has no project and is left out");
    assert_eq!(
        summarize(&path, &Settings::default(), None, None)
            .unwrap()
            .calls,
        2
    );
}

#[test]
fn project_key_of_a_sub_directory_is_its_git_root() {
    let dir = tempfile::tempdir().unwrap();
    let init = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .output()
        .expect("git init should run");
    assert!(init.status.success());
    let sub = dir.path().join("src").join("deep");
    std::fs::create_dir_all(&sub).unwrap();

    let root = project_key(dir.path());
    assert_eq!(project_key(&sub), root);
    assert_eq!(
        std::fs::canonicalize(&root).unwrap(),
        std::fs::canonicalize(dir.path()).unwrap()
    );
}

#[test]
fn project_key_outside_git_is_the_path_itself() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(project_key(dir.path()), dir.path().to_string_lossy());
}
