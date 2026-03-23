use ecotokens::tokens::estimate_tokens;

#[cfg(feature = "exact-tokens")]
use ecotokens::tokens::count_tokens;

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

#[cfg(feature = "exact-tokens")]
#[test]
fn exact_empty() {
    assert_eq!(count_tokens(""), 0);
}

#[cfg(feature = "exact-tokens")]
#[test]
fn exact_hello_world() {
    // "Hello, world!" → 4 tokens en cl100k_base
    assert_eq!(count_tokens("Hello, world!"), 4);
}

#[cfg(feature = "exact-tokens")]
#[test]
fn exact_more_precise_than_heuristic() {
    // Pour un texte connu, tiktoken doit donner un résultat différent de l'heuristique
    let text = "The quick brown fox jumps over the lazy dog";
    let exact = count_tokens(text);
    let estimate = estimate_tokens(text);
    // tiktoken: 9 tokens, heuristique: ceil(43 * 0.25) = 11
    assert_ne!(
        exact, estimate,
        "tiktoken and heuristic should differ on this text"
    );
    assert_eq!(exact, 9);
}
