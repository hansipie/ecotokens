#[path = "common.rs"]
mod common;
use common::*;

use ecotokens::config::Settings;
use ecotokens::handoff::cli::write;
use ecotokens::handoff::hook::process;
use ecotokens::handoff::store::{
    list_entries, read_handoff, write_handoff, write_session_record, EntryState,
};
use ecotokens::handoff::{SessionRecord, Status, HEADER_MAX_CHARS};
use tempfile::TempDir;

const CWD: &str = "/home/u/proj";

fn run(dir: &std::path::Path, settings: &Settings, cwd: &str) -> Option<String> {
    let input = hook_input("new1", cwd, "clear", "/tmp/new1.jsonl");
    process(&input, settings, dir, None, fixed_now())
}

fn context(out: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(out).unwrap();
    v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .to_string()
}

fn put(dir: &std::path::Path, id: &str, cwd: &str, hours_old: i64) {
    let mut h = sample_handoff(id, cwd);
    h.created = hours_before(fixed_now(), hours_old);
    h.objective = Some(format!("objective of {id}"));
    write_handoff(dir, &h).unwrap();
}

#[test]
fn two_sessions_in_one_directory_each_keep_their_own_file() {
    let root = TempDir::new().unwrap();
    let dir = root.path().join("handoff");
    for id in ["s1", "s2"] {
        let transcript = TranscriptBuilder::new(id, CWD, None)
            .edit(&format!("/home/u/proj/{id}.rs"), "a", "b")
            .build(root.path());
        write_session_record(
            &dir,
            &SessionRecord {
                session_id: id.into(),
                transcript_path: transcript.to_string_lossy().into_owned(),
                cwd: CWD.into(),
                source: "startup".into(),
                recorded: fixed_now(),
            },
        )
        .unwrap();
        assert_eq!(
            write(&dir, &settings(), id, Some(CWD), false, fixed_now()).code,
            0
        );
    }
    let one = read_handoff(&dir, "s1").unwrap();
    let two = read_handoff(&dir, "s2").unwrap();
    assert_eq!(one.key_files[0].path, "s1.rs");
    assert_eq!(two.key_files[0].path, "s2.rs");
}

#[test]
fn a_handoff_for_another_directory_is_ignored_next_to_a_matching_one() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "mine", CWD, 1);
    put(dir.path(), "theirs", "/home/u/elsewhere", 1);
    let ctx = context(&run(dir.path(), &settings(), CWD).expect("the matching one"));
    assert!(ctx.contains("objective of mine"), "{ctx}");
    assert!(!ctx.contains("objective of theirs"), "{ctx}");
}

#[test]
fn a_trailing_slash_does_not_break_the_match() {
    // Each lookup gets its own directory: the first injection consumes the file.
    for (written, asked) in [
        ("/home/u/proj/", "/home/u/proj"),
        ("/home/u/proj", "/home/u/proj/"),
    ] {
        let dir = TempDir::new().unwrap();
        put(dir.path(), "abc123", written, 1);
        assert!(
            run(dir.path(), &settings(), asked).is_some(),
            "{written} vs {asked}"
        );
    }
}

#[test]
fn a_branch_change_does_not_lose_the_handoff_and_the_header_keeps_the_original_branch() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "abc123", CWD, 1); // written on feat/x
    let ctx = context(&run(dir.path(), &settings(), CWD).unwrap());
    assert!(ctx.contains("on branch feat/x"), "{ctx}");
}

#[test]
fn injecting_the_only_pending_handoff_consumes_it_once() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "abc123", CWD, 1);
    assert!(run(dir.path(), &settings(), CWD).is_some());
    let h = read_handoff(dir.path(), "abc123").unwrap();
    assert_eq!(h.status, Status::Consumed);
    assert_eq!(h.consumed, Some(fixed_now()));
    assert_eq!(
        run(dir.path(), &settings(), CWD),
        None,
        "not injected a second time"
    );
    let text = std::fs::read_to_string(dir.path().join("abc123.md")).unwrap();
    assert!(text.contains("- status: consumed"), "{text}");
    assert!(
        text.contains("objective of abc123"),
        "content kept for explicit loading"
    );
}

