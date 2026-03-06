use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::Value;
use tantivy::{Index, ReloadPolicy, TantivyDocument};

use super::index::build_schema;

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub query: String,
    pub top_k: usize,
    pub index_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub file_path: String,
    pub snippet: String,
    pub score: f32,
    pub line_start: u64,
}

pub fn search_index(opts: SearchOptions) -> tantivy::Result<Vec<SearchResult>> {
    let index = Index::open_in_dir(&opts.index_dir)?;
    let (_, file_path_field, content_field, _, line_start_field, _) = build_schema();

    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;
    let searcher = reader.searcher();

    let query_parser = QueryParser::for_index(&index, vec![content_field]);
    let query = query_parser.parse_query(&opts.query)?;

    let top_docs = searcher.search(&query, &TopDocs::with_limit(opts.top_k))?;

    let mut results = Vec::new();
    for (score, doc_address) in top_docs {
        let doc: TantivyDocument = searcher.doc(doc_address)?;
        let file_path = doc
            .get_first(file_path_field)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let snippet = doc
            .get_first(content_field)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let line_start = doc
            .get_first(line_start_field)
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        results.push(SearchResult {
            file_path,
            snippet,
            score,
            line_start,
        });
    }
    Ok(results)
}
