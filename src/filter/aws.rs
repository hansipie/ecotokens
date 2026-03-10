use crate::filter::generic::filter_generic;

const MAX_JSON_BYTES: usize = 50 * 1024;

/// Returns the largest byte index ≤ `max` that falls on a UTF-8 char boundary.
fn floor_char_boundary(s: &str, max: usize) -> usize {
    if max >= s.len() {
        return s.len();
    }
    let mut i = max;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Filter AWS CLI output: minify JSON or apply generic filter.
pub fn filter_aws(output: &str) -> String {
    let trimmed = output.trim();
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let compact = serde_json::to_string(&json).unwrap_or_else(|_| trimmed.to_string());
        if compact.len() <= MAX_JSON_BYTES {
            compact
        } else {
            let boundary = floor_char_boundary(&compact, MAX_JSON_BYTES);
            format!("{}…[truncated]", &compact[..boundary])
        }
    } else {
        filter_generic(output, 100, 51200)
    }
}
