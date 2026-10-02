use ecotokens::rewrite::sanitize::{detect_failure, extract, restore};

#[test]
fn empty_response_is_a_failure() {
    let reason = detect_failure(100, "", 0.5);
    assert!(reason.is_some());
    assert!(reason.unwrap().contains("empty"));
}

#[test]
fn whitespace_only_response_is_a_failure() {
    let reason = detect_failure(100, "   \n\t  ", 0.5);
    assert!(reason.is_some());
}

#[test]
fn response_below_truncation_ratio_is_a_failure() {
    // 100 input tokens, ratio 0.5 -> anything under ~50 tokens fails.
    let short_response = "one two three";
    let reason = detect_failure(100, short_response, 0.5);
    assert!(reason.is_some());
    assert!(reason.unwrap().contains("truncat"));
}

#[test]
fn response_above_truncation_ratio_is_not_a_failure() {
    let long_response = "word ".repeat(200);
    let reason = detect_failure(100, &long_response, 0.5);
    assert!(reason.is_none());
}

#[test]
fn zero_input_tokens_never_triggers_truncation_check() {
    let reason = detect_failure(0, "any response at all", 0.5);
    assert!(reason.is_none());
}

#[test]
fn missing_sentinel_is_a_failure_via_restore() {
    let t = "contact a@b.com for details";
    let (_sanitized, spans) = extract(t);
    let err = restore("contact for details", &spans).unwrap_err();
    assert!(err.contains("sentinel"));
}

#[test]
fn duplicated_sentinel_is_a_failure_via_restore() {
    let t = "visit https://example.com now";
    let (sanitized, spans) = extract(t);
    let doubled = format!("{sanitized} {}", spans[0].sentinel);
    let err = restore(&doubled, &spans).unwrap_err();
    assert!(err.contains("sentinel"));
}
