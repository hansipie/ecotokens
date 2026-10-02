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
/// For directories, `depth` limits the recursion, counting the directory itself
/// as depth 0 (so `Some(1)` = only files directly inside; `Some(0)` = nothing;
/// `None` = unlimited). Directory traversal uses the `ignore` crate, so it
/// respects `.gitignore` and does not follow symlinks (no cycle / stack overflow).
/// `kinds` filters by symbol kind (None = all kinds).
pub fn outline_path(opts: OutlineOptions) -> Result<Vec<Symbol>, Box<dyn std::error::Error>> {
    let cwd = opts
        .base
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .canonicalize()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());

    let mut symbols = if opts.path.is_file() {
        symbols_for_file(&opts.path, &cwd)?
    } else {
        let mut result = Vec::new();
        let mut builder = ignore::WalkBuilder::new(&opts.path);
        builder.hidden(false).git_ignore(true).follow_links(false);
        if let Some(d) = opts.depth {
            builder.max_depth(Some(d as usize));
        }
        for entry in builder.build() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                result.extend(symbols_for_file(entry.path(), &cwd)?);
            }
        }
        result
    };

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

/// Parse the symbols of a single source or text file, rewriting IDs and paths to
/// be relative to `cwd`.
fn symbols_for_file(path: &Path, cwd: &Path) -> Result<Vec<Symbol>, Box<dyn std::error::Error>> {
    let mut result = Vec::new();
    let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let rel_path = abs_path
        .strip_prefix(cwd)
        .unwrap_or(&abs_path)
        .to_string_lossy()
        .to_string();
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "h" | "cpp" | "cc" | "cxx" | "hpp"
        | "hh" | "hxx" => {
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

    Ok(result)
}
