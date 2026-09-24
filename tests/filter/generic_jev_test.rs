use std::collections::HashMap;

use ecotokens::config::Settings;
use ecotokens::filter::generic::{filter_generic, filter_generic_with_judge};
use ecotokens::jev::{Answer, Answers, ChoiceAnswer, JevContext, JevError, StubJudge};

fn build_log(lines: usize, error_at: usize) -> String {
    (0..lines)
        .map(|i| {
            if i == error_at {
                "error: connection refused while uploading artifact build-4711".to_string()
            } else {
                format!("step {i}: ok")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Answers for `windows` windows of 200 lines starting at line 20, flagging
/// `flagged` (global line index) in its window.
fn answers_flagging(windows: usize, flagged: usize) -> Answers {
    let mut a = Answers::default();
    for k in 0..windows {
        let window_start = 20 + k * 200;
        let hit = (window_start..window_start + 200).contains(&flagged);
        let id = format!("L{flagged:05}");
        let probabilities = if hit {
            HashMap::from([(id.clone(), 0.9)])
        } else {
            HashMap::new()
        };
        a.insert(
            format!("pick_{k}"),
            Answer::Choice(ChoiceAnswer {
                choice: if hit {
                    id
                } else {
                    format!("L{window_start:05}")
                },
                probabilities,
                confidence: 0.9,
            }),
        );
        a.insert(
            format!("any_{k}"),
            Answer::Noul(if hit { 0.95 } else { 0.05 }),
        );
    }
    a
}

#[test]
fn mid_log_error_survives_head_tail_truncation() {
    let log = build_log(600, 300);
    let plain = filter_generic(&log, 200, 51200);
    assert!(!plain.contains("connection refused"), "precondition");

    let s = Settings::default();
    let judge = StubJudge::with_answers(answers_flagging(3, 300));
    let out = filter_generic_with_judge(&log, 200, 51200, JevContext::new(&judge, &s));

    assert!(out.contains("error: connection refused while uploading artifact build-4711"));
    assert!(out.starts_with("step 0: ok"));
    assert!(out.ends_with("step 599: ok"));
    assert!(out.contains("kept by Jev"));
    assert!(out.lines().count() < 60, "output not reduced:\n{out}");
    assert_eq!(judge.call_count(), 1, "all windows go in one request");
}

#[test]
fn state_lines_carry_ids_and_questions_cover_every_window() {
    let log = build_log(600, 300);
    let s = Settings::default();
    let judge = StubJudge::with_answers(answers_flagging(3, 300));
    filter_generic_with_judge(&log, 200, 51200, JevContext::new(&judge, &s));
    let (state, questions) = judge.last_request().unwrap();
    assert!(state["window_1"]
        .as_str()
        .unwrap()
        .contains("L00300 error: connection refused"));
    for k in 0..3 {
        assert!(questions.contains_key(&format!("pick_{k}")));
        assert!(questions.contains_key(&format!("any_{k}")));
    }
}

#[test]
fn falls_back_byte_identically_when_jev_cannot_help() {
    let log = build_log(600, 300);
    let s = Settings::default();
    let expected = filter_generic(&log, 200, 51200);

    let failing = StubJudge::failing(JevError::Timeout);
    assert_eq!(
        filter_generic_with_judge(&log, 200, 51200, JevContext::new(&failing, &s)),
        expected
    );

    let missing = StubJudge::with_answers(Answers::default());
    assert_eq!(
        filter_generic_with_judge(&log, 200, 51200, JevContext::new(&missing, &s)),
        expected
    );

    // Nothing flagged: same as today.
    let quiet = StubJudge::with_answers(answers_flagging(3, usize::MAX));
    assert_eq!(
        filter_generic_with_judge(&log, 200, 51200, JevContext::new(&quiet, &s)),
        expected
    );
}

#[test]
fn small_or_oversized_outputs_skip_jev() {
    let s = Settings::default();
    let judge = StubJudge::with_answers(Answers::default());
    let ctx = JevContext::new(&judge, &s);

    let small = build_log(50, 25);
    assert_eq!(filter_generic_with_judge(&small, 200, 51200, ctx), small);

    // More than 10 windows of 200 lines.
    let huge = build_log(2_100, 1_000);
    assert_eq!(
        filter_generic_with_judge(&huge, 200, 1_000_000, ctx),
        filter_generic(&huge, 200, 1_000_000)
    );
    assert_eq!(judge.call_count(), 0);
}
