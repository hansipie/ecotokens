use std::process::Command;
use tempfile::TempDir;

mod helpers;
use helpers::ecotokens;

fn temp_home_with_empty_metrics() -> TempDir {
    let dir = TempDir::new().unwrap();
    let config_dir = dir.path().join(".config").join("ecotokens");
    std::fs::create_dir_all(&config_dir).unwrap();
    // Create empty metrics file so metrics_path() resolves and read_from() returns []
    std::fs::write(config_dir.join("metrics.jsonl"), "").unwrap();
    dir
}

fn temp_claude_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
    dir
}

// ── T040 — end-to-end scenario ────────────────────────────────────────────────

#[test]
fn install_then_uninstall_is_clean() {
    let home = temp_claude_dir();
    let settings = home.path().join(".claude").join("settings.json");

    // Install
    let out = Command::new(ecotokens())
        .args(["install"])
        .env("HOME", home.path())
        .output()
        .expect("failed to run install");
    assert!(
        out.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        settings.exists(),
        "settings.json should be created after install"
    );

    // Verify hook is present
    let content = std::fs::read_to_string(&settings).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(
        v["hooks"]["PreToolUse"].is_array(),
        "PreToolUse hooks should be present"
    );

    // Uninstall
    let out = Command::new(ecotokens())
        .args(["uninstall"])
        .env("HOME", home.path())
        .output()
        .expect("failed to run uninstall");
    assert!(
        out.status.success(),
        "uninstall failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn filter_large_output_reduces_size() {
    // Generic filter triggers at 200 lines — create 300-line fixture
    let tmp = TempDir::new().unwrap();
    let input_file = tmp.path().join("large_output.txt");
    let lines: Vec<String> = (0..300)
        .map(|i| format!("line {i}: some content that takes space"))
        .collect();
    let input = lines.join("\n");
    std::fs::write(&input_file, &input).unwrap();

    let out = Command::new(ecotokens())
        .args(["filter", "--", "cat", input_file.to_str().unwrap()])
        .output()
        .expect("failed to run filter");
    assert!(
        out.status.success(),
        "filter should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let filtered = String::from_utf8_lossy(&out.stdout);
    assert!(
        filtered.len() < input.len(),
        "filtered output ({} bytes) should be shorter than input ({} bytes)",
        filtered.len(),
        input.len()
    );
    assert!(
        filtered.contains("[ecotokens]"),
        "should contain summary marker"
    );
}

#[test]
fn config_subcommand_shows_settings() {
    let out = Command::new(ecotokens())
        .args(["config"])
        .output()
        .expect("failed to run config");
    assert!(out.status.success(), "config should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("debug"),
        "config output should contain settings keys"
    );
}

// ── T050 — gain --history ──────────────────────────────────────────────────────

#[test]
fn gain_history_shows_three_period_labels() {
    let home = temp_home_with_empty_metrics();
    let out = Command::new(ecotokens())
        .args(["gain", "--history"])
        .env("HOME", home.path())
        .output()
        .expect("failed to run gain --history");
    assert!(
        out.status.success(),
        "gain --history should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Last 24h"),
        "output should contain 'Last 24h': {stdout}"
    );
    assert!(
        stdout.contains("Last 7 days"),
        "output should contain 'Last 7 days': {stdout}"
    );
    assert!(
        stdout.contains("Last 30 days"),
        "output should contain 'Last 30 days': {stdout}"
    );
}

#[test]
fn gain_history_json_is_valid_with_period_keys() {
    let home = temp_home_with_empty_metrics();
    let out = Command::new(ecotokens())
        .args(["gain", "--history", "--json"])
        .env("HOME", home.path())
        .output()
        .expect("failed to run gain --history --json");
    assert!(
        out.status.success(),
        "gain --history --json should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON: {stdout}");
    assert!(v["day"].is_object(), "JSON should have 'day' key: {stdout}");
    assert!(
        v["week"].is_object(),
        "JSON should have 'week' key: {stdout}"
    );
    assert!(
        v["month"].is_object(),
        "JSON should have 'month' key: {stdout}"
    );
}

#[test]
fn gain_history_and_period_are_mutually_exclusive() {
    let out = Command::new(ecotokens())
        .args(["gain", "--history", "--period", "week"])
        .output()
        .expect("failed to run");
    assert!(!out.status.success(), "should fail when both flags are set");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "should show conflict error: {stderr}"
    );
}

#[test]
fn config_json_flag_outputs_valid_json() {
    let out = Command::new(ecotokens())
        .args(["config", "--json"])
        .output()
        .expect("failed to run config --json");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(
        v.is_ok(),
        "config --json should produce valid JSON: {stdout}"
    );
}
