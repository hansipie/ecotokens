use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::Value;
use tantivy::{Index, ReloadPolicy, TantivyDocument};

use super::embed::{cosine_similarity, embed_text, load_embeddings};
use super::index::build_schema;

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub query: String,
    pub top_k: usize,
    pub index_dir: PathBuf,
    /// Provider d'embeddings pour le re-scoring sémantique (None = BM25 seul)
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
    let (_, file_path_field, content_field, _, line_start_field, _) = build_schema();

    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;
    let searcher = reader.searcher();

    let query_parser = QueryParser::for_index(&index, vec![content_field]);
    let query = query_parser.parse_query(&opts.query)?;

    // Récupérer plus de résultats BM25 que nécessaire si on va re-scorer
    let fetch_k = opts.top_k * 3;
    let top_docs = searcher.search(&query, &TopDocs::with_limit(fetch_k))?;

    // Charger embeddings et calculer l'embedding de la query (best-effort)
    let query_embedding = embed_text(&opts.query, &opts.embed_provider);
    let stored_embeddings = if query_embedding.is_some() {
        load_embeddings(&opts.index_dir)
    } else {
        std::collections::HashMap::new()
    };

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

        // Re-scoring sémantique : 50% BM25 + 50% cosine si embeddings disponibles
        let score = if let Some(ref qvec) = query_embedding {
            let chunk_idx = line_start / 50;
            let key = format!("{file_path}:{chunk_idx}");
            if let Some(chunk_vec) = stored_embeddings.get(&key) {
                let sem_score = cosine_similarity(qvec, chunk_vec).max(0.0);
                0.5 * bm25_score + 0.5 * sem_score
            } else {
                bm25_score
            }
        } else {
            bm25_score
        };

        results.push(SearchResult { file_path, snippet, score, line_start });
    }

    // Trier par score décroissant et limiter à top_k
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(opts.top_k);

    Ok(results)
}
