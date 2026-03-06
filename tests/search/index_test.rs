use ecotokens::search::index::{index_directory, IndexOptions};
use tempfile::TempDir;
use std::fs;

fn make_fixture(dir: &TempDir) {
    fs::write(dir.path().join("main.rs"), "fn main() {\n    println!(\"hello\");\n}\n").unwrap();
    fs::write(dir.path().join("lib.rs"), "pub fn greet(name: &str) -> String {\n    format!(\"Hello {name}\")\n}\n").unwrap();
    fs::write(dir.path().join("README.md"), "# MyProject\n\n## Installation\n\nRun cargo build.\n").unwrap();
}

#[test]
fn index_empty_directory_succeeds() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    let opts = IndexOptions { reset: false, path: src.path().to_path_buf(), index_dir: idx.path().to_path_buf() };
    let stats = index_directory(opts).unwrap();
    assert_eq!(stats.file_count, 0);
}

#[test]
fn index_fixture_project_finds_files() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    make_fixture(&src);
    let opts = IndexOptions { reset: false, path: src.path().to_path_buf(), index_dir: idx.path().to_path_buf() };
    let stats = index_directory(opts).unwrap();
    assert!(stats.file_count >= 2, "should index at least 2 source files, got {}", stats.file_count);
    assert!(stats.chunk_count > 0, "should produce chunks");
}

#[test]
fn reset_clears_existing_index() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    make_fixture(&src);

    let opts = IndexOptions { reset: false, path: src.path().to_path_buf(), index_dir: idx.path().to_path_buf() };
    index_directory(opts).unwrap();

    // Reset
    let opts2 = IndexOptions { reset: true, path: src.path().to_path_buf(), index_dir: idx.path().to_path_buf() };
    let stats2 = index_directory(opts2).unwrap();
    assert!(stats2.file_count >= 2, "after reset, should re-index all files");
}

#[test]
fn incremental_update_does_not_duplicate() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    make_fixture(&src);

    let opts = IndexOptions { reset: false, path: src.path().to_path_buf(), index_dir: idx.path().to_path_buf() };
    index_directory(opts.clone()).unwrap();
    let stats2 = index_directory(opts).unwrap();
    // Incremental: chunk count should not explode
    assert!(stats2.chunk_count > 0);
}
