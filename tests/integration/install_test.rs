#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use ecotokens::install::{
    are_session_hooks_installed, enable_hermes_plugin_in_config, install_codex_mcp_server,
    install_codex_plugin, install_gemini_hook, install_hermes_plugin, install_hook,
    install_mcp_server, install_post_hook, install_qwen_hook, install_session_hooks,
    is_codex_mcp_registered, is_codex_plugin_installed, is_gemini_hook_installed,
    is_gemini_mcp_registered, is_hermes_plugin_enabled_in_config, is_hermes_plugin_installed,
    is_mcp_registered, is_post_hook_installed, is_qwen_hook_installed, is_qwen_mcp_registered,
    uninstall_codex_mcp_server, uninstall_codex_plugin, uninstall_gemini, uninstall_hermes_plugin,
    uninstall_hook, uninstall_qwen,
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

fn assert_hook_idempotent(
    settings_path: &std::path::Path,
    install_fn: impl Fn(&std::path::Path),
    hook_type: &str,
    cmd_substr: &str,
) {
    install_fn(settings_path);
    install_fn(settings_path);
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(settings_path).unwrap()).unwrap();
    let hooks = v["hooks"][hook_type].as_array().unwrap();
    let count = hooks
        .iter()
        .filter(|h| {
            h["hooks"][0]["command"]
                .as_str()
                .unwrap_or("")
                .contains(cmd_substr)
        })
        .count();
    assert_eq!(count, 1, "should not duplicate the hook");
}

#[test]
fn gemini_install_hook_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let path = temp_gemini_settings(&dir);
    assert_hook_idempotent(
        &path,
        |p| {
            install_gemini_hook(p).unwrap();
        },
        "BeforeTool",
        "hook-gemini",
    );
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

