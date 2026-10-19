use ecotokens::search::index::{index_directory, IndexOptions};
use std::fs;
use tempfile::TempDir;

fn make_fixture(dir: &TempDir) {
    fs::write(
        dir.path().join("main.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("lib.rs"),
        "pub fn greet(name: &str) -> String {\n    format!(\"Hello {name}\")\n}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("README.md"),
        "# MyProject\n\n## Installation\n\nRun cargo build.\n",
    )
    .unwrap();
}

#[test]
fn index_empty_directory_succeeds() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    let opts = IndexOptions {
        reset: false,
        path: src.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
        progress: None,
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
        log_tx: None,
    };
    let stats = index_directory(opts).unwrap();
    assert_eq!(stats.file_count, 0);
}

#[test]
fn index_fixture_project_finds_files() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    make_fixture(&src);
    let opts = IndexOptions {
        reset: false,
        path: src.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
        progress: None,
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
        log_tx: None,
    };
    let stats = index_directory(opts).unwrap();
    assert!(
        stats.file_count >= 2,
        "should index at least 2 source files, got {}",
        stats.file_count
    );
    assert!(stats.chunk_count > 0, "should produce chunks");
}

#[test]
fn reset_clears_existing_index() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    make_fixture(&src);

    let opts = IndexOptions {
        reset: false,
        path: src.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
        progress: None,
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
        log_tx: None,
    };
    index_directory(opts).unwrap();

    // Reset
    let opts2 = IndexOptions {
        reset: true,
        path: src.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
        progress: None,
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
        log_tx: None,
    };
    let stats2 = index_directory(opts2).unwrap();
    assert!(
        stats2.file_count >= 2,
        "after reset, should re-index all files"
    );
}

#[test]
fn incremental_update_does_not_duplicate() {
    use std::thread;
    use std::time::Duration;

    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    make_fixture(&src);

    let opts = IndexOptions {
        reset: false,
        path: src.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
        progress: None,
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
        log_tx: None,
    };
    let stats1 = index_directory(opts.clone()).unwrap();
    assert!(stats1.file_count >= 2, "first pass should index files");

    // Modify one file to trigger re-indexing (wait to ensure mtime changes)
    thread::sleep(Duration::from_millis(1100));
    fs::write(
        src.path().join("main.rs"),
        "fn main() {\n    println!(\"updated\");\n}\n",
    )
    .unwrap();

    let stats2 = index_directory(opts).unwrap();
    // Incremental: should only have re-indexed the modified file
    assert_eq!(
        stats2.file_count, 1,
        "incremental should process only changed files"
    );
    assert!(stats2.chunk_count > 0, "changed file should produce chunks");
}

#[test]
fn incremental_update_prunes_stale_vectors_for_changed_file() {
    use ecotokens::search::hnsw::HnswIndex;
    use std::thread;
    use std::time::Duration;

    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    let file = src.path().join("main.rs");
    fs::write(&file, "fn old_name() {}\n").unwrap();

    let opts = IndexOptions {
        reset: false,
        path: src.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
        progress: None,
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
        log_tx: None,
    };
    index_directory(opts.clone()).unwrap();

    let stale_vectors = vec![("main.rs::old_name".to_string(), vec![1.0_f32, 0.0])];
    HnswIndex::build(&stale_vectors).save(idx.path()).unwrap();

    thread::sleep(Duration::from_millis(1100));
    fs::write(&file, "fn new_name() {}\n").unwrap();

    let stats = index_directory(opts).unwrap();

    assert_eq!(stats.vector_count, 0);
    assert!(
        HnswIndex::load(idx.path()).is_none(),
        "stale vectors for a changed file must not survive incremental reindex"
    );
}

#[test]
fn incremental_update_prunes_stale_vectors_when_manifest_is_missing() {
    use ecotokens::search::hnsw::HnswIndex;

    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    fs::write(src.path().join("main.rs"), "fn keep() {}\n").unwrap();

    let opts = IndexOptions {
        reset: false,
        path: src.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
        progress: None,
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
        log_tx: None,
    };
    index_directory(opts.clone()).unwrap();
    fs::remove_file(idx.path().join("semantic_manifest.json")).unwrap();

    let vectors = vec![
        ("main.rs::keep#fn".to_string(), vec![1.0_f32, 0.0]),
        ("main.rs::stale#fn".to_string(), vec![0.0_f32, 1.0]),
    ];
    HnswIndex::build(&vectors).save(idx.path()).unwrap();

    let stats = index_directory(opts).unwrap();
    let embeddings = HnswIndex::load(idx.path()).unwrap().to_embeddings();

    assert_eq!(stats.vector_count, 1);
    assert!(embeddings.contains_key("main.rs::keep#fn"));
    assert!(!embeddings.contains_key("main.rs::stale#fn"));
}

#[test]
fn incremental_update_prunes_vectors_for_deleted_file() {
    use ecotokens::search::hnsw::HnswIndex;

    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    fs::write(src.path().join("a.rs"), "fn alpha() {}\n").unwrap();
    fs::write(src.path().join("b.rs"), "fn beta() {}\n").unwrap();

    let opts = IndexOptions {
        reset: false,
        path: src.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
        progress: None,
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
        log_tx: None,
    };
    index_directory(opts.clone()).unwrap();

    let vectors = vec![
        ("a.rs::alpha#fn".to_string(), vec![1.0_f32, 0.0]),
        ("b.rs::beta#fn".to_string(), vec![0.0_f32, 1.0]),
    ];
    HnswIndex::build(&vectors).save(idx.path()).unwrap();
    fs::remove_file(src.path().join("b.rs")).unwrap();

    let stats = index_directory(opts).unwrap();
    let embeddings = HnswIndex::load(idx.path()).unwrap().to_embeddings();
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(idx.path().join("semantic_manifest.json")).unwrap(),
    )
    .unwrap();

    assert_eq!(stats.vector_count, 1);
    assert!(embeddings.contains_key("a.rs::alpha#fn"));
    assert!(!embeddings.contains_key("b.rs::beta#fn"));
    assert!(manifest["files"]["a.rs"].is_object());
    assert!(manifest["files"].get("b.rs").is_none());
}

