use super::post_handler::PostFilterResult;
use crate::filter::grep::filter_grep;
use crate::tokens::counter::count_tokens;

/// Compact large grep output. `_depth` is retained for call-site compatibility;
/// the former symbol-enrichment path was removed because it appended annotations
/// on top of every original line and so could never reduce the token count
/// (its fallback branch was permanently dead code).
pub fn handle_grep(output: &str, _depth: u32) -> PostFilterResult {
    let line_count = output.lines().count();
    if output.trim().is_empty() || line_count == 0 {
        return PostFilterResult::Passthrough;
    }

    // Threshold check — ≤ 30 lines → Passthrough (no compaction needed)
    if line_count <= 30 {
        return PostFilterResult::Passthrough;
    }

    let tokens_before = count_tokens(output) as u32;
    let compacted = filter_grep(output);
    let tokens_compacted = count_tokens(&compacted) as u32;

    if tokens_compacted < tokens_before {
        PostFilterResult::Filtered {
            output: compacted,
            tokens_before,
            tokens_after: tokens_compacted,
            content_before: output.to_string(),
        }
    } else {
        PostFilterResult::Passthrough
    }
}
