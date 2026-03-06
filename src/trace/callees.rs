use std::collections::HashSet;
use std::path::Path;

use tantivy::collector::TopDocs;
use tantivy::query::TermQuery;
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{Index, ReloadPolicy, TantivyDocument, Term};

use crate::search::index::build_schema;
use super::{CallEdge, TraceError};

/// Find all callees (functions called by) the given symbol name.
/// `depth` controls recursive traversal (1 = direct callees only).
pub fn find_callees(
    symbol_name: &str,
    index_dir: &Path,
    depth: u32,
) -> Result<Vec<CallEdge>, TraceError> {
    let index = match Index::open_in_dir(index_dir) {
        Ok(i) => i,
        Err(_) => return Err(TraceError::IndexNotFound),
    };

    let (_, file_path_field, content_field, kind_field, _, symbol_id_field) = build_schema();

    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;
    let searcher = reader.searcher();

    // Collect all known symbol names for matching
    let kind_term = Term::from_field_text(kind_field, "symbol");
    let kind_query = TermQuery::new(kind_term, IndexRecordOption::Basic);
    let all_symbols = searcher.search(&kind_query, &TopDocs::with_limit(10_000))?;

    let mut symbol_names: HashSet<String> = HashSet::new();
    let mut symbol_docs: Vec<(String, String, String, String)> = Vec::new(); // (sid, name, file, source)

    for (_score, addr) in &all_symbols {
        let doc: TantivyDocument = searcher.doc(*addr)?;
        let sid = doc.get_first(symbol_id_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let source = doc.get_first(content_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let file = doc.get_first(file_path_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let name = sid.split("::").last().unwrap_or("").split('#').next().unwrap_or("").to_string();
        symbol_names.insert(name.clone());
        symbol_docs.push((sid, name, file, source));
    }

    let mut visited = HashSet::new();
    let mut result = Vec::new();
    find_callees_recursive(
        symbol_name,
        &symbol_docs,
        &symbol_names,
        depth,
        &mut visited,
        &mut result,
    );

    Ok(result)
}

fn find_callees_recursive(
    symbol_name: &str,
    symbol_docs: &[(String, String, String, String)],
    known_symbols: &HashSet<String>,
    depth: u32,
    visited: &mut HashSet<String>,
    result: &mut Vec<CallEdge>,
) {
    if depth == 0 || visited.contains(symbol_name) {
        return;
    }
    visited.insert(symbol_name.to_string());

    // Find the symbol's source
    let source = symbol_docs
        .iter()
        .find(|(_, name, _, _)| name == symbol_name)
        .map(|(_, _, _, src)| src.as_str())
        .unwrap_or("");

    if source.is_empty() {
        return;
    }

    // Find all calls to known symbols within this source
    for known in known_symbols {
        if known == symbol_name {
            continue; // skip self-recursion as callee
        }
        let call_pattern = format!("{known}(");
        if source.contains(&call_pattern) {
            // Find the line
            let call_line = source
                .lines()
                .enumerate()
                .find(|(_, l)| l.contains(&call_pattern))
                .map(|(i, _)| i as u64)
                .unwrap_or(0);

            // Find the callee's file and ID
            let (sid, _, file, _) = symbol_docs
                .iter()
                .find(|(_, name, _, _)| name == known)
                .cloned()
                .unwrap_or_default();

            if !result.iter().any(|e| e.name == *known) {
                result.push(CallEdge {
                    symbol_id: sid,
                    name: known.clone(),
                    file_path: file,
                    line: call_line,
                });
            }

            // Recurse if depth > 1
            if depth > 1 {
                find_callees_recursive(known, symbol_docs, known_symbols, depth - 1, visited, result);
            }
        }
    }
}
