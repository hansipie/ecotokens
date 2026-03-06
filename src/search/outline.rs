use std::path::PathBuf;

use super::symbols::{parse_symbols, Symbol};
use super::text_docs::index_text_doc;

pub struct OutlineOptions {
    pub path: PathBuf,
    pub depth: Option<u32>,
    pub kinds: Option<Vec<String>>,
}

/// Return the list of symbols for a file or directory, sorted by line_start.
/// For directories, `depth` limits the recursion (None = unlimited).
/// `kinds` filters by symbol kind (None = all kinds).
pub fn outline_path(opts: OutlineOptions) -> Result<Vec<Symbol>, Box<dyn std::error::Error>> {
    let mut symbols = collect_symbols(&opts.path, opts.depth.unwrap_or(u32::MAX), 0)?;

    if let Some(ref kinds) = opts.kinds {
        symbols.retain(|s| kinds.contains(&s.kind));
    }

    symbols.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then(a.line_start.cmp(&b.line_start))
    });
    Ok(symbols)
}

fn collect_symbols(
    path: &PathBuf,
    max_depth: u32,
    current_depth: u32,
) -> Result<Vec<Symbol>, Box<dyn std::error::Error>> {
    let mut result = Vec::new();

    if path.is_file() {
        let rel = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match ext {
            "rs" | "py" | "js" | "ts" | "jsx" | "tsx" => {
                let syms = parse_symbols(path)?;
                result.extend(syms);
            }
            "md" | "markdown" | "toml" | "json" | "yaml" | "yml" => {
                let syms = index_text_doc(path, rel)?;
                result.extend(syms);
            }
            _ => {}
        }
    } else if path.is_dir() && current_depth < max_depth {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let child = entry.path();
            let mut child_syms = collect_symbols(&child, max_depth, current_depth + 1)?;
            // Prefix file_path with directory relative prefix
            let prefix = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !prefix.is_empty() {
                for s in &mut child_syms {
                    if !s.file_path.contains('/') {
                        s.file_path = format!("{prefix}/{}", s.file_path);
                        s.id = format!("{prefix}/{}", s.id);
                    }
                }
            }
            result.extend(child_syms);
        }
    }

    Ok(result)
}
