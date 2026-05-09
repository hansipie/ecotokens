use ecotokens::search::index::{index_directory, IndexOptions};
use ecotokens::search::query::{search_index, SearchOptions};
use std::fs;
use tempfile::TempDir;

fn build_fixture_index() -> (TempDir, TempDir) {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    fs::write(
        src.path().join("auth.rs"),
        "/// Handle user authentication\npub fn authenticate(user: &str, pass: &str) -> bool {\n    user == \"admin\"\n}\n",
    ).unwrap();
    fs::write(
        src.path().join("db.rs"),
        "/// Database connection pool\npub struct DbPool {\n    url: String,\n}\n",
    )
    .unwrap();
    fs::write(
        src.path().join("README.md"),
        "# Project\n\n## Authentication\n\nThe system uses token-based auth.\n",
    )
    .unwrap();
    let opts = IndexOptions {
        reset: false,
        path: src.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
        progress: None,
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
        log_tx: None,
    };
    index_directory(opts).unwrap();
    (src, idx)
}

#[test]
fn query_returns_relevant_file_in_top_results() {
    let (_src, idx) = build_fixture_index();
    let opts = SearchOptions {
        query: "authentication".to_string(),
        top_k: 3,
        index_dir: idx.path().to_path_buf(),
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
    };
    let results = search_index(opts).unwrap();
    assert!(!results.is_empty(), "should return results");
    let found_auth = results.iter().any(|r| {
        r.file_path.to_lowercase().contains("auth") || r.snippet.to_lowercase().contains("auth")
    });
    assert!(
        found_auth,
        "auth.rs or auth-related content should rank highly for 'authentication'"
    );
}

#[test]
fn unindexed_directory_returns_error() {
    let idx = TempDir::new().unwrap();
    let opts = SearchOptions {
        query: "anything".to_string(),
        top_k: 3,
        index_dir: idx.path().to_path_buf(),
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
    };
    let result = search_index(opts);
    assert!(
        result.is_err() || result.unwrap().is_empty(),
        "unindexed dir should return error or empty"
    );
}

#[test]
fn results_include_file_path_and_snippet() {
    let (_src, idx) = build_fixture_index();
    let opts = SearchOptions {
        query: "database connection".to_string(),
        top_k: 3,
        index_dir: idx.path().to_path_buf(),
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
    };
    let results = search_index(opts).unwrap();
    if !results.is_empty() {
        assert!(
            !results[0].file_path.is_empty(),
            "result should have file_path"
        );
        assert!(!results[0].snippet.is_empty(), "result should have snippet");
        assert!(results[0].score >= 0.0, "score should be non-negative");
    }
}

#[test]
fn search_returns_only_bm25_chunks() {
    let (_src, idx) = build_fixture_index();
    let opts = SearchOptions {
        query: "authenticate".to_string(),
        top_k: 10,
        index_dir: idx.path().to_path_buf(),
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
    };
    let results = search_index(opts).unwrap();
    assert!(!results.is_empty(), "should return results");
    assert!(
        results
            .iter()
            .all(|r| r.retrieval_source == ecotokens::search::query::RetrievalSource::Bm25),
        "with EmbedProvider::None, all results should have retrieval_source=Bm25"
    );
}
