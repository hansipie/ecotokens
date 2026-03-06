pub mod callers;
pub mod callees;

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