fn assert_uninstall_preserves_third_party(
    settings_path: &std::path::Path,
    hook_type: &str,
    ecotokens_command: &str,
    uninstall_fn: impl Fn(&std::path::Path),
) {
    let initial = serde_json::json!({
        "hooks": {
            hook_type: [
                {
                    "matcher": "other_tool",
                    "hooks": [{"type": "command", "command": "other-hook"}]
                },
                {
                    "matcher": "run_shell_command",
                    "hooks": [{"type": "command", "command": ecotokens_command}]
                }
            ]
        },
        "mcpServers": {
            "other-server": {"command": "other", "args": []},
            "ecotokens": {"command": "ecotokens", "args": ["mcp"], "type": "stdio"}
        }
    });
    std::fs::write(
        settings_path,
        serde_json::to_string_pretty(&initial).unwrap(),
    )
    .unwrap();
    uninstall_fn(settings_path);
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(settings_path).unwrap()).unwrap();
    let hooks = v["hooks"][hook_type].as_array().unwrap();
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
fn gemini_uninstall_preserves_third_party_hooks() {
    let dir = TempDir::new().unwrap();
    let path = temp_gemini_settings(&dir);
    assert_uninstall_preserves_third_party(&path, "BeforeTool", "ecotokens hook-gemini", |p| {
        uninstall_gemini(p).unwrap();
    });
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

// ── Qwen Code installation tests ───────────────────────────────────────────────

fn temp_qwen_settings(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join(".qwen").join("settings.json");
    std::fs::create_dir_all(path.parent().unwrap()).expect("create .qwen dir");
    path
}

#[test]
fn qwen_install_hook_writes_pre_tool_use_entry() {
    let dir = TempDir::new().unwrap();
    let path = temp_qwen_settings(&dir);

    install_qwen_hook(&path).expect("install_qwen_hook should succeed");

    let content = std::fs::read_to_string(&path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(
        v["hooks"]["PreToolUse"].is_array(),
        "PreToolUse hooks array must exist"
    );
    let hooks = v["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(hooks.len(), 1, "should have exactly one hook");
    assert_eq!(
        hooks[0]["hooks"][0]["command"].as_str().unwrap(),
        "ecotokens hook-qwen"
    );
    assert_eq!(hooks[0]["matcher"].as_str().unwrap(), "run_shell_command");
}

#[test]
fn qwen_install_hook_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let path = temp_qwen_settings(&dir);
    assert_hook_idempotent(
        &path,
        |p| {
            install_qwen_hook(p).unwrap();
        },
        "PreToolUse",
        "hook-qwen",
    );
}

#[test]
fn qwen_is_hook_installed_returns_false_before_install() {
    let dir = TempDir::new().unwrap();
    let path = temp_qwen_settings(&dir);
    assert!(!is_qwen_hook_installed(&path));
}

#[test]
fn qwen_is_hook_installed_returns_true_after_install() {
    let dir = TempDir::new().unwrap();
    let path = temp_qwen_settings(&dir);
    install_qwen_hook(&path).unwrap();
    assert!(is_qwen_hook_installed(&path));
}

#[test]
fn qwen_is_mcp_registered_returns_false_before_install() {
    let dir = TempDir::new().unwrap();
    let path = temp_qwen_settings(&dir);
    assert!(!is_qwen_mcp_registered(&path));
}

#[test]
fn qwen_uninstall_removes_hook_and_mcp() {
    let dir = TempDir::new().unwrap();
    let path = temp_qwen_settings(&dir);

    let initial = serde_json::json!({
        "hooks": { "PreToolUse": [{"matcher": "run_shell_command", "hooks": [{"type": "command", "command": "ecotokens hook-qwen"}]}] },
        "mcpServers": { "ecotokens": { "command": "ecotokens", "args": ["mcp"], "type": "stdio" } }
    });
    std::fs::write(&path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

    assert!(is_qwen_hook_installed(&path));
    assert!(is_qwen_mcp_registered(&path));

    uninstall_qwen(&path).expect("uninstall_qwen should succeed");

    assert!(!is_qwen_hook_installed(&path), "hook should be removed");
    assert!(!is_qwen_mcp_registered(&path), "MCP should be removed");
}

#[test]
fn qwen_uninstall_preserves_third_party_hooks() {
    let dir = TempDir::new().unwrap();
    let path = temp_qwen_settings(&dir);
    assert_uninstall_preserves_third_party(&path, "PreToolUse", "ecotokens hook-qwen", |p| {
        uninstall_qwen(p).unwrap();
    });
}

#[test]
fn qwen_uninstall_on_missing_file_is_ok() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".qwen").join("settings.json");
    assert!(uninstall_qwen(&path).is_ok());
}

#[test]
fn qwen_install_creates_directory() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".qwen").join("settings.json");
    install_qwen_hook(&path).expect("should create parent dir and settings file");
    assert!(path.exists());
}

// ── PostToolUse hook tests ──────────────────────────────────────────────────

#[test]
fn post_hook_installs_in_posttooluse() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");

    install_post_hook(&path).expect("install_post_hook should succeed");

    let content = std::fs::read_to_string(&path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    let post_hooks = v["hooks"]["PostToolUse"].as_array().unwrap();
    assert!(
        post_hooks.iter().any(|h| h["hooks"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|e| e["command"].as_str())
            .map(|c| c.contains("hook-post"))
            .unwrap_or(false)),
        "PostToolUse hook with 'hook-post' command should be installed"
    );
}

#[test]
fn post_hook_idempotent() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");

    install_post_hook(&path).unwrap();
    install_post_hook(&path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    let post_hooks = v["hooks"]["PostToolUse"].as_array().unwrap();
    let count = post_hooks
        .iter()
        .filter(|h| {
            h["hooks"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|e| e["command"].as_str())
                .map(|c| c.contains("hook-post"))
                .unwrap_or(false)
        })
        .count();
    assert_eq!(
        count, 1,
        "hook-post should appear exactly once after two installs"
    );
}

