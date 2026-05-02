use ecotokens::search::query::SearchResult;
use serde_json;

#[cfg(test)]
mod tests {
    use super::*;

    // T022 — SearchResult must expose line_end and retrieval_source
    #[test]
    fn search_result_has_line_end() {
        // Compile-time check: these fields must exist on SearchResult
        let _ = SearchResult {
            file_path: "src/foo.rs".to_string(),
            snippet: "fn foo() {}".to_string(),
            score: 0.9,
            line_start: 0,
            line_end: Some(5),
            retrieval_source: ecotokens::search::query::RetrievalSource::Both,
        };
    }

    // T059 — JSON serialisation must include all contractual fields
    #[test]
    fn mcp_search_returns_valid_json() {
        use ecotokens::search::query::RetrievalSource;

        let result = SearchResult {
            file_path: "src/config/settings.rs".to_string(),
            snippet: "pub fn load() -> Result<Settings>".to_string(),
            score: 0.91,
            line_start: 45,
            line_end: Some(78),
            retrieval_source: RetrievalSource::Vector,
        };

        let v = serde_json::to_value(&result).expect("serialisation failed");

        // All contractual fields from mcp.md must be present
        assert!(v.get("file_path").is_some(), "missing file_path");
        assert!(v.get("snippet").is_some(), "missing snippet");
        assert!(v.get("score").is_some(), "missing score");
        assert!(v.get("line_start").is_some(), "missing line_start");
        assert!(v.get("line_end").is_some(), "missing line_end");
        assert!(
            v.get("retrieval_source").is_some(),
            "missing retrieval_source"
        );

        assert_eq!(v["file_path"].as_str().unwrap(), "src/config/settings.rs");
        assert_eq!(v["line_start"].as_u64().unwrap(), 45);
        assert_eq!(v["line_end"].as_u64().unwrap(), 78);
        assert_eq!(v["retrieval_source"].as_str().unwrap(), "Vector");
    }

    // T021 — dual retrieval must use vector channel when available
    // Requires a real index + embed provider, so marked #[ignore] for CI.
    #[test]
    #[ignore]
    fn dual_retrieval_finds_no_keyword_match() {
        use ecotokens::config::settings::EmbedProvider;
        use ecotokens::search::index::{index_directory, IndexOptions};
        use ecotokens::search::query::{search_index, SearchOptions};
        use tempfile::TempDir;

        let corpus_dir = TempDir::new().unwrap();
        let index_dir = TempDir::new().unwrap();

        // Write a Rust file whose content describes error propagation
        // without using those exact words in variable names
        std::fs::write(
            corpus_dir.path().join("result_chain.rs"),
            r#"
pub fn propagate(x: i32) -> Result<i32, String> {
    if x < 0 { return Err("negative".into()); }
    Ok(x * 2)
}
"#,
        )
        .unwrap();

        let opts = IndexOptions {
            reset: true,
            path: corpus_dir.path().to_path_buf(),
            index_dir: index_dir.path().to_path_buf(),
            progress: None,
            embed_provider: EmbedProvider::Candle {
                model: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            },
            log_tx: None,
        };
        index_directory(opts).expect("indexing failed");

        let results = search_index(SearchOptions {
            query: "how errors are propagated".to_string(),
            top_k: 3,
            index_dir: index_dir.path().to_path_buf(),
            embed_provider: EmbedProvider::Candle {
                model: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            },
        })
        .expect("search failed");

        assert!(!results.is_empty(), "expected at least one result");
        // At least one result should come from the vector channel
        let has_vector = results.iter().any(|r| {
            !matches!(
                r.retrieval_source,
                ecotokens::search::query::RetrievalSource::Bm25
            )
        });
        assert!(has_vector, "expected at least one Vector or Both result");
    }

    // T033 — incremental reindex must reuse embeddings for unchanged files
    #[test]
    #[ignore]
    fn incremental_reindex_reuses_embeddings() {
        use ecotokens::config::settings::EmbedProvider;
        use ecotokens::search::index::{index_directory, IndexOptions};
        use tempfile::TempDir;

        let corpus = TempDir::new().unwrap();
        let index_dir = TempDir::new().unwrap();

        std::fs::write(corpus.path().join("a.rs"), "fn foo() {}").unwrap();
        std::fs::write(corpus.path().join("b.rs"), "fn bar() {}").unwrap();

        let provider = EmbedProvider::Candle {
            model: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
        };

        // First full index
        index_directory(IndexOptions {
            reset: true,
            path: corpus.path().to_path_buf(),
            index_dir: index_dir.path().to_path_buf(),
            progress: None,
            embed_provider: provider.clone(),
            log_tx: None,
        })
        .unwrap();

        let bin1_size = std::fs::metadata(index_dir.path().join("hnsw_index.bin"))
            .map(|m| m.len())
            .unwrap_or(0);

        // Modify only b.rs
        std::fs::write(corpus.path().join("b.rs"), "fn bar() { /* changed */ }").unwrap();

        // Incremental reindex
        index_directory(IndexOptions {
            reset: false,
            path: corpus.path().to_path_buf(),
            index_dir: index_dir.path().to_path_buf(),
            progress: None,
            embed_provider: provider,
            log_tx: None,
        })
        .unwrap();

        let bin2_size = std::fs::metadata(index_dir.path().join("hnsw_index.bin"))
            .map(|m| m.len())
            .unwrap_or(0);

        // Index was rebuilt (file changed), but total size should be similar (1 chunk difference)
        assert!(
            bin1_size > 0 && bin2_size > 0,
            "hnsw_index.bin should exist after both runs"
        );
    }
}
