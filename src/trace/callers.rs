use std::path::Path;

use tantivy::collector::TopDocs;
use tantivy::query::TermQuery;
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{Index, ReloadPolicy, TantivyDocument, Term};

use super::{CallEdge, TraceError};
use crate::search::index::build_schema;

const MAX_SYMBOL_DOCS: usize = 10_000;

/// Find all callers of the given symbol name in the indexed codebase.
/// Searches symbol source code for call expressions matching `symbol_name(`.
pub fn find_callers(symbol_name: &str, index_dir: &Path) -> Result<Vec<CallEdge>, TraceError> {
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

    // Find all symbol documents
    let kind_term = Term::from_field_text(kind_field, "symbol");
    let kind_query = TermQuery::new(kind_term, IndexRecordOption::Basic);

    let top_docs = searcher.search(&kind_query, &TopDocs::with_limit(MAX_SYMBOL_DOCS))?;
    if top_docs.len() > MAX_SYMBOL_DOCS {
        eprintln!(
            "ecotokens: warning: symbol limit ({MAX_SYMBOL_DOCS}) reached; some callers may be missing"
        );
    }

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
        let sym_name = sid
            .split("::")
            .last()
            .unwrap_or(sid)
            .split('#')
            .next()
            .unwrap_or("");
        if sym_name == symbol_name {
            continue;
        }

        // Locate the call within the symbol's original source (comment lines are
        // skipped internally, but not removed, so the index is not shifted).
        if let Some(within) = super::find_call_line(source, symbol_name) {
            let file = doc
                .get_first(file_path_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Offset by the symbol's starting line to get a real file line number.
            let sym_line_start = doc
                .get_first(line_start_field)
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let call_line = sym_line_start + within;

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
