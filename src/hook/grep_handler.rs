use std::path::{Path, PathBuf};

use super::post_handler::PostFilterResult;
use crate::filter::grep::{filter_grep, parse_grep_line};
use crate::search::outline::OutlineOptions;
use crate::tokens::counter::count_tokens;
use crate::trace::callees::find_callees;
use crate::trace::callers::find_callers;

const MAX_CALLERS_PER_SYMBOL: usize = 5;
const MAX_CALLEES_PER_SYMBOL: usize = 5;

/// Find the symbol whose line_start is the highest value ≤ line_no in the given file.
/// Returns `(symbol_name, symbol_id)` or None if no symbol found.
fn symbol_at_line(file_path: &str, line_no: usize) -> Option<(String, String)> {
    let path = Path::new(file_path);
    if !path.exists() {
        return None;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let symbols = crate::search::outline::outline_path(OutlineOptions {
        path: path.to_path_buf(),
        depth: Some(1),
        kinds: None,
        base: Some(cwd),
    })
    .unwrap_or_default();

    // Find the symbol with the highest line_start that is still ≤ line_no
    symbols
        .iter()
        .filter(|s| s.line_start as usize <= line_no)
        .max_by_key(|s| s.line_start)
        .map(|s| (s.name.clone(), s.id.clone()))
}

/// Build enrichment text for a symbol: callers + callees up to MAX_* entries.
fn enrich_symbol(symbol_name: &str, depth: u32, index_dir: &Path) -> String {
    let mut parts = Vec::new();

    if let Ok(callers) = find_callers(symbol_name, index_dir) {
        if !callers.is_empty() {
            let shown = callers.len().min(MAX_CALLERS_PER_SYMBOL);
            let extra = callers.len().saturating_sub(MAX_CALLERS_PER_SYMBOL);
            let caller_strs: Vec<String> = callers[..shown]
                .iter()
                .map(|e| format!("    ← {}", e.name))
                .collect();
            let mut block = format!(
                "  callers of `{}`:\n{}",
                symbol_name,
                caller_strs.join("\n")
            );
            if extra > 0 {
                block.push_str(&format!("\n    +{extra} more"));
            }
            parts.push(block);
        }
    }

    if let Ok(callees) = find_callees(symbol_name, index_dir, depth) {
        if !callees.is_empty() {
            let shown = callees.len().min(MAX_CALLEES_PER_SYMBOL);
            let extra = callees.len().saturating_sub(MAX_CALLEES_PER_SYMBOL);
            let callee_strs: Vec<String> = callees[..shown]
                .iter()
                .map(|e| format!("    → {}", e.name))
                .collect();
            let mut block = format!(
                "  callees of `{}`:\n{}",
                symbol_name,
                callee_strs.join("\n")
            );
            if extra > 0 {
                block.push_str(&format!("\n    +{extra} more"));
            }
            parts.push(block);
        }
    }

    parts.join("\n")
}

pub fn handle_grep(output: &str, depth: u32) -> PostFilterResult {
    let line_count = output.lines().count();
    if output.trim().is_empty() || line_count == 0 {
        return PostFilterResult::Passthrough;
    }

    // Step 1: threshold check — ≤ 30 lines → Passthrough
    if line_count <= 30 {
        return PostFilterResult::Passthrough;
    }

    let tokens_before = count_tokens(output) as u32;

    // Step 2: compact with filter_grep
    let compacted = filter_grep(output);

    // Step 3: attempt symbol enrichment
    let index_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ecotokens")
        .join("index");

    let mut enriched_lines: Vec<String> = Vec::new();
    let mut last_sym: Option<String> = None;

    for line in output.lines() {
        enriched_lines.push(line.to_string());
        if let Some((file, (Some(lineno), _))) = parse_grep_line(line) {
            if let Some((sym_name, _)) = symbol_at_line(&file, lineno) {
                // Only enrich if different symbol from previous match
                if last_sym.as_deref() != Some(&sym_name) {
                    let enrichment = enrich_symbol(&sym_name, depth, &index_dir);
                    if !enrichment.is_empty() {
                        enriched_lines.push(enrichment);
                    }
                    last_sym = Some(sym_name);
                }
            }
        }
    }
    let enriched = enriched_lines.join("\n");

    // Step 4: fallback — pick best option
    let tokens_compacted = count_tokens(&compacted) as u32;
    let tokens_enriched = count_tokens(&enriched) as u32;

    if tokens_enriched < tokens_before {
        PostFilterResult::Filtered {
            output: enriched,
            tokens_before,
            tokens_after: tokens_enriched,
            content_before: output.to_string(),
        }
    } else if tokens_compacted < tokens_before {
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
