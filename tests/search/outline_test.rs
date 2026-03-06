use ecotokens::search::outline::{outline_path, OutlineOptions};
use std::fs;
use tempfile::TempDir;

fn make_fixture() -> TempDir {
    let src = TempDir::new().unwrap();
    fs::write(
        src.path().join("lib.rs"),
        "pub fn alpha() {}\npub struct Beta;\n",
    )
    .unwrap();
    fs::write(src.path().join("other.rs"), "pub fn gamma() {}\n").unwrap();
    src
}

// ── T047 ──────────────────────────────────────────────────────────────────────

#[test]
fn outline_single_file_sorted_by_line() {
    let src = make_fixture();
    let path = src.path().join("lib.rs");
    let opts = OutlineOptions { path, depth: None, kinds: None };
    let symbols = outline_path(opts).unwrap();
    assert!(!symbols.is_empty(), "should return symbols for lib.rs");
    let lines: Vec<u64> = symbols.iter().map(|s| s.line_start).collect();
    let mut sorted = lines.clone();
    sorted.sort_unstable();
    assert_eq!(lines, sorted, "symbols should be sorted by line_start");
}

#[test]
fn outline_directory_returns_symbols_from_all_files() {
    let src = make_fixture();
    let opts = OutlineOptions {
        path: src.path().to_path_buf(),
        depth: Some(1),
        kinds: None,
    };
    let symbols = outline_path(opts).unwrap();
    let files: std::collections::HashSet<&str> =
        symbols.iter().map(|s| s.file_path.as_str()).collect();
    assert!(
        files.len() >= 2,
        "should have symbols from at least 2 files, got: {files:?}"
    );
}

#[test]
fn outline_filter_kinds_fn_only() {
    let src = make_fixture();
    let opts = OutlineOptions {
        path: src.path().to_path_buf(),
        depth: None,
        kinds: Some(vec!["fn".to_string()]),
    };
    let symbols = outline_path(opts).unwrap();
    assert!(!symbols.is_empty(), "should find at least one fn symbol");
    for s in &symbols {
        assert_eq!(s.kind, "fn", "expected only fn symbols, got kind: {}", s.kind);
    }
}

#[test]
fn outline_empty_directory_returns_empty() {
    let src = TempDir::new().unwrap();
    let opts = OutlineOptions { path: src.path().to_path_buf(), depth: None, kinds: None };
    let symbols = outline_path(opts).unwrap();
    assert!(symbols.is_empty(), "empty dir should yield no symbols");
}
