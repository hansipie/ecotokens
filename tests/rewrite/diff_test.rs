use ecotokens::rewrite::diff::{build_diff, DiffMetadata};

fn meta<'a>(mode: &'a str, target: Option<&'a str>, model: &'a str) -> DiffMetadata<'a> {
    DiffMetadata {
        mode,
        target,
        model,
        chunk_count: 1,
        outcome: "transformed",
    }
}

#[test]
fn header_contains_all_metadata_fields() {
    let out = build_diff(
        "old text",
        "new text",
        &meta("tone", Some("plain"), "llama3.2:3b"),
    );
    assert!(out.contains("mode: tone"), "got: {out}");
    assert!(out.contains("target: plain"), "got: {out}");
    assert!(out.contains("model: llama3.2:3b"), "got: {out}");
    assert!(out.contains("chunk_count: 1"), "got: {out}");
    assert!(out.contains("outcome: transformed"), "got: {out}");
    assert!(out.contains("timestamp:"), "got: {out}");
}

#[test]
fn header_omits_target_gracefully_when_absent() {
    let out = build_diff(
        "old text",
        "new text",
        &meta("paraphrase", None, "llama3.2:3b"),
    );
    assert!(out.contains("mode: paraphrase"), "got: {out}");
    // Must not panic/crash and must still be well-formed with an empty or
    // absent target value.
    assert!(out.contains("target:"), "got: {out}");
}

#[test]
fn body_is_a_unified_diff() {
    let original = "line one\nline two\nline three\n";
    let transformed = "line one\nline TWO\nline three\n";
    let out = build_diff(original, transformed, &meta("paraphrase", None, "m"));
    assert!(out.contains("-line two"), "got: {out}");
    assert!(out.contains("+line TWO"), "got: {out}");
}

#[test]
fn unified_diff_uses_three_lines_of_context() {
    // 10 identical lines, then one changed line in the middle, then 10 more
    // identical lines — with a 3-line context radius the hunk must include
    // exactly 3 unchanged lines before and after the change, not all 10.
    let mut original_lines: Vec<String> = (0..10).map(|i| format!("context {i}")).collect();
    original_lines.push("the original line".to_string());
    original_lines.extend((10..20).map(|i| format!("context {i}")));
    let original = original_lines.join("\n") + "\n";

    let mut transformed_lines: Vec<String> = (0..10).map(|i| format!("context {i}")).collect();
    transformed_lines.push("the changed line".to_string());
    transformed_lines.extend((10..20).map(|i| format!("context {i}")));
    let transformed = transformed_lines.join("\n") + "\n";

    let out = build_diff(&original, &transformed, &meta("paraphrase", None, "m"));

    // Lines immediately adjacent to the change must be present as context...
    assert!(out.contains("context 9"), "got: {out}");
    assert!(out.contains("context 10"), "got: {out}");
    // ...but lines far from the change must be excluded from the hunk.
    assert!(!out.contains("context 0\n"), "got: {out}");
    assert!(!out.contains("context 19\n"), "got: {out}");
}

#[test]
fn secrets_are_masked_on_both_sides() {
    // Synthetic AWS-access-key-shaped secret (matches src/masking's AKIA
    // pattern) present on both the original and transformed side.
    let secret = "AKIAABCD1234EFGH5678";
    let original = format!("Please use key {secret} for access.");
    let transformed = format!("Kindly use key {secret} for access.");
    let out = build_diff(&original, &transformed, &meta("paraphrase", None, "m"));
    assert!(!out.contains(secret), "raw secret leaked into diff: {out}");
    assert!(out.contains("[AWS_KEY]"), "got: {out}");
}
