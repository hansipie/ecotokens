#[path = "common.rs"]
mod common;
use common::*;

use ecotokens::config::Settings;
use ecotokens::handoff::hook::process;
use ecotokens::handoff::store::{read_session_record, write_handoff};
use ecotokens::handoff::{HEADER_MAX_CHARS, INJECT_HARD_CAP};
use tempfile::TempDir;

const CWD: &str = "/home/u/proj";

fn run(dir: &std::path::Path, settings: &Settings, source: &str) -> Option<String> {
    let input = hook_input("new1", CWD, source, "/tmp/new1.jsonl");
    process(&input, settings, dir, None, fixed_now())
}

fn context(out: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(out).expect("output is one JSON object");
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "SessionStart");
    v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext is a string")
        .to_string()
}

#[test]
fn clear_with_exactly_one_pending_handoff_injects_it() {
    let dir = TempDir::new().unwrap();
    write_handoff(dir.path(), &sample_handoff("abc123", CWD)).unwrap();
    let out = run(dir.path(), &settings(), "clear").expect("something is injected");
    let ctx = context(&out);
    assert!(ctx.starts_with("[ecotokens handoff]"), "{ctx}");
    assert!(ctx.contains("not an instruction"), "{ctx}");
    assert!(ctx.contains("# Handoff"), "{ctx}");
    assert!(ctx.contains("Add a manual handoff command."), "{ctx}");
    assert!(ctx.contains("cargo test: exit 101"), "{ctx}");
    assert!(
        ctx.contains("Write the failing tests for the store."),
        "{ctx}"
    );
    assert!(!out.contains('\n'), "the hook prints a single line of JSON");
}

#[test]
fn no_handoff_means_no_output() {
    let dir = TempDir::new().unwrap();
    assert_eq!(run(dir.path(), &settings(), "clear"), None);
}

#[test]
fn a_handoff_for_another_directory_is_not_injected() {
    let dir = TempDir::new().unwrap();
    write_handoff(dir.path(), &sample_handoff("abc123", "/home/u/elsewhere")).unwrap();
    assert_eq!(run(dir.path(), &settings(), "clear"), None);
}

#[test]
fn every_start_records_the_session() {
    for source in ["startup", "resume", "clear", "compact", "fork"] {
        let dir = TempDir::new().unwrap();
        run(dir.path(), &settings(), source);
        let rec = read_session_record(dir.path(), "new1")
            .unwrap_or_else(|| panic!("no record for {source}"));
        assert_eq!(rec.source, source);
        assert_eq!(rec.cwd, CWD);
        assert_eq!(rec.transcript_path, "/tmp/new1.jsonl");
        assert_eq!(rec.recorded, fixed_now());
    }
}

#[test]
fn bad_input_gives_no_output_and_no_panic() {
    let dir = TempDir::new().unwrap();
    for input in [
        "",
        "not json",
        "{}",
        "[]",
        r#"{"session_id":"../x","cwd":"/p","source":"clear"}"#,
        r#"{"session_id":"ok","source":"clear"}"#,
        r#"{"session_id":"ok","cwd":"/p"}"#,
        r#"{"session_id":42,"cwd":"/p","source":"clear"}"#,
    ] {
        assert_eq!(
            process(input, &settings(), dir.path(), None, fixed_now()),
            None,
            "{input}"
        );
    }
    assert!(
        !dir.path().join(".sessions").join("x.json").exists(),
        "a hostile id never reaches the file system"
    );
}

#[test]
fn a_disabled_feature_does_nothing_at_all() {
    let root = TempDir::new().unwrap();
    let dir = root.path().join("handoff");
    let off = Settings::default();
    assert_eq!(run(&dir, &off, "clear"), None);
    assert!(!dir.exists(), "nothing was written");
}

#[test]
fn an_unusable_directory_fails_open() {
    let root = TempDir::new().unwrap();
    let not_a_dir = root.path().join("file");
    std::fs::write(&not_a_dir, "x").unwrap();
    assert_eq!(run(&not_a_dir, &settings(), "clear"), None);
}

#[test]
fn a_corrupted_file_is_skipped_and_the_valid_one_still_injects() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("bad.md"), "garbage").unwrap();
    assert_eq!(
        run(dir.path(), &settings(), "clear"),
        None,
        "only a corrupted file"
    );
    write_handoff(dir.path(), &sample_handoff("abc123", CWD)).unwrap();
    let out = run(dir.path(), &settings(), "clear").expect("the valid one is injected");
    assert!(context(&out).contains("Add a manual handoff command."));
}

#[test]
fn an_oversized_file_cannot_push_the_injection_past_the_platform_cap() {
    let dir = TempDir::new().unwrap();
    let mut h = sample_handoff("abc123", CWD);
    h.problem = Some(
        (0..600)
            .map(|i| format!("line {i} {}", "x".repeat(40)))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    write_handoff(dir.path(), &h).unwrap();
    let big = Settings {
        handoff_max_chars: 9000,
        ..settings()
    };
    let ctx = context(&run(dir.path(), &big, "clear").unwrap());
    assert!(
        ctx.chars().count() <= INJECT_HARD_CAP,
        "{}",
        ctx.chars().count()
    );
    let header_len = ctx.find("# Handoff").expect("handoff body present");
    assert!(
        header_len <= HEADER_MAX_CHARS,
        "header is {header_len} chars"
    );
    assert!(
        ctx.contains("Add a manual handoff command."),
        "objective survives trimming"
    );
}
