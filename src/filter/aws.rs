use crate::filter::generic::{filter_generic, floor_char_boundary};

const MAX_JSON_BYTES: usize = 50 * 1024;

/// Filter AWS CLI output: minify JSON or apply generic filter.
pub fn filter_aws(output: &str) -> String {
    let trimmed = output.trim();
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let compact = serde_json::to_string(&json).unwrap_or_else(|_| trimmed.to_string());
        if compact.len() <= MAX_JSON_BYTES {
            compact
        } else {
            // Truncating mid-JSON necessarily produces invalid JSON, so label it
            // explicitly rather than emitting a silent `…[truncated]` that a
            // downstream JSON parser would choke on with no diagnostic.
            let boundary = floor_char_boundary(&compact, MAX_JSON_BYTES);
            format!(
                "[ecotokens] AWS JSON truncated to {} of {} bytes (no longer valid JSON):\n{}…[truncated]",
                boundary,
                compact.len(),
                &compact[..boundary]
            )
        }
    } else {
        filter_generic(output, 100, 51200)
    }
}
