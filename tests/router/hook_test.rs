#[path = "common.rs"]
mod common;
use common::{answers, router_settings};

use ecotokens::config::Settings;
use ecotokens::jev::{JevError, StubJudge};
use ecotokens::router::hook::{is_jev_down, mark_jev_down, process};
use ecotokens::router::Decision;
use tempfile::TempDir;

#[test]
fn delegated_message_injects_user_prompt_submit_context() {
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("down");
    let judge = StubJudge::with_answers(answers("tiny", 0.97, 0.02));
    let (routing, out) = process(
        "capital of Australia?",
        &router_settings(),
        Some(&judge),
        &marker,
    );
    assert_eq!(routing.unwrap().decision, Decision::Delegated);
    let v: serde_json::Value = serde_json::from_str(&out.expect("output")).unwrap();
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "UserPromptSubmit");
    let ctx = v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(ctx.contains("router-tiny"), "{ctx}");
}

#[test]
fn router_off_does_nothing() {
    let dir = TempDir::new().unwrap();
    let judge = StubJudge::with_answers(answers("tiny", 1.0, 0.0));
    let (routing, out) = process(
        "hi",
        &Settings::default(),
        Some(&judge),
        &dir.path().join("m"),
    );
    assert!(routing.is_none() && out.is_none());
    assert_eq!(judge.call_count(), 0);
}

#[test]
fn no_judge_does_nothing() {
    let dir = TempDir::new().unwrap();
    let (routing, out) = process("hi", &router_settings(), None, &dir.path().join("m"));
    assert!(routing.is_none() && out.is_none());
}

#[test]
fn unsure_or_follow_up_prints_nothing_but_is_recorded() {
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("m");
    let judge = StubJudge::with_answers(answers("large", 0.4, 0.1));
    let (routing, out) = process("hmm", &router_settings(), Some(&judge), &marker);
    assert_eq!(routing.unwrap().decision, Decision::SelfUnsure);
    assert!(out.is_none());
}

#[test]
fn service_failure_prints_nothing_and_pauses_jev() {
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("down");
    let judge = StubJudge::failing(JevError::Timeout);
    let (routing, out) = process("write a post", &router_settings(), Some(&judge), &marker);
    assert_eq!(routing.unwrap().decision, Decision::Error);
    assert!(out.is_none());
    assert!(is_jev_down(&marker));

    // Next message: Jev is not asked at all.
    let (routing, out) = process("write a post", &router_settings(), Some(&judge), &marker);
    assert_eq!(routing.unwrap().decision, Decision::JevDown);
    assert!(out.is_none());
    assert_eq!(judge.call_count(), 1);
}

#[test]
fn request_errors_do_not_pause_jev() {
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("down");
    let judge = StubJudge::failing(JevError::BadResponse("x".into()));
    let _ = process("write a post", &router_settings(), Some(&judge), &marker);
    assert!(!is_jev_down(&marker));
}

#[test]
fn stale_marker_is_ignored() {
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("down");
    std::fs::write(&marker, "1000").unwrap();
    assert!(!is_jev_down(&marker));
    mark_jev_down(&marker);
    assert!(is_jev_down(&marker));
    std::fs::write(&marker, "garbage").unwrap();
    assert!(!is_jev_down(&marker));
}
