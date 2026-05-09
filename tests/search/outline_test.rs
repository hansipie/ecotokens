use ecotokens::search::index::{index_directory, IndexOptions};
use ecotokens::search::outline::{outline_path, OutlineOptions};
use ecotokens::search::symbols::lookup_symbol;
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
    let opts = OutlineOptions {
        path,
        depth: None,
        kinds: None,
        base: None,
    };
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
        base: None,
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
        base: None,
    };
    let symbols = outline_path(opts).unwrap();
    assert!(!symbols.is_empty(), "should find at least one fn symbol");
    for s in &symbols {
        assert_eq!(
            s.kind, "fn",
            "expected only fn symbols, got kind: {}",
            s.kind
        );
    }
}

#[test]
fn outline_empty_directory_returns_empty() {
    let src = TempDir::new().unwrap();
    let opts = OutlineOptions {
        path: src.path().to_path_buf(),
        depth: None,
        kinds: None,
        base: None,
    };
    let symbols = outline_path(opts).unwrap();
    assert!(symbols.is_empty(), "empty dir should yield no symbols");
}

// ── IDs cohérents entre outline et index ──────────────────────────────────────

/// Reproduit le bug CLI : `outline .` retourne des IDs avec préfixe "./" alors que
/// l'index stocke les chemins sans ce préfixe → lookup_symbol retourne None.
#[test]
fn outline_dot_ids_have_no_dot_slash_prefix() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("lib.rs"), "pub fn hello() {}\n").unwrap();

    let opts = OutlineOptions {
        path: root.path().to_path_buf(),
        depth: Some(1),
        kinds: None,
        base: Some(root.path().to_path_buf()),
    };
    let symbols = outline_path(opts).unwrap();
    assert!(!symbols.is_empty(), "should find symbols");

    for sym in &symbols {
        assert!(
            !sym.id.starts_with("./"),
            "ID ne doit pas commencer par './' : '{}'",
            sym.id
        );
        assert!(
            !sym.file_path.starts_with("./"),
            "file_path ne doit pas commencer par './' : '{}'",
            sym.file_path
        );
    }
}

/// Reproduit le bug MCP : outline reçoit un chemin absolu vers un fichier hors du cwd,
/// base: None → strip_prefix(cwd) échoue → l'ID retourné est le chemin absolu complet,
/// alors que l'index stocke un chemin relatif → lookup_symbol retourne None.
#[test]
fn outline_without_base_absolute_path_id_mismatch() {
    // Fixture dans /tmp (hors du cwd du processus de test = racine du projet)
    let root = TempDir::new().unwrap();
    let src_dir = root.path().join("src");
    fs::create_dir(&src_dir).unwrap();
    fs::write(
        src_dir.join("lib.rs"),
        "pub fn compute(x: u32) -> u32 { x * 2 }\n",
    )
    .unwrap();

    // Indexer avec project_root comme base (comme le fait `ecotokens index`)
    let idx = TempDir::new().unwrap();
    index_directory(IndexOptions {
        reset: true,
        path: root.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
        progress: None,
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
        log_tx: None,
    })
    .unwrap();

    // outline sans base (cas MCP) avec un chemin absolu hors du cwd
    let opts = OutlineOptions {
        path: src_dir.join("lib.rs"), // chemin absolu dans /tmp
        depth: None,
        kinds: None,
        base: None, // cwd = racine du projet ecotokens, pas root
    };
    let symbols = outline_path(opts).unwrap();
    assert!(!symbols.is_empty(), "outline should find symbols");

    // Les IDs contiennent le chemin absolu au lieu du chemin relatif
    let sym = symbols.iter().find(|s| s.kind == "fn").unwrap();
    assert!(
        sym.id.starts_with('/'),
        "sans base correcte, l'ID devrait être absolu (bug): got '{}'",
        sym.id
    );

    // Conséquence : lookup_symbol ne trouve pas le symbole
    let result = lookup_symbol(&sym.id, idx.path()).unwrap();
    assert!(
        result.is_none(),
        "lookup avec un ID absolu ne devrait rien trouver dans l'index (bug démontré)"
    );
}

#[test]
fn outline_ids_use_relative_path_for_nested_file() {
    // Fixture : project_root/subdir/foo.rs
    let root = TempDir::new().unwrap();
    let subdir = root.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    fs::write(subdir.join("foo.rs"), "pub fn bar() {}\n").unwrap();

    let opts = OutlineOptions {
        path: subdir.join("foo.rs"),
        depth: None,
        kinds: None,
        base: Some(root.path().to_path_buf()),
    };
    let symbols = outline_path(opts).unwrap();
    let sym = symbols.iter().find(|s| s.name == "bar").unwrap();

    assert_eq!(sym.file_path, "subdir/foo.rs");
    assert_eq!(sym.id, "subdir/foo.rs::bar#fn");
}

#[test]
fn outline_ids_match_index_lookup() {
    // Fixture : project_root/src/lib.rs
    let root = TempDir::new().unwrap();
    let src_dir = root.path().join("src");
    fs::create_dir(&src_dir).unwrap();
    fs::write(
        src_dir.join("lib.rs"),
        "pub fn compute(x: u32) -> u32 { x * 2 }\n",
    )
    .unwrap();

    // Indexer avec project_root comme base
    let idx = TempDir::new().unwrap();
    index_directory(IndexOptions {
        reset: true,
        path: root.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
        progress: None,
        embed_provider: ecotokens::config::settings::EmbedProvider::None,
        log_tx: None,
    })
    .unwrap();

    // outline avec la même base
    let opts = OutlineOptions {
        path: src_dir.join("lib.rs"),
        depth: None,
        kinds: None,
        base: Some(root.path().to_path_buf()),
    };
    let symbols = outline_path(opts).unwrap();
    assert!(!symbols.is_empty(), "outline should find symbols");

    // Chaque ID retourné par outline doit être trouvable dans l'index
    for sym in &symbols {
        let result = lookup_symbol(&sym.id, idx.path()).unwrap();
        assert!(
            result.is_some(),
            "symbol '{}' from outline not found in index",
            sym.id
        );
    }
}
