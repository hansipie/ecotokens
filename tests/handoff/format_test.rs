#[path = "common.rs"]
mod common;
use common::*;

use ecotokens::handoff::format::{parse, render};
use ecotokens::handoff::{Handoff, Kind, Status, NOT_PROVIDED};

#[test]
fn render_then_parse_round_trips() {
    let h = sample_handoff("abc123", "/home/u/proj");
    let parsed = parse(&render(&h)).expect("a rendered handoff parses");
    assert_eq!(parsed, h);
}

#[test]
fn sections_appear_in_the_fixed_order() {
    let text = render(&sample_handoff("abc123", "/home/u/proj"));
    assert!(text.starts_with("# Handoff\n"), "{text}");
    let order = [
        "## Header",
        "## Objective",
        "## Problem",
        "## Key files",
        "## Failed attempts",
        "## Next steps",
    ];
    let positions: Vec<usize> = order
        .iter()
        .map(|s| text.find(s).unwrap_or_else(|| panic!("missing {s}")))
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{positions:?}");
}

#[test]
fn header_is_a_key_value_list_with_rfc3339_utc() {
    let text = render(&sample_handoff("abc123", "/home/u/proj"));
    assert!(text.contains("- session: abc123\n"), "{text}");
    assert!(text.contains("- cwd: /home/u/proj\n"), "{text}");
    assert!(text.contains("- branch: feat/x\n"), "{text}");
    assert!(text.contains("- created: 2026-10-04T18:20:11Z\n"), "{text}");
    assert!(text.contains("- status: pending\n"), "{text}");
    assert!(!text.contains("- consumed:"), "{text}");
}

#[test]
fn unfilled_model_fields_render_not_provided_and_parse_back_as_none() {
    let h = Handoff::new("abc123", "/home/u/proj", fixed_now());
    let text = render(&h);
    for section in ["Objective", "Problem", "Next steps"] {
        assert!(
            text.contains(&format!("## {section}\n{NOT_PROVIDED}\n")),
            "{section}: {text}"
        );
    }
    let parsed = parse(&text).unwrap();
    assert_eq!(parsed.objective, None);
    assert_eq!(parsed.problem, None);
    assert_eq!(parsed.next_steps, None);
}

#[test]
fn key_files_and_failed_attempts_use_the_documented_line_grammar() {
    let text = render(&sample_handoff("abc123", "/home/u/proj"));
    assert!(text.contains("- src/install.rs (modified)\n"), "{text}");
    assert!(text.contains("- src/router/hook.rs (read ×3)\n"), "{text}");
    assert!(
        text.contains("- [test_or_build] cargo test: exit 101\n"),
        "{text}"
    );
    assert!(
        text.contains("- [hypothesis] A single matcher cannot cover startup and clear.\n"),
        "{text}"
    );
}

#[test]
fn consumed_status_and_timestamp_round_trip() {
    let mut h = sample_handoff("abc123", "/home/u/proj");
    h.status = Status::Consumed;
    h.consumed = Some(fixed_now() + chrono::Duration::minutes(25));
    let text = render(&h);
    assert!(text.contains("- status: consumed\n"), "{text}");
    assert!(
        text.contains("- consumed: 2026-10-04T18:45:11Z\n"),
        "{text}"
    );
    assert_eq!(parse(&text).unwrap(), h);
}

#[test]
fn branch_is_optional() {
    let mut h = sample_handoff("abc123", "/home/u/proj");
    h.branch = None;
    let text = render(&h);
    assert!(!text.contains("- branch:"), "{text}");
    assert_eq!(parse(&text).unwrap().branch, None);
}

#[test]
fn user_edits_and_unknown_parts_are_preserved() {
    let text =
        "# Handoff\n\n## Header\n- session: abc123\n- cwd: /p\n- created: 2026-10-04T18:20:11Z\n\
- status: pending\n- reviewer: me\n\n## Objective\nDo it.\n\n## Problem\nnot provided\n\n\
## Key files\n- a.rs (modified)\nkeep this note\n\n## Failed attempts\n- [command] x: exit 1\n\
my own remark\n\n## Next steps\nGo.\n\n## Scratch\nsome user text\n";
    let parsed = parse(text).expect("parses");
    let again = render(&parsed);
    assert!(again.contains("- reviewer: me\n"), "{again}");
    assert!(again.contains("keep this note\n"), "{again}");
    assert!(again.contains("my own remark\n"), "{again}");
    assert!(again.contains("## Scratch\nsome user text\n"), "{again}");
    assert_eq!(parse(&again).unwrap(), parsed, "rendering is stable");
}

#[test]
fn failed_attempt_kinds_parse() {
    let mut h = sample_handoff("abc123", "/home/u/proj");
    h.failed_attempts = [
        Kind::TestOrBuild,
        Kind::Command,
        Kind::EditReverted,
        Kind::EditRewritten,
        Kind::Hypothesis,
    ]
    .into_iter()
    .map(|k| ecotokens::handoff::FailedAttempt::new(k, "x"))
    .collect();
    assert_eq!(
        parse(&render(&h)).unwrap().failed_attempts,
        h.failed_attempts
    );
}

#[test]
fn corrupted_files_are_errors_not_panics() {
    let good = render(&sample_handoff("abc123", "/home/u/proj"));
    let cases = [
        ("no title", good.replacen("# Handoff\n", "", 1)),
        ("no session", good.replace("- session: abc123\n", "")),
        ("no cwd", good.replace("- cwd: /home/u/proj\n", "")),
        (
            "no created",
            good.replace("- created: 2026-10-04T18:20:11Z\n", ""),
        ),
        (
            "bad status",
            good.replace("- status: pending", "- status: maybe"),
        ),
        (
            "bad timestamp",
            good.replace("2026-10-04T18:20:11Z", "yesterday"),
        ),
        ("empty", String::new()),
        ("binary-ish", "\u{0}\u{1}\u{2}".to_string()),
    ];
    for (name, text) in cases {
        assert!(parse(&text).is_err(), "{name} should be corrupted");
    }
}
