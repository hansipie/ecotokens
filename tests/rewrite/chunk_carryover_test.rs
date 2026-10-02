use ecotokens::rewrite::chunk::{carry_over_anchor, strip_echoed_anchor};

#[test]
fn anchor_is_the_whole_text_when_under_the_token_budget() {
    let transformed = "A short transformed chunk.";
    assert_eq!(carry_over_anchor(transformed), transformed);
}

#[test]
fn anchor_is_the_tail_when_over_the_token_budget() {
    let long_text = (0..500)
        .map(|i| format!("word{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let anchor = carry_over_anchor(&long_text);
    assert!(anchor.len() < long_text.len());
    // The anchor must be a suffix of the transformed text (whole-word
    // aligned), not the head.
    assert!(long_text.ends_with(anchor.as_str()));
    assert!(
        anchor.contains("word499"),
        "anchor must be the tail, not the head"
    );
    assert!(
        !anchor.contains("word0 "),
        "anchor must not include the very start"
    );
}

#[test]
fn anchor_uses_the_transformed_text_not_the_source() {
    // Caller responsibility, but assert the function has no notion of
    // "source" at all — it operates purely on whatever string it's given.
    let transformed_tail = "This is the transformed style to carry forward.";
    let anchor = carry_over_anchor(transformed_tail);
    assert_eq!(anchor, transformed_tail);
}

#[test]
fn echoed_anchor_is_stripped_from_the_start_of_output() {
    let anchor = "the established style continues here";
    let model_output = format!("{anchor}\nAnd now the actually new transformed content.");
    let stripped = strip_echoed_anchor(&model_output, anchor);
    assert_eq!(stripped, "And now the actually new transformed content.");
}

#[test]
fn echoed_anchor_with_leading_whitespace_is_still_stripped() {
    let anchor = "style anchor text";
    let model_output = format!("   {anchor}   \nNew content follows.");
    let stripped = strip_echoed_anchor(&model_output, anchor);
    assert_eq!(stripped, "New content follows.");
}

#[test]
fn output_without_echo_is_unchanged() {
    let anchor = "style anchor text";
    let model_output = "Completely new content with no repetition.";
    assert_eq!(strip_echoed_anchor(model_output, anchor), model_output);
}

#[test]
fn empty_anchor_never_strips_anything() {
    let model_output = "Some output.";
    assert_eq!(strip_echoed_anchor(model_output, ""), model_output);
    assert_eq!(strip_echoed_anchor(model_output, "   "), model_output);
}
