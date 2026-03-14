#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn build_fixture_index(src: &Path, idx: &Path) {
    let status = std::process::Command::new(ecotokens_bin())
        .args([
            "index",
            "--path",
            &src.to_string_lossy(),
            "--index-dir",
            &idx.to_string_lossy(),
        ])
        .env("ECOTOKENS_BATCH", "1")
        .status()
        .expect("ecotokens index should run");
    assert!(status.success(), "ecotokens index failed");
}

#[test]
fn test_identical_functions_form_one_group() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();

    // Two identical 6-line functions in different files
    let func = "fn compute(x: i32) -> i32 {\n    let a = x * 2;\n    let b = a + 1;\n    let c = b - x;\n    let d = c * 3;\n    d\n}\n";
    fs::write(src.path().join("a.rs"), func).unwrap();
    fs::write(src.path().join("b.rs"), func).unwrap();

    build_fixture_index(src.path(), idx.path());

    let opts = ecotokens::duplicates::DetectionOptions {
        index_dir: idx.path().to_path_buf(),
        threshold: 70.0,
        min_lines: 5,
    };
    let groups = ecotokens::duplicates::detect::detect_duplicates(&opts).unwrap();
    assert_eq!(groups.len(), 1, "expected 1 duplicate group");
    assert!(
        (groups[0].similarity.value - 100.0).abs() < 1.0,
        "expected ~100% similarity"
    );
}

#[test]
fn test_short_symbol_excluded() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();

    // 3-line function — below min_lines=5
    let short = "fn tiny() -> i32 {\n    42\n}\n";
    fs::write(src.path().join("a.rs"), short).unwrap();
    fs::write(src.path().join("b.rs"), short).unwrap();

    build_fixture_index(src.path(), idx.path());

    let opts = ecotokens::duplicates::DetectionOptions {
        index_dir: idx.path().to_path_buf(),
        threshold: 70.0,
        min_lines: 5,
    };
    let groups = ecotokens::duplicates::detect::detect_duplicates(&opts).unwrap();
    assert!(groups.is_empty(), "short symbols should be excluded");
}

#[test]
fn test_no_duplicates() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();

    fs::write(
        src.path().join("a.rs"),
        "fn foo(x: i32) -> i32 {\n    let a = x + 1;\n    let b = a * 2;\n    let c = b - 3;\n    let d = c + 4;\n    d\n}\n",
    ).unwrap();
    fs::write(
        src.path().join("b.rs"),
        "fn bar(y: f64) -> f64 {\n    let p = y.sin();\n    let q = p.cos();\n    let r = q.tan();\n    let s = r.abs();\n    s\n}\n",
    ).unwrap();

    build_fixture_index(src.path(), idx.path());

    let opts = ecotokens::duplicates::DetectionOptions {
        index_dir: idx.path().to_path_buf(),
        threshold: 90.0,
        min_lines: 5,
    };
    let groups = ecotokens::duplicates::detect::detect_duplicates(&opts).unwrap();
    assert!(
        groups.is_empty(),
        "dissimilar functions should produce no groups"
    );
}

#[test]
fn test_union_find_merges_similar_group() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();

    // A≈B and A≈C → single merged group
    let fa = "fn process_data(x: i32) -> i32 {\n    let a = x * 2;\n    let b = a + 1;\n    let c = b - x;\n    let d = c * 3;\n    d\n}\n";
    let fb = "fn process_data_v2(x: i32) -> i32 {\n    let a = x * 2;\n    let b = a + 1;\n    let c = b - x;\n    let d = c * 3;\n    d\n}\n";
    let fc = "fn process_data_v3(x: i32) -> i32 {\n    let a = x * 2;\n    let b = a + 1;\n    let c = b - x;\n    let d = c * 3;\n    d\n}\n";
    fs::write(src.path().join("a.rs"), fa).unwrap();
    fs::write(src.path().join("b.rs"), fb).unwrap();
    fs::write(src.path().join("c.rs"), fc).unwrap();

    build_fixture_index(src.path(), idx.path());

    let opts = ecotokens::duplicates::DetectionOptions {
        index_dir: idx.path().to_path_buf(),
        threshold: 70.0,
        min_lines: 5,
    };
    let groups = ecotokens::duplicates::detect::detect_duplicates(&opts).unwrap();
    assert!(!groups.is_empty(), "should detect at least one group");
    // All 3 similar functions should be in the same group
    let total_segs: usize = groups.iter().map(|g| g.segments.len()).sum();
    assert_eq!(
        total_segs, 3,
        "all 3 similar functions should be grouped together"
    );
}

#[test]
fn test_single_file_zero_groups() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();

    fs::write(
        src.path().join("a.rs"),
        "fn only_one(x: i32) -> i32 {\n    let a = x * 2;\n    let b = a + 1;\n    let c = b - 1;\n    let d = c + 0;\n    d\n}\n",
    ).unwrap();

    build_fixture_index(src.path(), idx.path());

    let opts = ecotokens::duplicates::DetectionOptions {
        index_dir: idx.path().to_path_buf(),
        threshold: 70.0,
        min_lines: 5,
    };
    let groups = ecotokens::duplicates::detect::detect_duplicates(&opts).unwrap();
    assert_eq!(groups.len(), 0, "single symbol → 0 groups");
}
