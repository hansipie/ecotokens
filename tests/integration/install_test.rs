use ecotokens::install::{install_hook, uninstall_hook, InstallResult};
use std::process::Command;
use tempfile::TempDir;

fn temp_claude_settings(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join(".claude").join("settings.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    path
}

#[test]
fn install_writes_hook_to_settings() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);

    let result = install_hook(&settings_path, false);
    assert!(result.is_ok(), "install should succeed: {result:?}");

    let content = std::fs::read_to_string(&settings_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(
        v["hooks"]["PreToolUse"].is_array(),
        "PreToolUse hooks should be present"
    );
}

#[test]
fn install_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);

    install_hook(&settings_path, false).unwrap();
    install_hook(&settings_path, false).unwrap();

    let content = std::fs::read_to_string(&settings_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    let hooks = v["hooks"]["PreToolUse"].as_array().unwrap();
    let ecotokens_count = hooks
        .iter()
        .filter(|h| h["hooks"][0]["command"].as_str().unwrap_or("").contains("ecotokens"))
        .count();
    assert_eq!(ecotokens_count, 1, "should not duplicate the hook");
}

#[test]
fn uninstall_removes_only_ecotokens_entry() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);

    // Pre-populate with another hook + ecotokens
    let initial = serde_json::json!({
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "OtherTool",
                    "hooks": [{"type": "command", "command": "other-hook"}]
                }
            ]
        }
    });
    std::fs::write(&settings_path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

    install_hook(&settings_path, false).unwrap();
    uninstall_hook(&settings_path).unwrap();

    let content = std::fs::read_to_string(&settings_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    let hooks = v["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(hooks.len(), 1, "only other hook should remain");
    assert!(hooks[0]["hooks"][0]["command"].as_str().unwrap().contains("other-hook"));
}

#[test]
fn uninstall_when_no_hook_is_ok() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    // File doesn't exist yet
    let result = uninstall_hook(&settings_path);
    assert!(result.is_ok(), "uninstall on missing file should be Ok");
}

#[test]
fn config_dir_created_on_install() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    install_hook(&settings_path, false).unwrap();
    assert!(settings_path.exists(), "settings.json should exist after install");
}

// ── T039t — ecotokens index CLI ───────────────────────────────────────────────

fn ecotokens_bin() -> String {
    env!("CARGO_BIN_EXE_ecotokens").to_string()
}

#[test]
fn index_path_flag_indexes_given_directory() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    std::fs::write(src.path().join("main.rs"), "fn main() {}").unwrap();
    let out = Command::new(ecotokens_bin())
        .args(["index", "--path", src.path().to_str().unwrap(), "--index-dir", idx.path().to_str().unwrap()])
        .output()
        .expect("failed to run ecotokens");
    assert!(out.status.success(), "exit code should be 0, stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("file") || stdout.contains("chunk"), "stdout should contain stats: {stdout}");
}

#[test]
fn index_reset_flag_recreates_index() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    std::fs::write(src.path().join("lib.rs"), "pub fn foo() {}").unwrap();
    // First index
    Command::new(ecotokens_bin())
        .args(["index", "--path", src.path().to_str().unwrap(), "--index-dir", idx.path().to_str().unwrap()])
        .output().unwrap();
    // Reset
    let out = Command::new(ecotokens_bin())
        .args(["index", "--path", src.path().to_str().unwrap(), "--index-dir", idx.path().to_str().unwrap(), "--reset"])
        .output()
        .expect("failed to run ecotokens");
    assert!(out.status.success(), "reset should succeed, stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn index_stats_printed_to_stdout() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    std::fs::write(src.path().join("a.rs"), "struct A;").unwrap();
    let out = Command::new(ecotokens_bin())
        .args(["index", "--path", src.path().to_str().unwrap(), "--index-dir", idx.path().to_str().unwrap()])
        .output()
        .expect("failed to run ecotokens");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Stats must be on stdout, not swallowed
    assert!(!stdout.is_empty(), "stdout should contain indexing stats");
}
