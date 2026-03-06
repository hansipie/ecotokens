use ecotokens::search::text_docs::index_text_doc;
use std::fs;
use tempfile::TempDir;

// ── T048b ─────────────────────────────────────────────────────────────────────

#[test]
fn readme_headings_extracted_as_symbols() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("README.md");
    fs::write(
        &path,
        "# Project\n\n## Installation\n\nRun cargo install.\n\n## Usage\n\nSee docs.\n",
    )
    .unwrap();
    let symbols = index_text_doc(&path, "README.md").unwrap();
    assert!(!symbols.is_empty(), "should extract heading symbols");
    let kinds: Vec<&str> = symbols.iter().map(|s| s.kind.as_str()).collect();
    assert!(
        kinds.contains(&"h1") || kinds.contains(&"h2"),
        "should have h1/h2 kinds, got: {kinds:?}"
    );
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.iter().any(|n| n.contains("Installation")),
        "should extract 'Installation' heading, got: {names:?}"
    );
}

#[test]
fn cargo_toml_extracts_table_sections() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Cargo.toml");
    fs::write(
        &path,
        "[package]\nname = \"foo\"\n\n[dependencies]\nserde = \"1\"\n",
    )
    .unwrap();
    let symbols = index_text_doc(&path, "Cargo.toml").unwrap();
    assert!(!symbols.is_empty(), "should extract TOML table sections");
    let kinds: Vec<&str> = symbols.iter().map(|s| s.kind.as_str()).collect();
    assert!(kinds.contains(&"table"), "should have 'table' kind, got: {kinds:?}");
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.iter().any(|n| *n == "package" || *n == "dependencies"),
        "should extract table names, got: {names:?}"
    );
}

#[test]
fn empty_file_returns_empty_symbols() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("empty.md");
    fs::write(&path, "").unwrap();
    let symbols = index_text_doc(&path, "empty.md").unwrap();
    assert!(symbols.is_empty(), "empty file should yield no symbols");
}

#[test]
fn json_file_extracts_root_keys() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.json");
    fs::write(&path, r#"{"name": "foo", "version": "1.0", "debug": true}"#).unwrap();
    let symbols = index_text_doc(&path, "config.json").unwrap();
    assert!(!symbols.is_empty(), "should extract JSON root keys");
    let kinds: Vec<&str> = symbols.iter().map(|s| s.kind.as_str()).collect();
    assert!(kinds.contains(&"key"), "should have 'key' kind, got: {kinds:?}");
}
