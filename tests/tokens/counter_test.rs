use ecotokens::tokens::estimate_tokens;

#[test]
fn empty_text_returns_zero() {
    assert_eq!(estimate_tokens(""), 0);
}

#[test]
fn short_text_rounds_up() {
    assert_eq!(estimate_tokens("hi"), 1);
}

#[test]
fn exact_multiple() {
    assert_eq!(estimate_tokens("abcd"), 1);
}

#[test]
fn long_text_scales() {
    let s = "a".repeat(1000);
    assert_eq!(estimate_tokens(&s), 250);
}

#[test]
fn unicode_counts_chars_not_bytes() {
    assert_eq!(estimate_tokens("é"), 1);
}
