#[path = "common.rs"]
mod common;
use common::*;

use ecotokens::handoff::hook::process;
use ecotokens::handoff::stats::{record, summarize, InjectionRecord};
use ecotokens::handoff::store::write_handoff;
use tempfile::TempDir;

const CWD: &str = "/home/u/proj";

fn rec(id: Option<&str>, chars: usize, stale: bool, listed: bool) -> InjectionRecord {
    InjectionRecord {
        timestamp: fixed_now(),
        handoff_id: id.map(str::to_string),
        chars,
        stale,
        source: "clear".into(),
        listed,
    }
}

#[test]
fn an_empty_store_reads_as_zero() {
    let dir = TempDir::new().unwrap();
    let s = summarize(&dir.path().join("handoff.db")).unwrap();
    assert_eq!(
        (s.count, s.chars_total, s.chars_avg, s.stale, s.listed),
        (0, 0, 0, 0, 0)
    );
}

#[test]
fn records_are_summed_and_averaged() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("handoff.db");
    record(&db, &rec(Some("a"), 1000, false, false)).unwrap();
    record(&db, &rec(Some("b"), 3000, true, false)).unwrap();
    let s = summarize(&db).unwrap();
    assert_eq!(s.count, 2);
    assert_eq!(s.chars_total, 4000);
    assert_eq!(s.chars_avg, 2000);
    assert_eq!(s.stale, 1);
    assert_eq!(s.listed, 0);
}

#[test]
fn a_list_injection_has_no_handoff_id() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("handoff.db");
    record(&db, &rec(None, 400, false, true)).unwrap();
    let s = summarize(&db).unwrap();
    assert_eq!((s.count, s.listed), (1, 1));
}

#[test]
fn the_hook_records_each_injection() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("handoff.db");
    let handoffs = dir.path().join("handoff");
    write_handoff(&handoffs, &sample_handoff("abc123", CWD)).unwrap();
    let input = hook_input("new1", CWD, "clear", "/t");
    let out = process(&input, &settings(), &handoffs, Some(&db), fixed_now()).unwrap();
    let s = summarize(&db).unwrap();
    assert_eq!(s.count, 1);
    assert!(s.chars_total > 0 && s.chars_total <= out.chars().count() as u64);
}

#[test]
fn a_failing_statistics_write_never_blocks_the_injection() {
    let dir = TempDir::new().unwrap();
    let handoffs = dir.path().join("handoff");
    write_handoff(&handoffs, &sample_handoff("abc123", CWD)).unwrap();
    // A directory cannot be opened as a database file.
    let broken = dir.path().join("a-directory");
    std::fs::create_dir(&broken).unwrap();
    let input = hook_input("new1", CWD, "clear", "/t");
    assert!(process(&input, &settings(), &handoffs, Some(&broken), fixed_now()).is_some());
}
