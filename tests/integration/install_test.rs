use ecotokens::install::{
    install_hook, install_vscode_mcp, is_vscode_mcp_registered, uninstall_hook,
    uninstall_vscode_mcp,
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

    let result = install_hook(&settings_path, &claude_json, false);
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

    install_hook(&settings_path, &claude_json, false).unwrap();
    install_hook(&settings_path, &claude_json, false).unwrap();

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
    std::fs::write(&settings_path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

    install_hook(&settings_path, &claude_json, false).unwrap();
    uninstall_hook(&settings_path, &claude_json).unwrap();

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
    install_hook(&settings_path, &claude_json, false).unwrap();
    assert!(settings_path.exists(), "settings.json should exist after install");
}

// ── T062t — install --with-mcp ────────────────────────────────────────────────

#[test]
fn install_with_mcp_adds_mcp_entry() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);

    install_hook(&settings_path, &claude_json, true).unwrap();

    // MCP must be in ~/.claude.json, NOT in settings.json
    let cv: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&claude_json).unwrap()
    ).unwrap();
    assert!(
        cv["mcpServers"]["ecotokens"].is_object(),
        "mcpServers.ecotokens should be in ~/.claude.json after --with-mcp"
    );
    assert_eq!(
        cv["mcpServers"]["ecotokens"]["command"].as_str().unwrap_or(""),
        "ecotokens",
        "MCP command should be 'ecotokens'"
    );
    assert_eq!(
        cv["mcpServers"]["ecotokens"]["args"][0].as_str().unwrap_or(""),
        "mcp",
        "MCP first arg should be 'mcp'"
    );

    // settings.json must NOT contain mcpServers
    let sv: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&settings_path).unwrap()
    ).unwrap();
    assert!(
        sv["mcpServers"].is_null() || !sv["mcpServers"].as_object().map_or(false, |m| m.contains_key("ecotokens")),
        "mcpServers.ecotokens should NOT be in settings.json"
    );
}

#[test]
fn install_with_mcp_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);

    install_hook(&settings_path, &claude_json, true).unwrap();
    install_hook(&settings_path, &claude_json, true).unwrap();

    let cv: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&claude_json).unwrap()
    ).unwrap();
    let servers = cv["mcpServers"].as_object().unwrap();
    let ecotokens_count = servers.keys().filter(|k| k.as_str() == "ecotokens").count();
    assert_eq!(ecotokens_count, 1, "should not duplicate MCP entry");
}

#[test]
fn install_with_mcp_preserves_existing_hooks() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);

    // Pre-populate with another hook
    let initial = serde_json::json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": "OtherTool",
                "hooks": [{"type": "command", "command": "other-hook"}]
            }]
        }
    });
    std::fs::write(&settings_path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

    install_hook(&settings_path, &claude_json, true).unwrap();

    let sv: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&settings_path).unwrap()
    ).unwrap();
    let hooks = sv["hooks"]["PreToolUse"].as_array().unwrap();
    assert!(hooks.len() >= 2, "both hooks should be present in settings.json");

    let cv: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&claude_json).unwrap()
    ).unwrap();
    assert!(cv["mcpServers"]["ecotokens"].is_object(), "MCP entry should be in ~/.claude.json");
}

#[test]
fn install_without_mcp_does_not_add_mcp_entry() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);

    install_hook(&settings_path, &claude_json, false).unwrap();

    assert!(
        !claude_json.exists()
            || {
                let cv: serde_json::Value = serde_json::from_str(
                    &std::fs::read_to_string(&claude_json).unwrap_or_default()
                ).unwrap_or(serde_json::json!({}));
                !cv["mcpServers"].as_object().map_or(false, |m| m.contains_key("ecotokens"))
            },
        "mcpServers.ecotokens should NOT be written without --with-mcp"
    );
}

#[test]
fn uninstall_removes_mcp_entry_from_claude_json() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);

    install_hook(&settings_path, &claude_json, true).unwrap();
    uninstall_hook(&settings_path, &claude_json).unwrap();

    let cv: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&claude_json).unwrap()
    ).unwrap();
    assert!(
        !cv["mcpServers"].as_object().map_or(false, |m| m.contains_key("ecotokens")),
        "mcpServers.ecotokens should be removed from ~/.claude.json after uninstall"
    );
}

