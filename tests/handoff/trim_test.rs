#[path = "common.rs"]
mod common;
use common::*;

use ecotokens::handoff::format::{render, trim_to_limit};
use ecotokens::handoff::{
    FailedAttempt, Handoff, KeyFile, Kind, HEADER_MAX_CHARS, INJECT_HARD_CAP, MAX_CHARS_CEILING,
};

fn len(h: &Handoff) -> usize {
    render(h).chars().count()
}

fn crowded() -> Handoff {
    let mut h = sample_handoff("abc123", "/home/u/proj");
    h.key_files = (1..=6)
        .map(|i| KeyFile::modified(&format!("src/file_{i}.rs")))
        .collect();
    h.failed_attempts = (1..=6)
        .map(|i| FailedAttempt::new(Kind::Command, &format!("command number {i}: exit 1")))
        .collect();
    h.problem = Some("first line\nsecond line\nthird line\nfourth line".into());
    h
}

// The ceiling leaves room for the injection header under Claude Code's cap.
const _: () = assert!(MAX_CHARS_CEILING + HEADER_MAX_CHARS < INJECT_HARD_CAP);

#[test]
fn a_handoff_that_fits_is_left_alone() {
    let mut h = crowded();
    let before = h.clone();
    let trim = trim_to_limit(&mut h, 10_000);
    assert_eq!(h, before);
    assert!(trim.removed.is_empty());
    assert!(!trim.over_limit);
}

#[test]
fn oldest_failed_attempts_go_first() {
    let mut h = crowded();
    let limit = len(&h) - 10;
    let trim = trim_to_limit(&mut h, limit);
    assert!(len(&h) <= limit);
    assert!(h.failed_attempts.len() < 6, "an attempt was removed");
    assert_eq!(
        h.failed_attempts[0].text, "command number 1: exit 1",
        "newest kept"
    );
    assert_eq!(h.key_files.len(), 6, "key files untouched");
    assert!(h.problem.as_deref().unwrap().contains("fourth line"));
    assert!(!trim.removed.is_empty());
}

#[test]
fn lowest_ranked_key_files_go_second() {
    let mut h = crowded();
    h.failed_attempts.clear();
    let limit = len(&h) - 10;
    trim_to_limit(&mut h, limit);
    assert!(len(&h) <= limit);
    assert!(h.key_files.len() < 6);
    assert_eq!(h.key_files[0].path, "src/file_1.rs", "best ranked kept");
    assert!(h.problem.as_deref().unwrap().contains("fourth line"));
}

#[test]
fn problem_is_cut_last_at_a_line_boundary_with_a_marker() {
    let mut h = crowded();
    h.failed_attempts.clear();
    h.key_files.clear();
    let limit = len(&h) - 10;
    trim_to_limit(&mut h, limit);
    assert!(len(&h) <= limit);
    let problem = h.problem.unwrap();
    assert!(problem.starts_with("first line"), "{problem}");
    assert!(!problem.contains("fourth line"), "{problem}");
    assert!(problem.ends_with('…'), "{problem}");
}

#[test]
fn objective_and_next_steps_are_never_cut() {
    let mut h = crowded();
    h.objective = Some("o".repeat(300));
    h.next_steps = Some("n".repeat(300));
    let trim = trim_to_limit(&mut h, 200);
    assert_eq!(h.objective.as_deref(), Some("o".repeat(300).as_str()));
    assert_eq!(h.next_steps.as_deref(), Some("n".repeat(300).as_str()));
    assert!(
        trim.over_limit,
        "the limit cannot be met without cutting them"
    );
    assert!(h.failed_attempts.is_empty() && h.key_files.is_empty());
}

#[test]
fn length_is_counted_in_characters_and_multibyte_text_is_never_split() {
    let mut h = crowded();
    h.failed_attempts.clear();
    h.key_files.clear();
    h.problem = Some("é".repeat(400) + "\n" + &"😀".repeat(400));
    let limit = len(&h) - 150;
    trim_to_limit(&mut h, limit);
    assert!(len(&h) <= limit, "{} > {limit}", len(&h));
    let text = render(&h);
    assert!(std::str::from_utf8(text.as_bytes()).is_ok());
}

#[test]
fn trimming_is_idempotent() {
    let mut h = crowded();
    let limit = len(&h) - 80;
    trim_to_limit(&mut h, limit);
    let once = h.clone();
    let trim = trim_to_limit(&mut h, limit);
    assert_eq!(h, once);
    assert!(trim.removed.is_empty());
}
