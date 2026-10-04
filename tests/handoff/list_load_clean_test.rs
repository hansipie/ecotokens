#[path = "common.rs"]
mod common;
use common::*;

use ecotokens::config::Settings;
use ecotokens::handoff::cli::{clean, list, load};
use ecotokens::handoff::hook::process;
use ecotokens::handoff::store::{read_handoff, write_handoff, write_session_record};
use ecotokens::handoff::{SessionRecord, Status};
use tempfile::TempDir;

const CWD: &str = "/home/u/proj";

fn put(dir: &std::path::Path, id: &str, cwd: &str, hours_old: i64) {
    let mut h = sample_handoff(id, cwd);
    h.created = hours_before(fixed_now(), hours_old);
    h.objective = Some(format!("objective of {id}\nsecond line"));
    write_handoff(dir, &h).unwrap();
}

fn json(out: &str) -> serde_json::Value {
    serde_json::from_str(out).unwrap_or_else(|e| panic!("not JSON ({e}): {out}"))
}

fn ids(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect()
}

// ── list ────────────────────────────────────────────────────────────────────

#[test]
fn list_shows_this_directory_newest_first_and_all_with_the_flag() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "older", CWD, 5);
    put(dir.path(), "newer", CWD, 1);
    put(dir.path(), "other", "/home/u/elsewhere", 2);
    let here = list(dir.path(), &settings(), CWD, false, true, fixed_now());
    assert_eq!(here.code, 0);
    assert_eq!(ids(&json(&here.out)), ["newer", "older"]);
    let all = list(dir.path(), &settings(), CWD, true, true, fixed_now());
    assert_eq!(ids(&json(&all.out)).len(), 3);
}

#[test]
fn list_json_follows_the_contract() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "abc123", CWD, 30);
    let out = list(dir.path(), &settings(), CWD, false, true, fixed_now());
    let entry = &json(&out.out)[0];
    assert_eq!(entry["id"], "abc123");
    assert_eq!(entry["created"], "2026-10-03T12:20:11Z");
    assert_eq!(entry["age_hours"], 30);
    assert_eq!(entry["status"], "pending");
    assert_eq!(entry["stale"], true);
    assert_eq!(entry["objective"], "objective of abc123", "first line only");
    assert_eq!(entry["cwd"], CWD);
}

#[test]
fn list_marks_corrupted_files() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("broken.md"), "garbage").unwrap();
    let out = list(dir.path(), &settings(), CWD, false, true, fixed_now());
    let v = json(&out.out);
    assert_eq!(v[0]["id"], "broken");
    assert_eq!(v[0]["status"], "corrupted");
    let human = list(dir.path(), &settings(), CWD, false, false, fixed_now());
    assert!(human.out.contains("corrupted"), "{}", human.out);
}

#[test]
fn list_human_output_has_a_row_per_handoff() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "abc123", CWD, 3);
    let out = list(dir.path(), &settings(), CWD, false, false, fixed_now());
    assert!(out.out.contains("abc123"), "{}", out.out);
    assert!(out.out.contains("pending"), "{}", out.out);
    assert!(out.out.contains("objective of abc123"), "{}", out.out);
}

#[test]
fn list_on_an_empty_directory_says_so_and_succeeds() {
    let root = TempDir::new().unwrap();
    let dir = root.path().join("never-created");
    let out = list(&dir, &settings(), CWD, false, false, fixed_now());
    assert_eq!(out.code, 0);
    assert!(
        out.out.contains("No handoff for this directory."),
        "{}",
        out.out
    );
    let as_json = list(&dir, &settings(), CWD, false, true, fixed_now());
    assert_eq!(json(&as_json.out), serde_json::json!([]));
}

// ── load ────────────────────────────────────────────────────────────────────

#[test]
fn load_prints_the_injectable_text_and_consumes_a_pending_handoff() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "abc123", CWD, 1);
    let out = load(dir.path(), &settings(), "abc123", false, fixed_now());
    assert_eq!(out.code, 0, "{}", out.err);
    assert!(out.out.starts_with("[ecotokens handoff]"), "{}", out.out);
    assert!(out.out.contains("objective of abc123"), "{}", out.out);
    assert_eq!(
        read_handoff(dir.path(), "abc123").unwrap().status,
        Status::Consumed
    );
}

#[test]
fn load_of_a_consumed_handoff_prints_it_and_leaves_its_status_alone() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "abc123", CWD, 1);
    load(dir.path(), &settings(), "abc123", false, fixed_now());
    let first = read_handoff(dir.path(), "abc123").unwrap();
    let later = fixed_now() + chrono::Duration::hours(2);
    let out = load(dir.path(), &settings(), "abc123", false, later);
    assert_eq!(out.code, 0);
    assert!(out.out.contains("objective of abc123"));
    let again = read_handoff(dir.path(), "abc123").unwrap();
    assert_eq!(
        again.consumed, first.consumed,
        "the consumption time did not move"
    );
}