#[test]
fn incremental_update_removes_legacy_embeddings_when_no_vectors_remain() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    fs::write(src.path().join("main.rs"), "fn main() {}\n").unwrap();

    let legacy = serde_json::json!({
        "deleted.rs::gone#fn": [1.0_f32, 0.0]
    });
    fs::write(
        idx.path().join("embeddings.json"),
        serde_json::to_string(&legacy).unwrap(),
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
    let stats = index_directory(opts).unwrap();

    assert_eq!(stats.vector_count, 0);
    assert!(!idx.path().join("hnsw_index.bin").exists());
    assert!(!idx.path().join("hnsw_meta.json").exists());
    assert!(!idx.path().join("embeddings.json").exists());
}

#[test]
fn incremental_update_removes_legacy_embeddings_after_successful_hnsw_save() {
    use ecotokens::search::hnsw::HnswIndex;

    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    fs::write(src.path().join("main.rs"), "fn keep() {}\n").unwrap();

    let opts = IndexOptions {
        reset: false,
        path: src.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
        progress: None,
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
        log_tx: None,
    };
    index_directory(opts.clone()).unwrap();

    let legacy = serde_json::json!({
        "main.rs::keep#fn": [1.0_f32, 0.0]
    });
    fs::write(
        idx.path().join("embeddings.json"),
        serde_json::to_string(&legacy).unwrap(),
    )
    .unwrap();

    let stats = index_directory(opts).unwrap();

    assert_eq!(stats.vector_count, 1);
    assert!(HnswIndex::load(idx.path()).is_some());
    assert!(!idx.path().join("embeddings.json").exists());
}

#[test]
fn incremental_update_with_manifest_keeps_vector_count_stable() {
    use ecotokens::search::hnsw::HnswIndex;

    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    fs::write(src.path().join("main.rs"), "fn keep() {}\n").unwrap();

    let opts = IndexOptions {
        reset: false,
        path: src.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
        progress: None,
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
        log_tx: None,
    };
    index_directory(opts.clone()).unwrap();

    let vectors = vec![("main.rs::keep#fn".to_string(), vec![1.0_f32, 0.0])];
    HnswIndex::build(&vectors).save(idx.path()).unwrap();

    let stats = index_directory(opts).unwrap();
    let embeddings = HnswIndex::load(idx.path()).unwrap().to_embeddings();

    assert_eq!(stats.file_count, 0);
    assert_eq!(stats.vector_count, 1);
    assert!(embeddings.contains_key("main.rs::keep#fn"));
}

#[test]
fn chunks_the_embedding_provider_cannot_embed_are_counted() {
    use ecotokens::config::settings::EmbedProvider;

    let src = TempDir::new().unwrap();
    make_fixture(&src);
    let index = |embed_provider| {
        let idx = TempDir::new().unwrap();
        index_directory(IndexOptions {
            reset: true,
            path: src.path().to_path_buf(),
            index_dir: idx.path().to_path_buf(),
            progress: None,
            embed_provider,
            log_tx: None,
        })
        .unwrap()
    };

    // Nothing listens on port 1: every embedding request fails.
    let stats = index(EmbedProvider::Ollama {
        url: "http://127.0.0.1:1".to_string(),
        model: "m".to_string(),
    });
    assert!(stats.chunk_count > 0);
    assert_eq!(stats.vector_count, 0);
    assert_eq!(stats.embed_failed_count, stats.chunk_count);

    // Without a provider there is nothing to fail.
    assert_eq!(index(EmbedProvider::None).embed_failed_count, 0);
}

/// Segment ids that `meta.json` still references, as 32-char hex file prefixes.
fn live_segment_ids(index_dir: &std::path::Path) -> std::collections::HashSet<String> {
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(index_dir.join("meta.json")).unwrap()).unwrap();
    meta["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["segment_id"].as_str().unwrap().replace('-', ""))
        .collect()
}

#[test]
fn reindexing_leaves_no_orphan_segment_files() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    make_fixture(&src);

    for round in 0..4 {
        // The index stores mtimes at one-second resolution.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(
            src.path().join("main.rs"),
            format!("fn main() {{ let round = {round}; }}\n"),
        )
        .unwrap();
        index_directory(IndexOptions {
            reset: false,
            path: src.path().to_path_buf(),
            index_dir: idx.path().to_path_buf(),
            progress: None,
            embed_provider: ecotokens::config::settings::EmbedProvider::None,
            log_tx: None,
        })
        .unwrap();
    }

    let live = live_segment_ids(idx.path());
    let orphans: Vec<String> = fs::read_dir(idx.path())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.len() > 33 && n.as_bytes()[32] == b'.')
        .filter(|n| !live.contains(&n[..32]))
        .collect();
    assert!(
        orphans.is_empty(),
        "segment files no longer referenced by meta.json: {orphans:?}"
    );
}
