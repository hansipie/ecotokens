pub mod embed;
pub mod hnsw;
pub mod index;
pub mod outline;
pub mod query;
pub mod symbols;
pub mod text_docs;

/// Common list of indexable file extensions used across the codebase.
pub fn is_indexable_extension(ext: &str) -> bool {
    matches!(
        ext,
        "rs" | "py"
            | "js"
            | "ts"
            | "jsx"
            | "tsx"
            | "c"
            | "h"
            | "cpp"
            | "cc"
            | "cxx"
            | "hpp"
            | "hh"
            | "hxx"
            | "md"
            | "toml"
            | "json"
            | "yaml"
            | "yml"
            | "txt"
    )
}
