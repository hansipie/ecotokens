pub mod callees;
pub mod callers;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CallEdge {
    /// Symbol ID of the caller/callee
    pub symbol_id: String,
    /// Human-readable name
    pub name: String,
    /// File path relative to project root
    pub file_path: String,
    /// Line number of the call site
    pub line: u64,
}

#[derive(Debug)]
pub enum TraceError {
    IndexNotFound,
    #[allow(dead_code)]
    SymbolNotFound(String),
    Tantivy(tantivy::TantivyError),
    Io(std::io::Error),
}

impl std::fmt::Display for TraceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceError::IndexNotFound => write!(f, "index not found — run `ecotokens index` first"),
            TraceError::SymbolNotFound(s) => write!(f, "symbol not found: {s}"),
            TraceError::Tantivy(e) => write!(f, "tantivy error: {e}"),
            TraceError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for TraceError {}

/// Return the 0-based line index within `source` of the first call to `name`,
/// or `None` if there is none. Comment lines are skipped, and the name must be
/// preceded by a non-identifier character so short names like `f` or `get` do
/// not match inside longer identifiers (e.g. `other_get(`). The index is into
/// the *original* source (not a comment-stripped copy), so callers can offset it
/// by the symbol's starting line to recover a correct file line number.
pub(crate) fn find_call_line(source: &str, name: &str) -> Option<u64> {
    if name.is_empty() {
        return None;
    }
    let pattern = format!("{name}(");
    for (i, line) in source.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("//") || t.starts_with('#') || t.starts_with("--") || t.starts_with('*') {
            continue;
        }
        if line_contains_call(line, &pattern) {
            return Some(i as u64);
        }
    }
    None
}

/// True if `line` contains `pattern` (`name(`) with a non-identifier char before it.
fn line_contains_call(line: &str, pattern: &str) -> bool {
    let bytes = line.as_bytes();
    let mut start = 0;
    while let Some(pos) = line[start..].find(pattern) {
        let idx = start + pos;
        let boundary_ok = idx == 0 || {
            let prev = bytes[idx - 1];
            prev != b'_' && !prev.is_ascii_alphanumeric()
        };
        if boundary_ok {
            return true;
        }
        start = idx + 1;
    }
    false
}

impl From<tantivy::TantivyError> for TraceError {
    fn from(e: tantivy::TantivyError) -> Self {
        TraceError::Tantivy(e)
    }
}

impl From<std::io::Error> for TraceError {
    fn from(e: std::io::Error) -> Self {
        TraceError::Io(e)
    }
}