#[test]
fn post_hook_preserves_existing_pretooluse_hook() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");

    // Install PreToolUse hook first
    install_hook(&path, &path).unwrap();
    // Then install PostToolUse hook
    install_post_hook(&path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();

    // PreToolUse hook should still be there
    let pre_hooks = v["hooks"]["PreToolUse"].as_array().unwrap();
    assert!(
        pre_hooks.iter().any(|h| h["hooks"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|e| e["command"].as_str())
            .map(|c| c == "ecotokens hook")
            .unwrap_or(false)),
        "PreToolUse hook should be preserved after install_post_hook"
    );
}

#[test]
fn uninstall_removes_post_hook() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);

    install_post_hook(&settings_path).unwrap();
    assert!(is_post_hook_installed(&settings_path));

    uninstall_hook(&settings_path, &claude_json).unwrap();
    assert!(
        !is_post_hook_installed(&settings_path),
        "PostToolUse hook should be removed after uninstall"
    );

    let content = std::fs::read_to_string(&settings_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    let post_hooks = &v["hooks"]["PostToolUse"];
    assert!(
        post_hooks.is_null() || post_hooks.as_array().map(|a| a.is_empty()).unwrap_or(false),
        "PostToolUse array should be empty or absent"
    );
}

#[test]
fn uninstall_removes_mcp_from_settings_json() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);

    install_mcp_server(&settings_path).unwrap();
    assert!(
        is_mcp_registered(&settings_path),
        "MCP should be present after install"
    );

    uninstall_hook(&settings_path, &claude_json).unwrap();
    assert!(
        !is_mcp_registered(&settings_path),
        "MCP should be removed from settings.json after uninstall"
    );
}

#[test]
fn uninstall_removes_session_hooks() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);

    install_session_hooks(&settings_path).unwrap();
    assert!(are_session_hooks_installed(&settings_path));

    uninstall_hook(&settings_path, &claude_json).unwrap();
    assert!(
        !are_session_hooks_installed(&settings_path),
        "SessionStart/SessionEnd hooks should be removed after uninstall"
    );
}

#[test]
fn uninstall_session_hooks_absent_is_ok() {
    let dir = TempDir::new().unwrap();
    let settings_path = temp_claude_settings(&dir);
    let claude_json = temp_claude_json(&dir);

    install_hook(&settings_path, &claude_json).unwrap();
    // Pas de session hooks — uninstall ne doit pas paniquer
    let result = uninstall_hook(&settings_path, &claude_json);
    assert!(
        result.is_ok(),
        "uninstall without session hooks should be Ok"
    );
}

#[test]
fn hermes_install_writes_plugin_manifest_and_hooks() {
    let dir = TempDir::new().unwrap();
    let plugin_dir = dir.path().join(".hermes").join("plugins").join("ecotokens");

    install_hermes_plugin(&plugin_dir).expect("install_hermes_plugin should succeed");

    assert!(is_hermes_plugin_installed(&plugin_dir));
    let manifest = std::fs::read_to_string(plugin_dir.join("plugin.yaml")).unwrap();
    assert!(
        manifest.contains("on_session_start"),
        "manifest doit déclarer on_session_start"
    );
    assert!(
        manifest.contains("on_session_end"),
        "manifest doit déclarer on_session_end"
    );
    assert!(manifest.contains("name: ecotokens"));
    assert!(manifest.contains("transform_terminal_output"));
    assert!(manifest.contains("transform_tool_result"));

    let init = std::fs::read_to_string(plugin_dir.join("__init__.py")).unwrap();
    assert!(init.contains("ctx.register_hook(\"transform_terminal_output\""));
    assert!(init.contains("ctx.register_hook(\"transform_tool_result\""));
    assert!(init.contains("ctx.register_hook(\"on_session_start\""));
    assert!(init.contains("ctx.register_hook(\"on_session_end\""));
    assert!(init.contains("filter-output"));
}

