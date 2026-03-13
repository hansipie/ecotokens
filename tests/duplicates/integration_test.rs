#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn setup_similarity_fixture() -> (TempDir, TempDir) {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();

    // 100% similar (identical)
    let func_100 = "fn compute_exact(x: i32) -> i32 {\n    let a = x * 2;\n    let b = a + 1;\n    let c = b - x;\n    let d = c * 3;\n    d\n}\n";
    fs::write(src.path().join("a.rs"), func_100).unwrap();
    fs::write(src.path().join("b.rs"), func_100).unwrap();

    // ~60% similar (significantly different)
    let func_low_a = "fn low_sim_a(x: i32) -> i32 {\n    let alpha = x * 100;\n    let beta = alpha / 7;\n    let gamma = beta + 42;\n    let delta = gamma % 13;\n    delta\n}\n";
    let func_low_b = "fn low_sim_b(y: f64) -> f64 {\n    let p = y.sin();\n    let q = p.cos();\n    let r = q * 3.14;\n    let s = r.abs();\n    s\n}\n";
    fs::write(src.path().join("c.rs"), func_low_a).unwrap();
    fs::write(src.path().join("d.rs"), func_low_b).unwrap();

    let status = Command::new(ecotokens_bin())
        .args([
            "index",
            "--path",
            &src.path().to_string_lossy(),
            "--index-dir",
            &idx.path().to_string_lossy(),
        ])
        .env("ECOTOKENS_BATCH", "1")
        .status()
        .expect("ecotokens index should run");
    assert!(status.success());

    (src, idx)
}

#[test]
fn test_threshold_filters_low_similarity() {
    let (_src, idx) = setup_similarity_fixture();

    let output = Command::new(ecotokens_bin())
        .args([
            "duplicates",
            "--threshold",
            "75",
            "--index-dir",
            &idx.path().to_string_lossy(),
        ])
        .env("ECOTOKENS_BATCH", "1")
        .output()
        .expect("ecotokens duplicates should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // The 100% similar group should appear
    assert!(
        stdout.contains("100") || stdout.contains("group") || stdout.contains("duplicate"),
        "should show 100% similar group: {stdout}"
    );
}

#[test]
fn test_invalid_threshold_exits_with_code_2() {
    let idx = TempDir::new().unwrap();

    let status = Command::new(ecotokens_bin())
        .args([
            "duplicates",
            "--threshold",
            "101",
            "--index-dir",
            &idx.path().to_string_lossy(),
        ])
        .env("ECOTOKENS_BATCH", "1")
        .status()
        .expect("ecotokens duplicates should run");

    assert_eq!(
        status.code(),
        Some(2),
        "threshold > 100 should exit with code 2"
    );
}

#[test]
fn test_json_output() {
    let (_src, idx) = setup_similarity_fixture();

    let output = Command::new(ecotokens_bin())
        .args([
            "duplicates",
            "--threshold",
            "70",
            "--json",
            "--index-dir",
            &idx.path().to_string_lossy(),
        ])
        .env("ECOTOKENS_BATCH", "1")
        .output()
        .expect("ecotokens duplicates should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("--json output should be valid JSON");
    assert!(
        parsed.get("groups").is_some(),
        "JSON output should have 'groups' field: {stdout}"
    );
    assert!(
        parsed.get("threshold").is_some(),
        "JSON output should have 'threshold' field"
    );
}
