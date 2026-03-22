use std::path::{Path, PathBuf};

use super::symbols::{parse_symbols, Symbol};
use super::text_docs::index_text_doc;

pub struct OutlineOptions {
    pub path: PathBuf,
    pub depth: Option<u32>,
    pub kinds: Option<Vec<String>>,
    /// Base directory used to compute relative paths in symbol IDs.
    /// Defaults to the current working directory when `None`.
    pub base: Option<PathBuf>,
}

/// Return the list of symbols for a file or directory, sorted by line_start.
/// For directories, `depth` limits the recursion (None = unlimited).
/// `kinds` filters by symbol kind (None = all kinds).
pub fn outline_path(opts: OutlineOptions) -> Result<Vec<Symbol>, Box<dyn std::error::Error>> {
    let cwd = opts
        .base
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .canonicalize()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());
    let mut symbols = collect_symbols(&opts.path, opts.depth.unwrap_or(u32::MAX), 0, &cwd)?;

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
    cwd: &Path,
) -> Result<Vec<Symbol>, Box<dyn std::error::Error>> {
    let mut result = Vec::new();

    if path.is_file() {
        let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let rel_path = abs_path
            .strip_prefix(cwd)
            .unwrap_or(&abs_path)
            .to_string_lossy()
            .to_string();
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match ext {
            "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "h" | "cpp" | "cc" | "cxx"
            | "hpp" | "hh" | "hxx" => {
                let mut syms = parse_symbols(path)?;
                for sym in &mut syms {
                    if sym.file_path == filename {
                        if let Some(suffix) = sym.id.strip_prefix(&format!("{filename}::")) {
                            sym.id = format!("{rel_path}::{suffix}");
                        }
                        sym.file_path = rel_path.clone();
                    }
                }
                result.extend(syms);
            }
            "md" | "markdown" | "toml" | "json" | "yaml" | "yml" => {
                let syms = index_text_doc(path, &rel_path)?;
                result.extend(syms);
            }
            _ => {}
        }
    } else if path.is_dir() && current_depth < max_depth {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let child = entry.path();
            result.extend(collect_symbols(&child, max_depth, current_depth + 1, cwd)?);
        }
    }

    Ok(result)
}