#[test]
fn hermes_plugin_registers_session_hooks_for_auto_watch() {
    let dir = TempDir::new().unwrap();
    let plugin_dir = dir.path().join(".hermes").join("plugins").join("ecotokens");

    install_hermes_plugin(&plugin_dir).unwrap();

    let init = std::fs::read_to_string(plugin_dir.join("__init__.py")).unwrap();

    // Les hooks session doivent appeler ecotokens session-start / session-end.
    assert!(
        init.contains("session-start"),
        "__init__.py doit appeler ecotokens session-start"
    );
    assert!(
        init.contains("session-end"),
        "__init__.py doit appeler ecotokens session-end"
    );
    // Les deux doivent être fail-open.
    assert!(
        init.contains("on_session_start"),
        "on_session_start doit être défini"
    );
    assert!(
        init.contains("on_session_end"),
        "on_session_end doit être défini"
    );
    // Syntaxe Python valide après ajout des hooks session.
    let out = std::process::Command::new("python3")
        .args([
            "-m",
            "py_compile",
            plugin_dir.join("__init__.py").to_str().unwrap(),
        ])
        .output();
    match out {
        Ok(r) => assert!(
            r.status.success(),
            "py_compile doit réussir après ajout des hooks session:\n{}",
            String::from_utf8_lossy(&r.stderr)
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("python3 non disponible, test de syntaxe ignoré");
        }
        Err(e) => panic!("impossible de lancer python3: {e}"),
    }
}

#[test]
fn hermes_install_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let plugin_dir = dir.path().join(".hermes").join("plugins").join("ecotokens");

    install_hermes_plugin(&plugin_dir).unwrap();
    let first = std::fs::read_to_string(plugin_dir.join("__init__.py")).unwrap();
    install_hermes_plugin(&plugin_dir).unwrap();
    let second = std::fs::read_to_string(plugin_dir.join("__init__.py")).unwrap();

    assert_eq!(first, second, "Hermes plugin install should be stable");
}

#[test]
fn hermes_uninstall_removes_plugin_directory() {
    let dir = TempDir::new().unwrap();
    let plugin_dir = dir.path().join(".hermes").join("plugins").join("ecotokens");

    install_hermes_plugin(&plugin_dir).unwrap();
    assert!(is_hermes_plugin_installed(&plugin_dir));

    uninstall_hermes_plugin(&plugin_dir).expect("uninstall_hermes_plugin should succeed");

    assert!(!plugin_dir.exists(), "Hermes plugin dir should be removed");
}

