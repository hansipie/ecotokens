use ecotokens::search::index::{open_or_create_index, IndexOptions};
use ecotokens::search::symbols::{lookup_symbol, parse_symbols, write_symbols};
use std::fs;
use tempfile::TempDir;

// ── T046 — parse_symbols ──────────────────────────────────────────────────────

#[test]
fn parse_rust_extracts_fn_and_struct() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("lib.rs");
    fs::write(
        &path,
        "pub fn greet(name: &str) -> String { format!(\"hi {name}\") }\npub struct Foo;\n",
    )
    .unwrap();
    let symbols = parse_symbols(&path).unwrap();
    assert!(!symbols.is_empty(), "should extract at least one symbol");
    let kinds: Vec<&str> = symbols.iter().map(|s| s.kind.as_str()).collect();
    assert!(kinds.contains(&"fn"), "should extract fn, got: {kinds:?}");
    assert!(
        kinds.contains(&"struct"),
        "should extract struct, got: {kinds:?}"
    );
}

#[test]
fn parse_empty_file_returns_empty() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("empty.rs");
    fs::write(&path, "").unwrap();
    let symbols = parse_symbols(&path).unwrap();
    assert!(symbols.is_empty(), "empty file should yield no symbols");
}

#[test]
fn parse_rust_extracts_impl() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("types.rs");
    fs::write(
        &path,
        "pub struct Bar;\nimpl Bar { pub fn new() -> Self { Bar } }\n",
    )
    .unwrap();
    let symbols = parse_symbols(&path).unwrap();
    let kinds: Vec<&str> = symbols.iter().map(|s| s.kind.as_str()).collect();
    assert!(
        kinds.contains(&"impl") || kinds.contains(&"fn"),
        "should extract impl or fn inside impl, got: {kinds:?}"
    );
}

#[test]
fn symbol_ids_are_stable() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("lib.rs");
    fs::write(&path, "pub fn hello() {}\n").unwrap();
    let s1 = parse_symbols(&path).unwrap();
    let s2 = parse_symbols(&path).unwrap();
    assert!(!s1.is_empty());
    assert_eq!(s1[0].id, s2[0].id, "IDs should be stable across calls");
}

#[test]
fn symbol_id_contains_name_and_kind() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("foo.rs");
    fs::write(&path, "pub fn do_thing() {}\n").unwrap();
    let symbols = parse_symbols(&path).unwrap();
    let fn_sym = symbols.iter().find(|s| s.kind == "fn").unwrap();
    assert!(
        fn_sym.id.contains("do_thing"),
        "ID should contain function name, got: {}",
        fn_sym.id
    );
    assert!(
        fn_sym.id.contains("fn"),
        "ID should contain kind 'fn', got: {}",
        fn_sym.id
    );
}

// ── T048 — lookup_symbol ──────────────────────────────────────────────────────

#[test]
fn lookup_invalid_id_returns_none_or_error() {
    let idx = TempDir::new().unwrap();
    let result = lookup_symbol("nonexistent::ghost#fn", idx.path());
    match result {
        Ok(None) => {}
        Ok(Some(_)) => panic!("should not return a snippet for unknown id"),
        Err(_) => {} // acceptable for an empty/missing index
    }
}

#[test]
fn lookup_valid_id_returns_source_snippet() {
    let dir = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    let path = dir.path().join("lib.rs");
    fs::write(&path, "pub fn compute(x: u32) -> u32 { x * 2 }\n").unwrap();

    let symbols = parse_symbols(&path).unwrap();
    assert!(!symbols.is_empty(), "parse_symbols should find fn compute");

    // Index the symbols so lookup_symbol can find them
    let opts = IndexOptions {
        reset: false,
        path: dir.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
        progress: None,
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
    };
    let index = open_or_create_index(&opts.index_dir, opts.reset).unwrap();
    let mut writer = index.writer(15_000_000).unwrap();
    write_symbols(&symbols, &mut writer).unwrap();
    writer.commit().unwrap();

    let sym = symbols.iter().find(|s| s.kind == "fn").unwrap();
    let snippet = lookup_symbol(&sym.id, idx.path()).unwrap();
    assert!(
        snippet.is_some(),
        "should return source for valid id after indexing"
    );
    assert!(
        snippet.unwrap().contains("compute"),
        "snippet should contain the function name"
    );
}
