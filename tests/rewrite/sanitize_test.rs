use ecotokens::rewrite::sanitize::{extract, restore};

#[test]
fn round_trip_identity_holds_for_plain_prose() {
    let t = "This is a simple sentence with no special content.";
    let (sanitized, spans) = extract(t);
    let restored = restore(&sanitized, &spans).unwrap();
    assert_eq!(restored, t);
}

#[test]
fn round_trip_identity_holds_for_mixed_content() {
    let t = "Check out https://example.com/path, email me at a@b.com, on 2026-08-16. \
        Here's `inline code` and a fenced block:\n```\nfn main() {}\n```\nAlso the number 42,000.5.";
    let (sanitized, spans) = extract(t);
    assert!(!spans.is_empty());
    let restored = restore(&sanitized, &spans).unwrap();
    assert_eq!(restored, t);
}

#[test]
fn sentinels_are_unique_per_request() {
    let t = "one 1 two 2 three 3 four 4 five 5";
    let (_sanitized, spans) = extract(t);
    let mut sentinels: Vec<&str> = spans.iter().map(|s| s.sentinel.as_str()).collect();
    let before_len = sentinels.len();
    sentinels.sort();
    sentinels.dedup();
    assert_eq!(sentinels.len(), before_len, "sentinels must all be unique");
}

#[test]
fn input_already_containing_sentinel_characters_is_escaped() {
    let t = "weird input with a literal ⟦ET0⟧ marker and 42 numbers";
    let (sanitized, spans) = extract(t);
    let restored = restore(&sanitized, &spans).unwrap();
    assert_eq!(restored, t);
}

#[test]
fn restore_fails_when_sentinel_is_missing() {
    let t = "call me at a@b.com";
    let (_sanitized, spans) = extract(t);
    // Simulate the model dropping the sentinel entirely.
    let corrupted = "call me sometime";
    let err = restore(corrupted, &spans).unwrap_err();
    assert!(err.contains("0 time"), "got: {err}");
}

#[test]
fn restore_fails_when_sentinel_is_duplicated() {
    let t = "visit https://example.com today";
    let (sanitized, spans) = extract(t);
    let sentinel = &spans[0].sentinel;
    let duplicated = format!("{sanitized} {sentinel}");
    let err = restore(&duplicated, &spans).unwrap_err();
    assert!(err.contains("2 time"), "got: {err}");
}

#[test]
fn fenced_code_block_is_protected() {
    let t = "before\n```\nlet x = 1;\n```\nafter";
    let (sanitized, spans) = extract(t);
    assert!(!sanitized.contains("let x = 1;"));
    assert!(spans.iter().any(|s| s.original.contains("let x = 1;")));
}