#[test]
fn hermes_plugin_python_syntax_is_valid() {
    let dir = TempDir::new().unwrap();
    let plugin_dir = dir.path().join(".hermes").join("plugins").join("ecotokens");

    install_hermes_plugin(&plugin_dir).unwrap();

    let init_py = plugin_dir.join("__init__.py");
    let out = Command::new("python3")
        .args(["-m", "py_compile", init_py.to_str().unwrap()])
        .output();

    match out {
        Ok(result) => {
            assert!(
                result.status.success(),
                "python3 -m py_compile failed:\n{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("python3 not found, skipping syntax check");
        }
        Err(e) => panic!("failed to run python3: {e}"),
    }
}

#[test]
fn hermes_uninstall_when_plugin_absent_is_ok() {
    let dir = TempDir::new().unwrap();
    let plugin_dir = dir.path().join(".hermes").join("plugins").join("ecotokens");
    // Le dossier n'existe pas — uninstall doit être idempotent
    assert!(uninstall_hermes_plugin(&plugin_dir).is_ok());
}

#[test]
fn codex_install_writes_plugin_manifest() {
    let dir = TempDir::new().unwrap();
    let plugin_dir = dir.path().join(".codex").join("plugins").join("ecotokens");

    install_codex_plugin(&plugin_dir).expect("install_codex_plugin should succeed");

    assert!(is_codex_plugin_installed(&plugin_dir));
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(plugin_dir.join(".codex-plugin").join("plugin.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["name"].as_str().unwrap(), "ecotokens");
    assert_eq!(
        manifest["interface"]["displayName"].as_str().unwrap(),
        "ecotokens"
    );
    assert!(manifest.get("hooks").is_none());
    assert!(
        !plugin_dir.join("hooks.json").exists(),
        "hooks.json ne doit pas être à la racine"
    );
    assert!(
        !plugin_dir.join("hooks").join("hooks.json").exists(),
        "pas de hooks/hooks.json"
    );
}

#[test]
fn codex_install_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let plugin_dir = dir.path().join(".codex").join("plugins").join("ecotokens");

    install_codex_plugin(&plugin_dir).unwrap();
    let first =
        std::fs::read_to_string(plugin_dir.join(".codex-plugin").join("plugin.json")).unwrap();
    install_codex_plugin(&plugin_dir).unwrap();
    let second =
        std::fs::read_to_string(plugin_dir.join(".codex-plugin").join("plugin.json")).unwrap();

    assert_eq!(first, second, "Codex plugin install should be stable");
}

#[test]
fn codex_uninstall_removes_plugin_directory() {
    let dir = TempDir::new().unwrap();
    let plugin_dir = dir.path().join(".codex").join("plugins").join("ecotokens");

    install_codex_plugin(&plugin_dir).unwrap();
    assert!(is_codex_plugin_installed(&plugin_dir));

    uninstall_codex_plugin(&plugin_dir).expect("uninstall_codex_plugin should succeed");

    assert!(!plugin_dir.exists(), "Codex plugin dir should be removed");
}

#[test]
fn codex_uninstall_when_plugin_absent_is_ok() {
    let dir = TempDir::new().unwrap();
    let plugin_dir = dir.path().join(".codex").join("plugins").join("ecotokens");
    assert!(uninstall_codex_plugin(&plugin_dir).is_ok());
}

#[test]
fn codex_plugin_install_no_hooks_file() {
    // Codex exposes SessionStart but has no SessionEnd, so session lifecycle hooks are not
    // installed. install_codex_plugin must not create any hooks file in the plugin directory.
    let dir = TempDir::new().unwrap();
    let plugin_dir = dir.path().join(".codex").join("plugins").join("ecotokens");

    install_codex_plugin(&plugin_dir).unwrap();

    assert!(
        !plugin_dir.join("hooks.json").exists(),
        "pas de hooks.json à la racine"
    );
    assert!(
        !plugin_dir.join("hooks").join("hooks.json").exists(),
        "pas de hooks/hooks.json"
    );
}

#[test]
fn codex_mcp_install_writes_config_toml() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join(".codex").join("config.toml");

    assert!(!is_codex_mcp_registered(&config_path));
    install_codex_mcp_server(&config_path).expect("install_codex_mcp_server should succeed");
    assert!(is_codex_mcp_registered(&config_path));

    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("[mcp_servers.ecotokens]"));
    assert!(content.contains("command ="));
    assert!(content.contains("mcp-server"));
}

#[test]
fn codex_mcp_install_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join(".codex").join("config.toml");

    install_codex_mcp_server(&config_path).unwrap();
    let first = std::fs::read_to_string(&config_path).unwrap();
    install_codex_mcp_server(&config_path).unwrap();
    let second = std::fs::read_to_string(&config_path).unwrap();

    assert_eq!(first, second, "Codex MCP install should be stable");
    assert_eq!(
        first.matches("[mcp_servers.ecotokens]").count(),
        1,
        "doit apparaître une seule fois"
    );
}

#[test]
fn codex_mcp_uninstall_removes_entry() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join(".codex").join("config.toml");

    install_codex_mcp_server(&config_path).unwrap();
    assert!(is_codex_mcp_registered(&config_path));

    uninstall_codex_mcp_server(&config_path).expect("uninstall_codex_mcp_server should succeed");
    assert!(!is_codex_mcp_registered(&config_path));
}

#[test]
fn codex_mcp_uninstall_when_absent_is_ok() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join(".codex").join("config.toml");
    assert!(uninstall_codex_mcp_server(&config_path).is_ok());
}

