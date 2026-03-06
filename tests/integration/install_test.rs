use ecotokens::install::{install_hook, uninstall_hook, InstallResult};
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
