#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use ecotokens::install::{
    install_gemini_hook, install_hook, is_gemini_hook_installed, is_gemini_mcp_registered,
    uninstall_gemini, uninstall_hook,
};
use std::process::Command;
use tempfile::TempDir;

fn temp_claude_settings(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join(".claude").join("settings.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    path
}

fn temp_claude_json(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join(".claude.json")
}

#[test]
fn install_writes_hook_to_settings() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);

    let result = install_hook(&settings_path, &claude_json);
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
    let claude_json = temp_claude_json(&dir);

    install_hook(&settings_path, &claude_json).unwrap();
    install_hook(&settings_path, &claude_json).unwrap();

    let content = std::fs::read_to_string(&settings_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    let hooks = v["hooks"]["PreToolUse"].as_array().unwrap();
    let ecotokens_count = hooks
        .iter()
        .filter(|h| {
            h["hooks"][0]["command"]
                .as_str()
                .unwrap_or("")
                .contains("ecotokens")
        })
        .count();
    assert_eq!(ecotokens_count, 1, "should not duplicate the hook");
}

#[test]
fn uninstall_removes_only_ecotokens_entry() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);

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
    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&initial).unwrap(),
    )
    .unwrap();

    install_hook(&settings_path, &claude_json).unwrap();
    uninstall_hook(&settings_path, &claude_json).unwrap();

    let content = std::fs::read_to_string(&settings_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    let hooks = v["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(hooks.len(), 1, "only other hook should remain");
    assert!(hooks[0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("other-hook"));
}

#[test]
fn uninstall_when_no_hook_is_ok() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);
    // Files don't exist yet
    let result = uninstall_hook(&settings_path, &claude_json);
    assert!(result.is_ok(), "uninstall on missing file should be Ok");
}

#[test]
fn config_dir_created_on_install() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);
    install_hook(&settings_path, &claude_json).unwrap();
    assert!(
        settings_path.exists(),
        "settings.json should exist after install"
    );
}

#[test]
fn uninstall_removes_mcp_entry_from_claude_json() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);

    // Simulate a previously-installed MCP entry
    let initial = serde_json::json!({
        "mcpServers": {
            "ecotokens": { "command": "ecotokens", "args": ["mcp"], "type": "stdio" }
        }
    });
    std::fs::write(
        &claude_json,
        serde_json::to_string_pretty(&initial).unwrap(),
    )
    .unwrap();

    uninstall_hook(&settings_path, &claude_json).unwrap();

    let cv: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&claude_json).unwrap()).unwrap();
    assert!(
        !cv["mcpServers"]
            .as_object()
            .map_or(false, |m| m.contains_key("ecotokens")),
        "mcpServers.ecotokens should be removed from ~/.claude.json after uninstall"
    );
}

#[test]
fn uninstall_preserves_other_mcp_entries() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);

    // Pre-populate ~/.claude.json with another MCP server + ecotokens
    let initial = serde_json::json!({
        "mcpServers": {
            "other-tool": { "command": "other-tool mcp", "type": "stdio" },
            "ecotokens": { "command": "ecotokens", "args": ["mcp"], "type": "stdio" }
        }
    });
    std::fs::write(
        &claude_json,
        serde_json::to_string_pretty(&initial).unwrap(),
    )
    .unwrap();

    uninstall_hook(&settings_path, &claude_json).unwrap();

    let cv: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&claude_json).unwrap()).unwrap();
    assert!(
        cv["mcpServers"]["other-tool"].is_object(),
        "other MCP server should still be present after uninstall"
    );
    assert!(
        !cv["mcpServers"]
            .as_object()
            .map_or(false, |m| m.contains_key("ecotokens")),
        "ecotokens MCP entry should be gone"
    );
}

// ── T039t — ecotokens index CLI ───────────────────────────────────────────────

