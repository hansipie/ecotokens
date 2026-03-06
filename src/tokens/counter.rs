/// Estimate token count using chars * 0.25 heuristic (D2 — ~80-85% accuracy, <1ms).
pub fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() as f64 * 0.25).ceil() as usize
}