#[test]
fn several_pending_handoffs_give_only_a_short_list_and_consume_nothing() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "aaa111", CWD, 2);
    put(dir.path(), "bbb222", CWD, 26);
    let ctx = context(&run(dir.path(), &settings(), CWD).expect("a list"));
    assert!(ctx.contains("2 saved states"), "{ctx}");
    assert!(ctx.contains("/handoff-load"), "{ctx}");
    assert!(ctx.contains("aaa111") && ctx.contains("bbb222"), "{ctx}");
    assert!(ctx.contains("objective of aaa111"), "{ctx}");
    assert!(
        ctx.contains("(stale)"),
        "the 26 hour old one is flagged: {ctx}"
    );
    assert!(
        !ctx.contains("## Problem"),
        "no handoff body is loaded: {ctx}"
    );
    for id in ["aaa111", "bbb222"] {
        assert_eq!(
            read_handoff(dir.path(), id).unwrap().status,
            Status::Pending
        );
    }
}

#[test]
fn the_short_list_is_capped() {
    let dir = TempDir::new().unwrap();
    for i in 0..12 {
        put(dir.path(), &format!("id{i:02}"), CWD, 1 + i);
    }
    let ctx = context(&run(dir.path(), &settings(), CWD).unwrap());
    assert!(ctx.contains("12 saved states"), "{ctx}");
    assert!(ctx.contains("2 more"), "{ctx}");
    assert!(ctx.chars().count() < 2_000, "{}", ctx.chars().count());
}

#[test]
fn an_old_handoff_is_injected_with_a_stale_marker_giving_its_age() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "abc123", CWD, 30);
    let ctx = context(&run(dir.path(), &settings(), CWD).unwrap());
    assert!(ctx.contains("30 hours"), "{ctx}");
    assert!(ctx.contains("verify it before relying on it"), "{ctx}");
    let header = &ctx[..ctx.find("# Handoff").unwrap()];
    assert!(header.chars().count() <= HEADER_MAX_CHARS, "{header}");
}

#[test]
fn a_fresh_handoff_carries_no_stale_marker() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "abc123", CWD, 3);
    let ctx = context(&run(dir.path(), &settings(), CWD).unwrap());
    assert!(ctx.contains("3 hours"), "{ctx}");
    assert!(!ctx.contains("verify it before relying on it"), "{ctx}");
}

#[test]
fn the_stale_threshold_follows_the_setting() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "abc123", CWD, 3);
    let strict = Settings {
        handoff_stale_hours: 2,
        ..settings()
    };
    let ctx = context(&run(dir.path(), &strict, CWD).unwrap());
    assert!(ctx.contains("verify it before relying on it"), "{ctx}");
}

#[test]
fn a_corrupted_file_does_not_hide_the_others() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("broken.md"), "# Handoff\nnothing else").unwrap();
    put(dir.path(), "abc123", CWD, 1);
    assert!(run(dir.path(), &settings(), CWD).is_some());
}

#[test]
fn entries_are_classified_pending_consumed_corrupted_and_stale() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "fresh", CWD, 1);
    put(dir.path(), "old", CWD, 50);
    put(dir.path(), "used", CWD, 1);
    std::fs::write(dir.path().join("broken.md"), "garbage").unwrap();
    // Mark one consumed through a hook run on a directory holding only it.
    let mut used = read_handoff(dir.path(), "used").unwrap();
    used.status = Status::Consumed;
    used.consumed = Some(fixed_now());
    write_handoff(dir.path(), &used).unwrap();

    let entries = list_entries(dir.path(), Some(CWD), 24, fixed_now());
    let state = |id: &str| {
        entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| (e.state, e.stale))
    };
    assert_eq!(state("fresh"), Some((EntryState::Pending, false)));
    assert_eq!(state("old"), Some((EntryState::Pending, true)));
    assert_eq!(state("used"), Some((EntryState::Consumed, false)));
    assert_eq!(state("broken"), Some((EntryState::Corrupted, false)));
    let order: Vec<&str> = entries
        .iter()
        .filter(|e| e.state != EntryState::Corrupted)
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(order, ["fresh", "used", "old"], "newest first");
}

#[test]
fn entries_can_be_limited_to_a_directory_or_not() {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "here", CWD, 1);
    put(dir.path(), "there", "/home/u/elsewhere", 1);
    let ids = |cwd: Option<&str>| -> Vec<String> {
        list_entries(dir.path(), cwd, 24, fixed_now())
            .into_iter()
            .map(|e| e.id)
            .collect()
    };
    assert_eq!(ids(Some(CWD)), ["here"]);
    assert_eq!(ids(None).len(), 2);
}
