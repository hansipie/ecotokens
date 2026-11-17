pub mod embed;
pub mod hnsw;
pub mod index;
pub mod outline;
pub mod query;
pub mod symbols;
pub mod text_docs;

/// Extensions parsed into symbols with a tree-sitter grammar.
pub const CODE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "jsx", "tsx", "c", "h", "cpp", "cc", "cxx", "hpp", "hh", "hxx", "gd",
];

/// Extensions split into structural sections (headings, tables, top-level keys).
pub const TEXT_DOC_EXTENSIONS: &[&str] = &["md", "markdown", "toml", "json", "yaml", "yml"];

/// Extensions indexed as plain text, chunked by lines.
pub const PLAIN_TEXT_EXTENSIONS: &[&str] = &["txt"];

/// True when `ext` has a tree-sitter grammar for symbol extraction.
pub fn is_code_extension(ext: &str) -> bool {
    CODE_EXTENSIONS.contains(&ext)
}

/// True when `ext` is a structured text document (Markdown, TOML, JSON, YAML).
pub fn is_text_doc_extension(ext: &str) -> bool {
    TEXT_DOC_EXTENSIONS.contains(&ext)
}

/// Common list of indexable file extensions used across the codebase.
pub fn is_indexable_extension(ext: &str) -> bool {
    is_code_extension(ext) || is_text_doc_extension(ext) || PLAIN_TEXT_EXTENSIONS.contains(&ext)
}
