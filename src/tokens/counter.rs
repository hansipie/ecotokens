/// Estimate token count using chars * 0.25 heuristic (D2 — ~80-85% accuracy, <1ms).
/// Kept as public API and used by tests; superseded by `count_tokens` when `exact-tokens` is enabled.
#[cfg_attr(feature = "exact-tokens", allow(dead_code))]
pub fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() as f64 * 0.25).ceil() as usize
}

#[cfg(feature = "exact-tokens")]
mod exact {
    use std::sync::OnceLock;
    use tiktoken_rs::CoreBPE;

    static BPE: OnceLock<CoreBPE> = OnceLock::new();

    pub fn count_tokens(text: &str) -> usize {
        let bpe = BPE.get_or_init(|| {
            tiktoken_rs::cl100k_base().expect("failed to load cl100k_base tokenizer")
        });
        bpe.encode_with_special_tokens(text).len()
    }
}

/// Count tokens exactly (tiktoken cl100k_base) when the `exact-tokens` feature is enabled,
/// otherwise falls back to the character heuristic (~80-85% accuracy).
pub fn count_tokens(text: &str) -> usize {
    #[cfg(feature = "exact-tokens")]
    return exact::count_tokens(text);
    #[cfg(not(feature = "exact-tokens"))]
    estimate_tokens(text)
}
