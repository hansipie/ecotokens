use ecotokens::rewrite::sanitize::cleanup_response;

#[test]
fn strips_a_heres_preamble() {
    let response = "Here's the rewritten text:\nActual content follows here.";
    let out = cleanup_response(response, false);
    assert_eq!(out, "Actual content follows here.");
}

#[test]
fn strips_a_sure_preamble() {
    let response = "Sure! Here is the content you asked for.\nThe real body text.";
    let out = cleanup_response(response, false);
    assert_eq!(out, "The real body text.");
}

#[test]
fn strips_a_certainly_preamble() {
    let response = "Certainly, here you go:\nRewritten body.";
    let out = cleanup_response(response, false);
    assert_eq!(out, "Rewritten body.");
}

#[test]
fn strips_a_below_is_preamble() {
    let response = "Below is the rewritten passage:\nThe passage text.";
    let out = cleanup_response(response, false);
    assert_eq!(out, "The passage text.");
}

#[test]
fn does_not_strip_a_line_that_only_superficially_resembles_a_preamble() {
    let response = "The report on hers is finished.\nMore content.";
    let out = cleanup_response(response, false);
    assert!(out.contains("The report on hers is finished."));
}

#[test]
fn strips_whole_response_fence_when_absent_from_input() {
    let response = "```\nThe transformed text.\n```";
    let out = cleanup_response(response, false);
    assert_eq!(out, "The transformed text.");
}

#[test]
fn strips_whole_response_fence_with_language_tag() {
    let response = "```markdown\nThe transformed text.\n```";
    let out = cleanup_response(response, false);
    assert_eq!(out, "The transformed text.");
}

#[test]
fn keeps_fence_when_input_already_had_one() {
    let response = "```\ncode block content\n```";
    let out = cleanup_response(response, true);
    assert_eq!(out, response);
}

#[test]
fn strips_trailing_commentary() {
    let response = "The rewritten text.\nI've simplified the wording for clarity.";
    let out = cleanup_response(response, false);
    assert_eq!(out, "The rewritten text.");
}

#[test]
fn trims_surrounding_whitespace() {
    let response = "  \n  The text.  \n\n";
    let out = cleanup_response(response, false);
    assert_eq!(out, "The text.");
}
