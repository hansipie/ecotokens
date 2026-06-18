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