#[test]
fn hermes_default_plugin_dir_respects_hermes_home() {
    use ecotokens::install::default_hermes_plugin_dir;

    let dir = TempDir::new().unwrap();
    // SAFETY: les tests Rust sont mono-thread par défaut dans ce module.
    // On isole la variable d'env avec un scope explicite.
    let plugin_dir = {
        // HERMES_HOME défini → le chemin doit l'utiliser
        std::env::set_var("HERMES_HOME", dir.path());
        let p = default_hermes_plugin_dir();
        std::env::remove_var("HERMES_HOME");
        p
    };

    let expected = dir.path().join("plugins").join("ecotokens");
    assert_eq!(
        plugin_dir.as_deref(),
        Some(expected.as_path()),
        "HERMES_HOME doit être utilisé comme base du chemin plugin"
    );
}

// ── Phase 2 : enable_hermes_plugin_in_config ────────────────────────────────

#[test]
fn hermes_enable_plugin_creates_config_with_enabled_entry() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join(".hermes").join("config.yaml");

    assert!(!is_hermes_plugin_enabled_in_config(&config));
    enable_hermes_plugin_in_config(&config).expect("enable should succeed");

    assert!(config.exists(), "config.yaml doit être créé");
    assert!(
        is_hermes_plugin_enabled_in_config(&config),
        "ecotokens doit apparaître dans plugins.enabled"
    );
    let content = std::fs::read_to_string(&config).unwrap();
    assert!(content.contains("plugins:"), "doit contenir plugins:");
    assert!(content.contains("enabled:"), "doit contenir enabled:");
    assert!(content.contains("- ecotokens"), "doit contenir - ecotokens");
}

#[test]
fn hermes_enable_plugin_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.yaml");

    enable_hermes_plugin_in_config(&config).unwrap();
    let first = std::fs::read_to_string(&config).unwrap();

    enable_hermes_plugin_in_config(&config).unwrap();
    let second = std::fs::read_to_string(&config).unwrap();

    assert_eq!(first, second, "double appel ne doit pas dupliquer l'entrée");
    assert_eq!(
        first.matches("- ecotokens").count(),
        1,
        "ecotokens ne doit apparaître qu'une fois"
    );
}

#[test]
fn hermes_enable_plugin_preserves_existing_keys() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.yaml");

    let initial = "model: claude-opus\nother_key: value\n";
    std::fs::write(&config, initial).unwrap();

    enable_hermes_plugin_in_config(&config).unwrap();

    let content = std::fs::read_to_string(&config).unwrap();
    assert!(
        content.contains("model: claude-opus"),
        "clé model préservée"
    );
    assert!(
        content.contains("other_key: value"),
        "clé other_key préservée"
    );
    assert!(content.contains("- ecotokens"), "ecotokens ajouté");
}

#[test]
fn hermes_enable_plugin_appends_to_existing_enabled_list() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.yaml");

    let initial = "plugins:\n  enabled:\n    - other-plugin\n";
    std::fs::write(&config, initial).unwrap();

    enable_hermes_plugin_in_config(&config).unwrap();

    let content = std::fs::read_to_string(&config).unwrap();
    assert!(
        content.contains("- other-plugin"),
        "plugin existant préservé"
    );
    assert!(content.contains("- ecotokens"), "ecotokens ajouté");
    assert_eq!(
        content.matches("plugins:").count(),
        1,
        "pas de doublon de la section plugins:"
    );
}

#[test]
fn hermes_enable_plugin_adds_enabled_when_plugins_key_exists_without_it() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.yaml");

    let initial = "plugins:\n  disabled:\n    - legacy\n";
    std::fs::write(&config, initial).unwrap();

    enable_hermes_plugin_in_config(&config).unwrap();

    let content = std::fs::read_to_string(&config).unwrap();
    assert!(content.contains("  enabled:"), "enabled: doit être ajouté");
    assert!(
        content.contains("- ecotokens"),
        "ecotokens doit être ajouté"
    );
    assert!(content.contains("- legacy"), "disabled: préservé");
}

