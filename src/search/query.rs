use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::Term;
use tantivy::{Index, ReloadPolicy, TantivyDocument};

use super::embed::{cosine_similarity, embed_text, load_embeddings};
use super::index::build_schema;

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub query: String,
    pub top_k: usize,
    pub index_dir: PathBuf,
    /// Embedding provider used for semantic re-ranking (None = BM25 only)
    pub embed_provider: crate::config::settings::EmbedProvider,
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
    let (_, file_path_field, content_field, kind_field, line_start_field, _) = build_schema();

    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;
    let searcher = reader.searcher();

    let query_parser = QueryParser::for_index(&index, vec![content_field]);
    let content_query = query_parser.parse_query(&opts.query).or_else(|_| {
        // Strip tantivy special characters and retry as a plain literal search
        let clean: String = opts
            .query
            .chars()
            .filter(|c| {
                !matches!(
                    c,
                    '+' | '-'
                        | '!'
                        | '('
                        | ')'
                        | '{'
                        | '}'
                        | '['
                        | ']'
                        | '^'
                        | '"'
                        | '~'
                        | '*'
                        | '?'
                        | ':'
                        | '\\'
                )
            })
            .collect();
        query_parser.parse_query(&clean)
    })?;
    // Keep search focused on textual BM25 chunks and exclude symbolic docs.
    let kind_term = Term::from_field_text(kind_field, "bm25");
    let kind_query = TermQuery::new(kind_term, IndexRecordOption::Basic);
    let query = BooleanQuery::new(vec![
        (Occur::Must, Box::new(content_query)),
        (Occur::Must, Box::new(kind_query)),
    ]);

    // Fetch more BM25 results than needed when semantic re-scoring is enabled.
    let fetch_k = opts.top_k * 3;
    let top_docs = searcher.search(&query, &TopDocs::with_limit(fetch_k))?;

    // Load embeddings and compute query embedding (best-effort).
    let query_embedding = embed_text(&opts.query, &opts.embed_provider);
    let stored_embeddings = if query_embedding.is_some() {
        load_embeddings(&opts.index_dir)
    } else {
        std::collections::HashMap::new()
    };

    let bm25_max = top_docs
        .iter()
        .map(|(s, _)| *s)
        .fold(f32::EPSILON, f32::max);

    let mut results = Vec::new();
    for (bm25_score, doc_address) in top_docs {
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

        // Semantic re-ranking: 50% normalized BM25 + 50% cosine when embeddings are available.
        // BM25 is normalized by the max score in the result set to match the cosine range [0, 1].
        let bm25_norm = bm25_score / bm25_max;
        let score = if let Some(ref qvec) = query_embedding {
            let chunk_idx = line_start / 50;
            let key = format!("{file_path}:{chunk_idx}");
            if let Some(chunk_vec) = stored_embeddings.get(&key) {
                let sem_score = cosine_similarity(qvec, chunk_vec).max(0.0);
                0.5 * bm25_norm + 0.5 * sem_score
            } else {
                bm25_norm
            }
        } else {
            bm25_norm
        };

        results.push(SearchResult {
            file_path,
            snippet,
            score,
            line_start,
        });
    }

    // Sort by descending score and keep only top_k.
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(opts.top_k);

    Ok(results)
}
