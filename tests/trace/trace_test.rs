use ecotokens::search::index::{index_directory, IndexOptions};
use ecotokens::trace::callees::find_callees;
use ecotokens::trace::callers::find_callers;
use std::fs;
use tempfile::TempDir;

// Helper: create a Rust fixture project with caller/callee relationships and index it.
fn setup_indexed_fixture() -> (TempDir, TempDir) {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();

    // File A: defines helper() and compute()
    fs::write(
        src.path().join("helpers.rs"),
        r#"
pub fn helper() -> u32 {
    42
}

pub fn compute(x: u32) -> u32 {
    x * 2
}
"#,
    )
    .unwrap();

    // File B: main() calls helper() and compute()
    fs::write(
        src.path().join("main.rs"),
        r#"
pub fn main() {
    let a = helper();
    let b = compute(a);
    println!("{b}");
}

pub fn unused() {
    // calls nothing relevant
}
"#,
    )
    .unwrap();

    // File C: orchestrator() calls helper() — second caller
    fs::write(
        src.path().join("orchestrator.rs"),
        r#"
pub fn orchestrator() {
    let _ = helper();
    let _ = helper();
}
"#,
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
    index_directory(opts).expect("indexing should succeed");

    (src, idx)
}

// ── T053 — find_callers ──────────────────────────────────────────────────────

#[test]
fn callers_of_helper_returns_two_callers() {
    let (_src, idx) = setup_indexed_fixture();
    let edges = find_callers("helper", idx.path()).unwrap();
    assert!(
        edges.len() >= 2,
        "helper() is called by main() and orchestrator(), got {} callers: {:?}",
        edges.len(),
        edges,
    );
}

#[test]
fn callers_of_unknown_symbol_returns_empty() {
    let (_src, idx) = setup_indexed_fixture();
    let edges = find_callers("nonexistent_symbol_xyz", idx.path());
    match edges {
        Ok(v) => assert!(v.is_empty(), "unknown symbol should have no callers"),
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("not found") || msg.contains("symbol"),
                "error should mention symbol not found, got: {msg}"
            );
        }
    }
}

#[test]
fn callers_of_unused_returns_empty() {
    let (_src, idx) = setup_indexed_fixture();
    let edges = find_callers("unused", idx.path()).unwrap();
    assert!(
        edges.is_empty(),
        "unused() has no callers, got: {:?}",
        edges
    );
}

#[test]
fn caller_edges_contain_file_and_line() {
    let (_src, idx) = setup_indexed_fixture();
    let edges = find_callers("helper", idx.path()).unwrap();
    assert!(!edges.is_empty());
    for edge in &edges {
        assert!(!edge.file_path.is_empty(), "file_path should be non-empty");
        assert!(!edge.name.is_empty(), "caller name should be non-empty");
    }
}

// ── T054 — find_callees ──────────────────────────────────────────────────────

#[test]
fn callees_of_main_returns_helper_and_compute() {
    let (_src, idx) = setup_indexed_fixture();
    let edges = find_callees("main", idx.path(), 1).unwrap();
    let names: Vec<&str> = edges.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"helper"),
        "main() calls helper(), got: {names:?}"
    );
    assert!(
        names.contains(&"compute"),
        "main() calls compute(), got: {names:?}"
    );
}

#[test]
fn callees_depth_2_returns_transitive() {
    let (_src, idx) = setup_indexed_fixture();
    // orchestrator() → helper() → nothing, depth 2 should still work
    let edges = find_callees("orchestrator", idx.path(), 2).unwrap();
    let names: Vec<&str> = edges.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"helper"),
        "orchestrator() calls helper(), got: {names:?}"
    );
}

#[test]
fn callees_of_leaf_returns_empty() {
    let (_src, idx) = setup_indexed_fixture();
    // helper() doesn't call any known symbol
    let edges = find_callees("helper", idx.path(), 1).unwrap();
    // helper returns 42, no calls to indexed functions
    assert!(
        edges.is_empty(),
        "helper() calls no indexed symbol, got: {:?}",
        edges
    );
}

#[test]
fn callees_unknown_symbol_returns_empty_or_error() {
    let (_src, idx) = setup_indexed_fixture();
    let edges = find_callees("ghost_fn", idx.path(), 1);
    match edges {
        Ok(v) => assert!(v.is_empty()),
        Err(e) => {
            let msg = e.to_string();
            assert!(msg.contains("not found") || msg.contains("symbol"));
        }
    }
}
