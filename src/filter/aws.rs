use crate::filter::generic::filter_generic;

const MAX_JSON_BYTES: usize = 50 * 1024;

/// Filter AWS CLI output: minify JSON or apply generic filter.
pub fn filter_aws(output: &str) -> String {
    let trimmed = output.trim();
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let compact = serde_json::to_string(&json).unwrap_or_else(|_| trimmed.to_string());
        if compact.len() <= MAX_JSON_BYTES {
            compact
        } else {
            format!("{}…[truncated]", &compact[..MAX_JSON_BYTES])
        }
    } else {
        filter_generic(output, 100, 51200)
    }
}