#[test]
fn uninstall_preserves_other_mcp_entries() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);

    // Pre-populate ~/.claude.json with another MCP server
    let initial = serde_json::json!({
        "mcpServers": {
            "other-tool": { "command": "other-tool mcp", "type": "stdio" }
        }
    });
    std::fs::write(&claude_json, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

    install_hook(&settings_path, &claude_json, true).unwrap();
    uninstall_hook(&settings_path, &claude_json).unwrap();

    let cv: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&claude_json).unwrap()
    ).unwrap();
    assert!(
        cv["mcpServers"]["other-tool"].is_object(),
        "other MCP server should still be present after uninstall"
    );
    assert!(
        !cv["mcpServers"].as_object().map_or(false, |m| m.contains_key("ecotokens")),
        "ecotokens MCP entry should be gone"
    );
}

// ── VS Code MCP installation ──────────────────────────────────────────────────

fn temp_vscode_settings(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("Code").join("User").join("settings.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    path
}

#[test]
fn vscode_install_writes_mcp_entry() {
    let dir = TempDir::new().unwrap();
    let vscode_path = temp_vscode_settings(&dir);

    install_vscode_mcp(&vscode_path).expect("install_vscode_mcp should succeed");

    let content = std::fs::read_to_string(&vscode_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(
        v["mcp"]["servers"]["ecotokens"].is_object(),
        "mcp.servers.ecotokens should be present: {v}"
    );
    assert_eq!(
        v["mcp"]["servers"]["ecotokens"]["command"].as_str().unwrap_or(""),
        "ecotokens",
        "command should be 'ecotokens'"
    );
    assert_eq!(
        v["mcp"]["servers"]["ecotokens"]["args"][0].as_str().unwrap_or(""),
        "mcp",
        "first arg should be 'mcp'"
    );
}

#[test]
fn vscode_install_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let vscode_path = temp_vscode_settings(&dir);

    install_vscode_mcp(&vscode_path).unwrap();
    install_vscode_mcp(&vscode_path).unwrap();

    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&vscode_path).unwrap()).unwrap();
    let count = v["mcp"]["servers"]
        .as_object()
        .map(|m| m.keys().filter(|k| k.as_str() == "ecotokens").count())
        .unwrap_or(0);
    assert_eq!(count, 1, "should not duplicate the MCP entry");
}

#[test]
fn vscode_install_preserves_existing_settings() {
    let dir = TempDir::new().unwrap();
    let vscode_path = temp_vscode_settings(&dir);

    // Pre-populate with unrelated setting
    let initial = serde_json::json!({
        "editor.fontSize": 14,
        "mcp": {
            "servers": {
                "other-tool": { "type": "stdio", "command": "other-tool", "args": ["mcp"] }
            }
        }
    });
    std::fs::write(&vscode_path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

    install_vscode_mcp(&vscode_path).unwrap();

    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&vscode_path).unwrap()).unwrap();
    assert!(v["mcp"]["servers"]["ecotokens"].is_object(), "ecotokens should be added");
    assert!(v["mcp"]["servers"]["other-tool"].is_object(), "other-tool should be preserved");
    assert_eq!(v["editor.fontSize"].as_u64().unwrap_or(0), 14, "unrelated settings preserved");
}

#[test]
fn vscode_uninstall_removes_mcp_entry() {
    let dir = TempDir::new().unwrap();
    let vscode_path = temp_vscode_settings(&dir);

    install_vscode_mcp(&vscode_path).unwrap();
    assert!(is_vscode_mcp_registered(&vscode_path), "should be registered after install");

    uninstall_vscode_mcp(&vscode_path).unwrap();
    assert!(!is_vscode_mcp_registered(&vscode_path), "should not be registered after uninstall");
}

#[test]
fn vscode_uninstall_preserves_other_mcp_entries() {
    let dir = TempDir::new().unwrap();
    let vscode_path = temp_vscode_settings(&dir);

    let initial = serde_json::json!({
        "mcp": {
            "servers": {
                "other-tool": { "type": "stdio", "command": "other-tool", "args": ["mcp"] }
            }
        }
    });
    std::fs::write(&vscode_path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

    install_vscode_mcp(&vscode_path).unwrap();
    uninstall_vscode_mcp(&vscode_path).unwrap();

    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&vscode_path).unwrap()).unwrap();
    assert!(
        v["mcp"]["servers"]["other-tool"].is_object(),
        "other-tool should still be present after uninstall"
    );
    assert!(
        !v["mcp"]["servers"].as_object().map_or(false, |m| m.contains_key("ecotokens")),
        "ecotokens entry should be gone"
    );
}

#[test]
fn vscode_uninstall_when_file_missing_is_ok() {
    let dir = TempDir::new().unwrap();
    let vscode_path = temp_vscode_settings(&dir);
    // File does not exist
    let result = uninstall_vscode_mcp(&vscode_path);
    assert!(result.is_ok(), "uninstall on missing file should be Ok");
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