#[test]
fn load_json_carries_the_text_and_the_flags() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "abc123", CWD, 30);
    let out = load(dir.path(), &settings(), "abc123", true, fixed_now());
    let v = json(&out.out);
    assert_eq!(v["id"], "abc123");
    assert_eq!(v["stale"], true);
    assert_eq!(v["consumed_now"], true);
    assert!(v["text"]
        .as_str()
        .unwrap()
        .contains("verify it before relying on it"));
}

#[test]
fn load_of_an_unknown_or_malformed_id_lists_what_exists() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "abc123", CWD, 1);
    for bad in ["nope", "../x", ""] {
        let out = load(dir.path(), &settings(), bad, false, fixed_now());
        assert_eq!(out.code, 1, "{bad:?}");
        assert!(out.err.contains("abc123"), "{bad:?}: {}", out.err);
        assert!(out.out.is_empty(), "nothing is loaded");
    }
}

#[test]
fn load_of_a_corrupted_file_says_so() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("broken.md"), "garbage").unwrap();
    let out = load(dir.path(), &settings(), "broken", false, fixed_now());
    assert_eq!(out.code, 1);
    assert!(out.err.contains("corrupted"), "{}", out.err);
}

// ── clean ───────────────────────────────────────────────────────────────────

fn record(dir: &std::path::Path, id: &str, days_old: i64) {
    write_session_record(
        dir,
        &SessionRecord {
            session_id: id.into(),
            transcript_path: "/t".into(),
            cwd: CWD.into(),
            source: "startup".into(),
            recorded: fixed_now() - chrono::Duration::days(days_old),
        },
    )
    .unwrap();
}

fn put_days(dir: &std::path::Path, id: &str, days_old: i64) {
    let mut h = sample_handoff(id, CWD);
    h.created = fixed_now() - chrono::Duration::days(days_old);
    write_handoff(dir, &h).unwrap();
}

#[test]
fn clean_removes_handoffs_and_records_past_the_retention_whether_consumed_or_not() {
    let dir = TempDir::new().unwrap();
    put_days(dir.path(), "old-pending", 31);
    put_days(dir.path(), "old-consumed", 40);
    let mut used = read_handoff(dir.path(), "old-consumed").unwrap();
    used.status = Status::Consumed;
    used.consumed = Some(used.created);
    write_handoff(dir.path(), &used).unwrap();
    put_days(dir.path(), "recent", 29);
    record(dir.path(), "old-record", 31);
    record(dir.path(), "new-record", 1);

    let out = clean(dir.path(), &settings(), false, true, fixed_now());
    assert_eq!(out.code, 0, "{}", out.err);
    let v = json(&out.out);
    assert_eq!(v["removed"].as_array().unwrap().len(), 3, "{v}");
    assert_eq!(v["kept"], 2);
    assert!(!dir.path().join("old-pending.md").exists());
    assert!(!dir.path().join("old-consumed.md").exists());
    assert!(!dir
        .path()
        .join(".sessions")
        .join("old-record.json")
        .exists());
    assert!(dir.path().join("recent.md").exists());
    assert!(dir
        .path()
        .join(".sessions")
        .join("new-record.json")
        .exists());
}

#[test]
fn clean_dry_run_reports_without_removing() {
    let dir = TempDir::new().unwrap();
    put_days(dir.path(), "old", 45);
    let out = clean(dir.path(), &settings(), true, true, fixed_now());
    assert_eq!(json(&out.out)["removed"].as_array().unwrap().len(), 1);
    assert!(dir.path().join("old.md").exists(), "dry run keeps the file");
}

#[test]
fn clean_follows_the_retention_setting() {
    let dir = TempDir::new().unwrap();
    put_days(dir.path(), "week-old", 8);
    let short = Settings {
        handoff_retention_days: 7,
        ..settings()
    };
    clean(dir.path(), &short, false, false, fixed_now());
    assert!(!dir.path().join("week-old.md").exists());
}

#[test]
fn clean_on_a_missing_directory_is_a_quiet_success() {
    let root = TempDir::new().unwrap();
    let out = clean(
        &root.path().join("nope"),
        &settings(),
        false,
        true,
        fixed_now(),
    );
    assert_eq!(out.code, 0);
    assert_eq!(json(&out.out)["removed"], serde_json::json!([]));
}

#[test]
fn the_hook_runs_the_same_cleanup_silently() {
    let dir = TempDir::new().unwrap();
    put_days(dir.path(), "old", 45);
    put_days(dir.path(), "fresh", 1);
    let input = hook_input("new1", CWD, "startup", "/t");
    assert_eq!(
        process(&input, &settings(), dir.path(), None, fixed_now()),
        None
    );
    assert!(!dir.path().join("old.md").exists(), "cleaned by the hook");
    assert!(dir.path().join("fresh.md").exists());
}