#[test]
fn index_path_flag_indexes_given_directory() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    std::fs::write(src.path().join("main.rs"), "fn main() {}").unwrap();
    let out = Command::new(ecotokens_bin())
        .args([
            "index",
            "--path",
            src.path().to_str().unwrap(),
            "--index-dir",
            idx.path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run ecotokens");
    assert!(
        out.status.success(),
        "exit code should be 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("file") || stdout.contains("chunk"),
        "stdout should contain stats: {stdout}"
    );
}

#[test]
fn index_reset_flag_recreates_index() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    std::fs::write(src.path().join("lib.rs"), "pub fn foo() {}").unwrap();
    // First index
    Command::new(ecotokens_bin())
        .args([
            "index",
            "--path",
            src.path().to_str().unwrap(),
            "--index-dir",
            idx.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    // Reset
    let out = Command::new(ecotokens_bin())
        .args([
            "index",
            "--path",
            src.path().to_str().unwrap(),
            "--index-dir",
            idx.path().to_str().unwrap(),
            "--reset",
        ])
        .output()
        .expect("failed to run ecotokens");
    assert!(
        out.status.success(),
        "reset should succeed, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn index_stats_printed_to_stdout() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    std::fs::write(src.path().join("a.rs"), "struct A;").unwrap();
    let out = Command::new(ecotokens_bin())
        .args([
            "index",
            "--path",
            src.path().to_str().unwrap(),
            "--index-dir",
            idx.path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run ecotokens");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Stats must be on stdout, not swallowed
    assert!(!stdout.is_empty(), "stdout should contain indexing stats");
}

// ── Gemini CLI installation tests ─────────────────────────────────────────────

fn temp_gemini_settings(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join(".gemini").join("settings.json");
    std::fs::create_dir_all(path.parent().unwrap()).expect("create .gemini dir");
    path
}

#[test]
fn gemini_install_hook_writes_before_tool_entry() {
    let dir = TempDir::new().unwrap();
    let path = temp_gemini_settings(&dir);

    install_gemini_hook(&path).expect("install_gemini_hook should succeed");

    let content = std::fs::read_to_string(&path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(
        v["hooks"]["BeforeTool"].is_array(),
        "BeforeTool hooks array must exist"
    );
    let hooks = v["hooks"]["BeforeTool"].as_array().unwrap();
    assert_eq!(hooks.len(), 1, "should have exactly one hook");
    assert_eq!(
        hooks[0]["hooks"][0]["command"].as_str().unwrap(),
        "ecotokens hook-gemini"
    );
}

#[test]
fn gemini_install_hook_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let path = temp_gemini_settings(&dir);

    install_gemini_hook(&path).unwrap();
    install_gemini_hook(&path).unwrap();

    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let hooks = v["hooks"]["BeforeTool"].as_array().unwrap();
    let count = hooks
        .iter()
        .filter(|h| {
            h["hooks"][0]["command"]
                .as_str()
                .unwrap_or("")
                .contains("hook-gemini")
        })
        .count();
    assert_eq!(count, 1, "should not duplicate the hook");
}

#[test]
fn gemini_is_hook_installed_returns_false_before_install() {
    let dir = TempDir::new().unwrap();
    let path = temp_gemini_settings(&dir);
    assert!(!is_gemini_hook_installed(&path));
}

#[test]
fn gemini_is_hook_installed_returns_true_after_install() {
    let dir = TempDir::new().unwrap();
    let path = temp_gemini_settings(&dir);
    install_gemini_hook(&path).unwrap();
    assert!(is_gemini_hook_installed(&path));
}

#[test]
fn gemini_is_mcp_registered_returns_false_before_install() {
    let dir = TempDir::new().unwrap();
    let path = temp_gemini_settings(&dir);
    assert!(!is_gemini_mcp_registered(&path));
}

#[test]
fn gemini_uninstall_removes_hook_and_mcp() {
    let dir = TempDir::new().unwrap();
    let path = temp_gemini_settings(&dir);

    // Simulate pre-installed hook + MCP entry
    let initial = serde_json::json!({
        "hooks": { "BeforeTool": [{"matcher": "run_shell_command", "hooks": [{"type": "command", "command": "ecotokens hook-gemini"}]}] },
        "mcpServers": { "ecotokens": { "command": "ecotokens", "args": ["mcp"], "type": "stdio" } }
    });
    std::fs::write(&path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

    assert!(is_gemini_hook_installed(&path));
    assert!(is_gemini_mcp_registered(&path));

    uninstall_gemini(&path).expect("uninstall_gemini should succeed");

    assert!(!is_gemini_hook_installed(&path), "hook should be removed");
    assert!(!is_gemini_mcp_registered(&path), "MCP should be removed");
}

#[test]
fn gemini_uninstall_preserves_third_party_hooks() {
    let dir = TempDir::new().unwrap();
    let path = temp_gemini_settings(&dir);

    let initial = serde_json::json!({
        "hooks": {
            "BeforeTool": [
                {
                    "matcher": "other_tool",
                    "hooks": [{"type": "command", "command": "other-hook"}]
                },
                {
                    "matcher": "run_shell_command",
                    "hooks": [{"type": "command", "command": "ecotokens hook-gemini"}]
                }
            ]
        },
        "mcpServers": {
            "other-server": {"command": "other", "args": []},
            "ecotokens": {"command": "ecotokens", "args": ["mcp"], "type": "stdio"}
        }
    });
    std::fs::write(&path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

    uninstall_gemini(&path).unwrap();

    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

    let hooks = v["hooks"]["BeforeTool"].as_array().unwrap();
    assert_eq!(hooks.len(), 1, "third-party hook should survive");
    assert_eq!(
        hooks[0]["hooks"][0]["command"].as_str().unwrap(),
        "other-hook"
    );

    assert!(
        v["mcpServers"]["other-server"].is_object(),
        "third-party MCP should survive"
    );
    assert!(
        v["mcpServers"]["ecotokens"].is_null(),
        "ecotokens MCP should be gone"
    );
}

#[test]
fn gemini_uninstall_on_missing_file_is_ok() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".gemini").join("settings.json");
    // File does not exist
    assert!(uninstall_gemini(&path).is_ok());
}

#[test]
fn gemini_install_creates_directory() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".gemini").join("settings.json");
    // Parent does not exist yet
    install_gemini_hook(&path).expect("should create parent dir and settings file");
    assert!(path.exists());
}
