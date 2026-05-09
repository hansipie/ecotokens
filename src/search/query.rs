use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::Term;
use tantivy::{Index, ReloadPolicy, TantivyDocument};

use super::embed::{cosine_similarity, embed_text, load_embeddings};
use super::hnsw::HnswIndex;
use super::index::build_schema;

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub query: String,
    pub top_k: usize,
    pub index_dir: PathBuf,
    /// Embedding provider used for semantic retrieval (None/Candle/Ollama/LmStudio)
    pub embed_provider: crate::config::settings::EmbedProvider,
}

/// How this result was retrieved — useful for debugging and metrics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RetrievalSource {
    Bm25,
    Vector,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub file_path: String,
    pub snippet: String,
    pub score: f32,
    pub line_start: u64,
    /// Last line of the chunk (absent for legacy BM25-only chunks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_end: Option<u64>,
    pub retrieval_source: RetrievalSource,
}

pub fn search_index(opts: SearchOptions) -> tantivy::Result<Vec<SearchResult>> {
    let index = Index::open_in_dir(&opts.index_dir)?;
    let (_, file_path_field, content_field, kind_field, line_start_field, symbol_id_field) =
        build_schema();

    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;
    let searcher = reader.searcher();

    let query_parser = QueryParser::for_index(&index, vec![content_field]);
    let content_query = query_parser.parse_query(&opts.query).or_else(|_| {
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
    // Keep search focused on textual BM25 chunks (exclude symbolic docs).
    let kind_term = Term::from_field_text(kind_field, "bm25");
    let kind_query = TermQuery::new(kind_term, IndexRecordOption::Basic);
    let query = BooleanQuery::new(vec![
        (Occur::Must, Box::new(content_query)),
        (Occur::Must, Box::new(kind_query)),
    ]);

    let fetch_k = opts.top_k * 3;
    let top_docs = searcher.search(&query, &TopDocs::with_limit(fetch_k))?;

    // ── Vector retrieval (best-effort; never blocks BM25 path) ────────────────
    let query_embedding = embed_text(&opts.query, &opts.embed_provider);
    let hnsw_index = HnswIndex::load(&opts.index_dir);
    let vector_hits: std::collections::HashMap<String, f32> =
        if let (Some(ref qvec), Some(ref idx)) = (&query_embedding, &hnsw_index) {
            idx.search(qvec, fetch_k).into_iter().collect()
        } else {
            std::collections::HashMap::new()
        };

    // Legacy embeddings.json support (pre-HNSW chunks)
    let stored_embeddings = if query_embedding.is_some() && hnsw_index.is_none() {
        load_embeddings(&opts.index_dir)
    } else {
        std::collections::HashMap::new()
    };

    let bm25_max = top_docs
        .iter()
        .map(|(s, _)| *s)
        .fold(f32::EPSILON, f32::max);

    // BM25 results → fused scores; tuple = (score, snippet, line_start, source, file_path)
    let mut scored: std::collections::HashMap<String, (f32, String, u64, RetrievalSource, String)> =
        std::collections::HashMap::new();

    for (bm25_score, doc_address) in &top_docs {
        let doc: TantivyDocument = searcher.doc(*doc_address)?;
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
        let chunk_key = doc
            .get_first(symbol_id_field)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{file_path}:{}", line_start / 50));

        let bm25_norm = bm25_score / bm25_max;

        // Look up vector score from HNSW or legacy embeddings
        let vec_score = if let Some(s) = vector_hits.get(&chunk_key) {
            Some(*s)
        } else if let Some(ref qvec) = query_embedding {
            stored_embeddings
                .get(&chunk_key)
                .map(|cv| cosine_similarity(qvec, cv).max(0.0))
        } else {
            None
        };

        let (score, source) = match vec_score {
            Some(vs) => (0.4 * bm25_norm + 0.6 * vs, RetrievalSource::Both),
            None => (bm25_norm, RetrievalSource::Bm25),
        };

        scored
            .entry(chunk_key)
            .and_modify(|e| {
                if score > e.0 {
                    *e = (
                        score,
                        snippet.clone(),
                        line_start,
                        source.clone(),
                        file_path.clone(),
                    );
                }
            })
            .or_insert((score, snippet, line_start, source, file_path));
    }

    // Pure vector-only hits (not found by BM25)
    if let Some(ref _qvec) = query_embedding {
        for (chunk_key, vscore) in &vector_hits {
            if scored.contains_key(chunk_key) {
                continue;
            }
            // Derive file_path from chunk_key: "rel_path::name#kind" → "rel_path", "rel_path:idx" → "rel_path"
            let file_path = if let Some(pos) = chunk_key.find("::") {
                chunk_key[..pos].to_string()
            } else if let Some(last_colon) = chunk_key.rfind(':') {
                let after = &chunk_key[last_colon + 1..];
                if after.chars().all(|c| c.is_ascii_digit()) {
                    chunk_key[..last_colon].to_string()
                } else {
                    chunk_key.clone()
                }
            } else {
                chunk_key.clone()
            };
            scored.insert(
                chunk_key.clone(),
                (
                    0.6 * vscore,
                    String::new(), // snippet not available from HNSW alone
                    0,
                    RetrievalSource::Vector,
                    file_path,
                ),
            );
        }
    }

    let mut results: Vec<SearchResult> = scored
        .into_iter()
        .map(
            |(_, (score, snippet, line_start, source, file_path))| SearchResult {
                file_path,
                snippet,
                score,
                line_start,
                line_end: None,
                retrieval_source: source,
            },
        )
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(opts.top_k);

    Ok(results)
}
