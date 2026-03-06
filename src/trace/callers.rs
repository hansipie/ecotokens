use std::path::Path;

use tantivy::collector::TopDocs;
use tantivy::query::TermQuery;
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{Index, ReloadPolicy, TantivyDocument, Term};

use crate::search::index::build_schema;
use super::{CallEdge, TraceError};

/// Find all callers of the given symbol name in the indexed codebase.
/// Searches symbol source code for call expressions matching `symbol_name(`.
pub fn find_callers(
    symbol_name: &str,
    index_dir: &Path,
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

    // Find all symbol documents
    let kind_term = Term::from_field_text(kind_field, "symbol");
    let kind_query = TermQuery::new(kind_term, IndexRecordOption::Basic);

    let top_docs = searcher.search(&kind_query, &TopDocs::with_limit(10_000))?;

    let call_pattern = format!("{symbol_name}(");
    let mut edges = Vec::new();

    for (_score, addr) in top_docs {
        let doc: TantivyDocument = searcher.doc(addr)?;

        let source = doc
            .get_first(content_field)
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Skip the symbol itself (don't report self-calls as callers)
        let sid = doc
            .get_first(symbol_id_field)
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Check if this symbol's name matches the target (skip self)
        let sym_name = sid.split("::").last().unwrap_or(sid).split('#').next().unwrap_or("");
        if sym_name == symbol_name {
            continue;
        }

        // Check if source contains a call to the target symbol
        if source.contains(&call_pattern) {
            let file = doc
                .get_first(file_path_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Find the line of the call within this symbol's source
            let call_line = source
                .lines()
                .enumerate()
                .find(|(_, l)| l.contains(&call_pattern))
                .map(|(i, _)| i as u64)
                .unwrap_or(0);

            edges.push(CallEdge {
                symbol_id: sid.to_string(),
                name: sym_name.to_string(),
                file_path: file,
                line: call_line,
            });
        }
    }

    Ok(edges)
}
