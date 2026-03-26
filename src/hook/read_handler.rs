use std::path::{Path, PathBuf};

use super::post_handler::PostFilterResult;
use crate::search::outline::OutlineOptions;
use crate::tokens::counter::count_tokens;
use crate::trace::callees::find_callees;
use crate::trace::callers::find_callers;

const MAX_CALLERS_PER_SYMBOL: usize = 5;
const MAX_CALLEES_PER_SYMBOL: usize = 5;

/// Binary file extensions — never try to outline these.
const BINARY_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "svg", "webp", "tiff", "pdf", "doc", "docx", "xls",
    "xlsx", "ppt", "pptx", "zip", "tar", "gz", "bz2", "xz", "7z", "rar", "wasm", "exe", "dll",
    "so", "a", "lib", "dylib", "bin", "dat", "db", "sqlite", "sqlite3", "mp3", "mp4", "wav", "avi",
    "mkv", "mov", "ttf", "otf", "woff", "woff2", "class", "jar", "pyc", "o", "rlib",
];

const MAX_CONTENT_BYTES: usize = 500 * 1024; // 500 KB

fn is_binary_path(file_path: &str) -> bool {
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    BINARY_EXTS.contains(&ext.as_str())
}

fn default_index_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ecotokens")
        .join("index")
}

/// Format one symbol with its callers/callees.
fn format_symbol_with_trace(
    sym_name: &str,
    sym_kind: &str,
    sym_line: u64,
    depth: u32,
    index_dir: &Path,
) -> String {
    let mut lines = vec![format!("  {} {} (l.{})", sym_kind, sym_name, sym_line)];

    if let Ok(callers) = find_callers(sym_name, index_dir) {
        if !callers.is_empty() {
            let shown = callers.len().min(MAX_CALLERS_PER_SYMBOL);
            let extra = callers.len().saturating_sub(MAX_CALLERS_PER_SYMBOL);
            for c in &callers[..shown] {
                lines.push(format!("    ← {}", c.name));
            }
            if extra > 0 {
                lines.push(format!("    +{extra} more callers"));
            }
        }
    }

    if let Ok(callees) = find_callees(sym_name, index_dir, depth) {
        if !callees.is_empty() {
            let shown = callees.len().min(MAX_CALLEES_PER_SYMBOL);
            let extra = callees.len().saturating_sub(MAX_CALLEES_PER_SYMBOL);
            for c in &callees[..shown] {
                lines.push(format!("    → {}", c.name));
            }
            if extra > 0 {
                lines.push(format!("    +{extra} more callees"));
            }
        }
    }

    lines.join("\n")
}

pub fn handle_read(file_path: &str, content: &str, depth: u32) -> PostFilterResult {
    // Guard: empty content
    if content.is_empty() {
        return PostFilterResult::Passthrough;
    }

    // Guard: binary extension
    if is_binary_path(file_path) {
        return PostFilterResult::Passthrough;
    }

    // Guard: large file
    if content.len() > MAX_CONTENT_BYTES {
        return PostFilterResult::Passthrough;
    }

    let tokens_before = count_tokens(content) as u32;
    let index_dir = default_index_dir();

    // Get outline via tree-sitter (works even without tantivy index)
    let path = Path::new(file_path);
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let symbols = match crate::search::outline::outline_path(OutlineOptions {
        path: path.to_path_buf(),
        depth: Some(1),
        kinds: None,
        base: Some(cwd),
    }) {
        Ok(syms) => syms,
        Err(_) => return PostFilterResult::Passthrough,
    };

    if symbols.is_empty() {
        return PostFilterResult::Passthrough;
    }

    // Build outline without trace
    let outline_header = format!("[ecotokens outline] {}", file_path);
    let outline_lines: Vec<String> = symbols
        .iter()
        .map(|s| format!("  {} {} (l.{})", s.kind, s.name, s.line_start))
        .collect();
    let outline_only = format!("{}\n{}", outline_header, outline_lines.join("\n"));

    // Build enriched outline (with callers/callees)
    let enriched_lines: Vec<String> = symbols
        .iter()
        .map(|s| format_symbol_with_trace(&s.name, &s.kind, s.line_start, depth, &index_dir))
        .collect();
    let outline_enriched = format!("{}\n{}", outline_header, enriched_lines.join("\n"));

    // Fallback: pick best option that is smaller than original
    let tokens_enriched = count_tokens(&outline_enriched) as u32;
    let tokens_outline = count_tokens(&outline_only) as u32;

    if tokens_enriched < tokens_before {
        PostFilterResult::Filtered {
            output: outline_enriched,
            tokens_before,
            tokens_after: tokens_enriched,
            content_before: content.to_string(),
        }
    } else if tokens_outline < tokens_before {
        PostFilterResult::Filtered {
            output: outline_only,
            tokens_before,
            tokens_after: tokens_outline,
            content_before: content.to_string(),
        }
    } else {
        PostFilterResult::Passthrough
    }
}
