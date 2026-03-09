use ecotokens::filter::generic::{filter_generic, force_filter_generic, THRESHOLD_LINES};

#[test]
fn short_output_passes_through() {
    let input = "line1\nline2\nline3\n";
    let out = filter_generic(input, 500, 51200);
    assert_eq!(out, input);
}

#[test]
fn output_above_line_threshold_is_summarized() {
    let lines: Vec<String> = (1..=600).map(|i| format!("line {i}")).collect();
    let input = lines.join("\n");
    let out = filter_generic(&input, 500, 51200);
    assert!(out.len() < input.len(), "output should be shorter");
    assert!(out.contains("[ecotokens]"), "should contain summary marker");
}

#[test]
fn output_above_byte_threshold_is_summarized() {
    let input = "x".repeat(53248);
    let out = filter_generic(&input, 500, 51200);
    assert!(out.len() < input.len());
    assert!(out.contains("[ecotokens]"));
}

#[test]
fn summary_preserves_first_and_last_lines() {
    let lines: Vec<String> = (1..=600).map(|i| format!("line {i}")).collect();
    let input = lines.join("\n");
    let out = filter_generic(&input, 500, 51200);
    assert!(out.contains("line 1"), "first lines should be present");
    assert!(out.contains("line 600"), "last lines should be present");
}

#[test]
fn passthrough_threshold_is_respected() {
    let lines: Vec<String> = (1..=499).map(|i| format!("line {i}")).collect();
    let input = lines.join("\n");
    let out = filter_generic(&input, 500, 51200);
    assert_eq!(out, input, "under threshold should pass through unchanged");
}

#[test]
fn threshold_lines_constant_is_500() {
    assert_eq!(THRESHOLD_LINES, 500);
}

#[test]
fn force_filter_always_reduces_non_empty_output() {
    let input = "abcdefghijklmnopqrstuvwxyz";
    let out = force_filter_generic(input);
    assert!(out.len() < input.len());
}

#[test]
fn force_filter_reduces_estimated_tokens() {
    let input = "x".repeat(20);
    let before = ecotokens::tokens::estimate_tokens(&input);
    let out = force_filter_generic(&input);
    let after = ecotokens::tokens::estimate_tokens(&out);
    assert!(after < before, "after={after}, before={before}");
}
