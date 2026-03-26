use ecotokens::hook::grep_handler::handle_grep;
use ecotokens::hook::post_handler::PostFilterResult;

fn make_grep_output(n_lines: usize) -> String {
    (0..n_lines)
        .map(|i| format!("src/foo.rs:{}:    some matching line here", i + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn grep_short_output_passthrough() {
    // ≤ 30 lines → Passthrough (no compaction needed)
    let output = make_grep_output(10);
    let result = handle_grep(&output, 1);
    assert!(
        matches!(result, PostFilterResult::Passthrough),
        "short grep output (10 lines) should passthrough"
    );
}

#[test]
fn grep_empty_output_passthrough() {
    let result = handle_grep("", 1);
    assert!(
        matches!(result, PostFilterResult::Passthrough),
        "empty grep output should passthrough"
    );
}

#[test]
fn grep_long_output_filtered() {
    // > 30 lines → Filtered with compacted output
    let output = make_grep_output(51);
    let result = handle_grep(&output, 1);
    // With stub this will FAIL (RED) — stub returns Passthrough
    assert!(
        matches!(result, PostFilterResult::Filtered { .. }),
        "long grep output (51 lines) should be Filtered, got Passthrough (expected RED with stub)"
    );
}

#[test]
fn grep_exactly_30_lines_passthrough() {
    let output = make_grep_output(30);
    let result = handle_grep(&output, 1);
    assert!(
        matches!(result, PostFilterResult::Passthrough),
        "exactly 30 lines should passthrough"
    );
}

#[test]
fn grep_31_lines_filtered() {
    let output = make_grep_output(31);
    let result = handle_grep(&output, 1);
    // RED with stub
    assert!(
        matches!(result, PostFilterResult::Filtered { .. }),
        "31 lines should be Filtered"
    );
}
