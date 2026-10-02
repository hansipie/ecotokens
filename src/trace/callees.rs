use std::collections::HashSet;
use std::path::Path;

use tantivy::collector::TopDocs;
use tantivy::query::TermQuery;
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{Index, ReloadPolicy, TantivyDocument, Term};

use super::{CallEdge, TraceError};
use crate::search::index::build_schema;

const MAX_SYMBOL_DOCS: usize = 10_000;

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

    let (_, file_path_field, content_field, kind_field, line_start_field, symbol_id_field) =
        build_schema();

    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;
    let searcher = reader.searcher();

    // Collect all known symbol names for matching
    let kind_term = Term::from_field_text(kind_field, "symbol");
    let kind_query = TermQuery::new(kind_term, IndexRecordOption::Basic);
    let all_symbols = searcher.search(&kind_query, &TopDocs::with_limit(MAX_SYMBOL_DOCS))?;
    if all_symbols.len() > MAX_SYMBOL_DOCS {
        eprintln!(
            "ecotokens: warning: symbol limit ({MAX_SYMBOL_DOCS}) reached; some callees may be missing"
        );
    }

    let mut symbol_names: HashSet<String> = HashSet::new();
    // (sid, name, file, source, line_start)
    let mut symbol_docs: Vec<(String, String, String, String, u64)> = Vec::new();

    for (_score, addr) in &all_symbols {
        let doc: TantivyDocument = searcher.doc(*addr)?;
        let sid = doc
            .get_first(symbol_id_field)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let source = doc
            .get_first(content_field)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let file = doc
            .get_first(file_path_field)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let line_start = doc
            .get_first(line_start_field)
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let name = sid
            .split("::")
            .last()
            .unwrap_or("")
            .split('#')
            .next()
            .unwrap_or("")
            .to_string();
        symbol_names.insert(name.clone());
        symbol_docs.push((sid, name, file, source, line_start));
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
    symbol_docs: &[(String, String, String, String, u64)],
    known_symbols: &HashSet<String>,
    depth: u32,
    visited: &mut HashSet<String>,
    result: &mut Vec<CallEdge>,
) {
    if depth == 0 || visited.contains(symbol_name) {
        return;
    }
    visited.insert(symbol_name.to_string());

    // Consider every symbol that shares this (short) name, not just the first
    // match — otherwise two same-named symbols in different modules would
    // silently use the wrong body for callee extraction.
    let bodies: Vec<(&str, u64)> = symbol_docs
        .iter()
        .filter(|(_, name, _, _, _)| name == symbol_name)
        .map(|(_, _, _, src, ls)| (src.as_str(), *ls))
        .collect();

    if bodies.iter().all(|(src, _)| src.is_empty()) {
        return;
    }

    // Find all calls to known symbols within these sources
    for known in known_symbols {
        if known == symbol_name {
            continue; // skip self-recursion as callee
        }
        // First body that calls `known`, offset by that body's starting line so
        // the reported line is a real file line, not an index into a stripped copy.
        let call_line = bodies
            .iter()
            .find_map(|(src, ls)| super::find_call_line(src, known).map(|within| ls + within));

        if let Some(call_line) = call_line {
            // Find the callee's file and ID
            let (sid, _, file, _, _) = symbol_docs
                .iter()
                .find(|(_, name, _, _, _)| name == known)
                .cloned()
                .unwrap_or_default();

            if !result.iter().any(|e| e.symbol_id == sid) {
                result.push(CallEdge {
                    symbol_id: sid,
                    name: known.clone(),
                    file_path: file,
                    line: call_line,
                });
            }

            // Recurse if depth > 1
            if depth > 1 {
                find_callees_recursive(
                    known,
                    symbol_docs,
                    known_symbols,
                    depth - 1,
                    visited,
                    result,
                );
            }
        }
    }
}
