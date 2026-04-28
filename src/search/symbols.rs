use serde::{Deserialize, Serialize};
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::TermQuery;
use tantivy::schema::IndexRecordOption;
use tantivy::schema::Value;
use tantivy::{doc, IndexWriter, TantivyDocument};
use tantivy::{Index, ReloadPolicy, Term};
use tree_sitter::{Language, Parser};

use super::index::build_schema;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    /// Stable ID: "{rel_file_path}::{name}#{kind}"
    pub id: String,
    pub name: String,
    /// "fn" | "struct" | "impl" | "enum" | "trait" | "h1" | "h2" | "h3" | "table" | "key"
    pub kind: String,
    pub file_path: String,
    pub line_start: u64,
    pub source: String,
}

#[derive(Debug)]
pub enum SymbolError {
    Io(std::io::Error),
    Parse,
}

impl std::fmt::Display for SymbolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SymbolError::Io(e) => write!(f, "io error: {e}"),
            SymbolError::Parse => write!(f, "parse error"),
        }
    }
}

impl From<std::io::Error> for SymbolError {
    fn from(e: std::io::Error) -> Self {
        SymbolError::Io(e)
    }
}

impl std::error::Error for SymbolError {}

fn language_for_ext(ext: &str) -> Option<Language> {
    match ext {
        "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        "py" => Some(tree_sitter_python::LANGUAGE.into()),
        "ts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "js" | "jsx" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "c" | "h" => Some(tree_sitter_c::LANGUAGE.into()),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Some(tree_sitter_cpp::LANGUAGE.into()),
        _ => None,
    }
}

/// Node kinds that represent top-level declarations for each language.
fn is_declaration_node(node_kind: &str) -> Option<&'static str> {
    match node_kind {
        "function_item" => Some("fn"),
        "struct_item" => Some("struct"),
        "impl_item" => Some("impl"),
        "enum_item" => Some("enum"),
        "trait_item" => Some("trait"),
        "function_definition" => Some("fn"),   // Python, C, C++
        "class_definition" => Some("struct"),  // Python
        "function_declaration" => Some("fn"),  // JS
        "class_declaration" => Some("struct"), // JS
        "struct_specifier" => Some("struct"),  // C, C++
        "enum_specifier" => Some("enum"),      // C, C++
        "type_definition" => Some("type"),     // C (typedef)
        "class_specifier" => Some("struct"),   // C++
        "namespace_definition" => Some("namespace"), // C++
        _ => None,
    }
}

/// Extract the declaration name from a node (first `identifier` child).
/// For C/C++ functions, the name is nested inside `function_declarator` or
/// `pointer_declarator`, so we recurse into those.
fn extract_name<'a>(node: tree_sitter::Node<'a>, source: &'a str) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" || child.kind() == "type_identifier" {
            return child.utf8_text(source.as_bytes()).ok();
        }
        if matches!(child.kind(), "function_declarator" | "pointer_declarator") {
            if let Some(name) = extract_name(child, source) {
                return Some(name);
            }
        }
    }
    None
}

/// Parse symbols from a source file using tree-sitter AST.
/// Returns an empty vec for unsupported file extensions (no error).
pub fn parse_symbols(path: &Path) -> Result<Vec<Symbol>, SymbolError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let lang = match language_for_ext(ext) {
        Some(l) => l,
        None => return Ok(vec![]),
    };

    let content = std::fs::read_to_string(path)?;
    if content.is_empty() {
        return Ok(vec![]);
    }

    let mut parser = Parser::new();
    parser.set_language(&lang).map_err(|_| SymbolError::Parse)?;
    let tree = parser.parse(&content, None).ok_or(SymbolError::Parse)?;

    let rel = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mut symbols = Vec::new();
    let root = tree.root_node();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if let Some(kind) = is_declaration_node(child.kind()) {
            if let Some(name) = extract_name(child, &content) {
                let line_start = child.start_position().row as u64;
                let end_byte = child.end_byte().min(content.len());
                let source_snippet = content[child.start_byte()..end_byte].to_string();
                let id = format!("{rel}::{name}#{kind}");
                symbols.push(Symbol {
                    id,
                    name: name.to_string(),
                    kind: kind.to_string(),
                    file_path: rel.to_string(),
                    line_start,
                    source: source_snippet,
                });
            }
        }
    }

    Ok(symbols)
}

/// Write a list of symbols into an open tantivy IndexWriter.
/// Caller is responsible for calling writer.commit().
pub fn write_symbols(symbols: &[Symbol], writer: &mut IndexWriter) -> tantivy::Result<()> {
    let (_, file_path_field, content_field, kind_field, line_start_field, symbol_id_field) =
        build_schema();
    for sym in symbols {
        writer.add_document(doc!(
            file_path_field => sym.file_path.clone(),
            content_field   => sym.source.clone(),
            kind_field      => "symbol",
            line_start_field => sym.line_start,
            symbol_id_field => sym.id.clone(),
        ))?;
    }
    Ok(())
}

/// Look up a symbol by stable ID from the tantivy symbolic index.
/// Returns None if the ID is not found or the index is empty/missing.
pub fn lookup_symbol(id: &str, index_dir: &Path) -> tantivy::Result<Option<String>> {
    let index = match Index::open_in_dir(index_dir) {
        Ok(i) => i,
        Err(_) => return Ok(None),
    };
    let (_, _, content_field, _, _, symbol_id_field) = build_schema();

    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;
    let searcher = reader.searcher();

    let term = Term::from_field_text(symbol_id_field, id);
    let query = TermQuery::new(term, IndexRecordOption::Basic);
    let top_docs = searcher.search(&query, &TopDocs::with_limit(1))?;

    if let Some((_, addr)) = top_docs.first() {
        let doc: TantivyDocument = searcher.doc(*addr)?;
        let source = doc
            .get_first(content_field)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        return Ok(source);
    }
    Ok(None)
}
