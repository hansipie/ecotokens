#[path = "common.rs"]
mod common;
use common::*;

use ecotokens::config::Settings;
use ecotokens::handoff::hook::process;
use ecotokens::handoff::store::{read_session_record, write_handoff};
use tempfile::TempDir;

const CWD: &str = "/home/u/proj";

fn run(dir: &std::path::Path, settings: &Settings, source: &str) -> Option<String> {
    let input = hook_input("new1", CWD, source, "/tmp/new1.jsonl");
    process(&input, settings, dir, None, fixed_now())
}

fn dir_with_handoff() -> TempDir {
    let dir = TempDir::new().unwrap();
    write_handoff(dir.path(), &sample_handoff("abc123", CWD)).unwrap();
    dir
}

#[test]
fn a_compaction_re_injects_the_pending_handoff() {
    let dir = dir_with_handoff();
    let out = run(dir.path(), &settings(), "compact").expect("injected after compaction");
    assert!(out.contains("Add a manual handoff command."), "{out}");
    assert!(out.contains("SessionStart"), "{out}");
}

#[test]
fn a_compaction_without_a_handoff_adds_nothing_and_raises_nothing() {
    let dir = TempDir::new().unwrap();
    assert_eq!(run(dir.path(), &settings(), "compact"), None);
}

#[test]
fn other_sources_record_but_do_not_inject_by_default() {
    for source in ["startup", "resume", "fork"] {
        let dir = dir_with_handoff();
        assert_eq!(run(dir.path(), &settings(), source), None, "{source}");
        assert!(
            read_session_record(dir.path(), "new1").is_some(),
            "{source} still records the session"
        );
    }
}

#[test]
fn the_opt_in_setting_extends_injection_to_the_other_sources() {
    let on = Settings {
        handoff_inject_startup: true,
        ..settings()
    };
    for source in ["startup", "resume", "fork"] {
        let dir = dir_with_handoff();
        let out = run(dir.path(), &on, source);
        assert!(out.is_some(), "{source} injects when opted in");
    }
}

#[test]
fn an_unknown_source_never_injects_even_when_opted_in() {
    let on = Settings {
        handoff_inject_startup: true,
        ..settings()
    };
    let dir = dir_with_handoff();
    assert_eq!(run(dir.path(), &on, "teleport"), None);
}

#[test]
fn clear_and_compact_do_not_depend_on_the_opt_in() {
    let off = Settings {
        handoff_inject_startup: false,
        ..settings()
    };
    for source in ["clear", "compact"] {
        let dir = dir_with_handoff();
        assert!(run(dir.path(), &off, source).is_some(), "{source}");
    }
}

#[test]
fn the_output_is_valid_alongside_another_hooks_additional_context() {
    // Claude Code runs matching hooks in parallel and receives every
    // `additionalContext`; ours must therefore be a complete, standalone JSON
    // object with nothing else on stdout.
    let dir = dir_with_handoff();
    let out = run(dir.path(), &settings(), "compact").unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 1, "only hookSpecificOutput");
    assert_eq!(v["hookSpecificOutput"].as_object().unwrap().len(), 2);
}