#[test]
fn codex_install_end_to_end_with_codex_home() {
    let dir = TempDir::new().unwrap();
    let codex_home = dir.path().to_str().unwrap();

    let out = Command::new(ecotokens_bin())
        .args(["install", "--target", "codex"])
        .env("CODEX_HOME", codex_home)
        .env("HOME", dir.path())
        .output()
        .expect("failed to run ecotokens install --target codex");

    assert!(
        out.status.success(),
        "install --target codex doit réussir, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let plugin_dir = dir.path().join("plugins").join("ecotokens");
    assert!(
        plugin_dir
            .join(".codex-plugin")
            .join("plugin.json")
            .exists(),
        "plugin.json doit être créé"
    );
    assert!(!plugin_dir.join("hooks").join("hooks.json").exists());
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(plugin_dir.join(".codex-plugin").join("plugin.json")).unwrap(),
    )
    .unwrap();
    assert!(manifest.get("hooks").is_none());
}

// ── Phase 5 : test d'intégration end-to-end install Hermes ──────────────────

#[test]
fn hermes_install_end_to_end_with_hermes_home() {
    let dir = TempDir::new().unwrap();
    let hermes_home = dir.path().to_str().unwrap();

    // Lancer ecotokens install --target hermes avec HERMES_HOME temporaire.
    let out = Command::new(ecotokens_bin())
        .args(["install", "--target", "hermes"])
        .env("HERMES_HOME", hermes_home)
        .output()
        .expect("failed to run ecotokens install");

    assert!(
        out.status.success(),
        "install --target hermes doit réussir, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let plugin_dir = dir.path().join("plugins").join("ecotokens");
    assert!(
        plugin_dir.join("plugin.yaml").exists(),
        "plugin.yaml doit être créé"
    );
    assert!(
        plugin_dir.join("__init__.py").exists(),
        "__init__.py doit être créé"
    );

    // Vérifier que plugin.yaml contient les hooks attendus.
    let manifest = std::fs::read_to_string(plugin_dir.join("plugin.yaml")).unwrap();
    assert!(manifest.contains("transform_terminal_output"));
    assert!(manifest.contains("transform_tool_result"));
}

#[test]
fn hermes_install_enable_plugin_flag_updates_config_yaml() {
    let dir = TempDir::new().unwrap();
    let hermes_home = dir.path().to_str().unwrap();

    let out = Command::new(ecotokens_bin())
        .args(["install", "--target", "hermes", "--enable-plugin"])
        .env("HERMES_HOME", hermes_home)
        .output()
        .expect("failed to run ecotokens install --enable-plugin");

    assert!(
        out.status.success(),
        "install --enable-plugin doit réussir, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config = dir.path().join("config.yaml");
    assert!(
        config.exists(),
        "config.yaml doit être créé par --enable-plugin"
    );
    assert!(
        is_hermes_plugin_enabled_in_config(&config),
        "ecotokens doit apparaître dans plugins.enabled"
    );

    // Idempotence : un second appel ne doit pas dupliquer l'entrée.
    Command::new(ecotokens_bin())
        .args(["install", "--target", "hermes", "--enable-plugin"])
        .env("HERMES_HOME", hermes_home)
        .output()
        .unwrap();

    let content = std::fs::read_to_string(&config).unwrap();
    assert_eq!(
        content.matches("- ecotokens").count(),
        1,
        "ecotokens ne doit apparaître qu'une seule fois après deux installs"
    );
}

#[test]
fn hermes_uninstall_end_to_end_with_hermes_home() {
    let dir = TempDir::new().unwrap();
    let hermes_home = dir.path().to_str().unwrap();

    // Installer puis désinstaller.
    Command::new(ecotokens_bin())
        .args(["install", "--target", "hermes"])
        .env("HERMES_HOME", hermes_home)
        .output()
        .unwrap();

    let plugin_dir = dir.path().join("plugins").join("ecotokens");
    assert!(plugin_dir.exists(), "plugin installé");

    let out = Command::new(ecotokens_bin())
        .args(["uninstall", "--target", "hermes"])
        .env("HERMES_HOME", hermes_home)
        .output()
        .expect("failed to run ecotokens uninstall");

    assert!(
        out.status.success(),
        "uninstall doit réussir, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !plugin_dir.exists(),
        "le dossier plugin doit être supprimé après uninstall"
    );
}
