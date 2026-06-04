use std::path::Path;

pub type InstallResult = std::io::Result<()>;

const HOOK_COMMAND: &str = "ecotokens hook";
const GEMINI_HOOK_COMMAND: &str = "ecotokens hook-gemini";
const HOOK_MATCHER: &str = "Bash";

const QWEN_HOOK_COMMAND: &str = "ecotokens hook-qwen";
const POST_HOOK_COMMAND: &str = "ecotokens hook-post";
const POST_HOOK_MATCHER: &str = "Read|Grep|Glob";

const GEMINI_POST_HOOK_COMMAND: &str = "ecotokens hook-post-gemini";
const GEMINI_POST_HOOK_MATCHER: &str = "read_file|search_file_content|list_directory";
const QWEN_POST_HOOK_COMMAND: &str = "ecotokens hook-post-qwen";
const QWEN_POST_HOOK_MATCHER: &str = "read_file|search_files|list_dir";

fn read_settings(path: &Path) -> serde_json::Value {
    if path.exists() {
        let s = std::fs::read_to_string(path).unwrap_or_default();
        match serde_json::from_str(&s) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "ecotokens: warning: {} contains invalid JSON, ignoring: {}",
                    path.display(),
                    e
                );
                serde_json::json!({})
            }
        }
    } else {
        serde_json::json!({})
    }
}

fn write_settings(path: &Path, v: &serde_json::Value) -> InstallResult {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        serde_json::to_string_pretty(v).expect("serde_json: impossible (non-string key)"),
    )
}

// ============================================================================
// Generic helpers for hook manipulation (eliminates Groups 3-6, 9, 15, 18)
// ============================================================================

/// Check if a hook array contains a command.
fn has_hook_command(v: &serde_json::Value, hook_type: &str, command: &str) -> bool {
    v["hooks"][hook_type]
        .as_array()
        .map(|hooks| {
            hooks.iter().any(|h| {
                h["hooks"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|e| e["command"].as_str())
                    .map(|c| c == command)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Check if ecotokens MCP server is registered.
fn has_ecotokens_mcp_server(v: &serde_json::Value) -> bool {
    v["mcpServers"]
        .as_object()
        .map(|m| m.contains_key("ecotokens"))
        .unwrap_or(false)
}

/// Generic hook entry builder.
fn hook_entry(matcher: &str, command: &str) -> serde_json::Value {
    serde_json::json!({
        "matcher": matcher,
        "hooks": [{ "type": "command", "command": command }]
    })
}

/// Install a hook idempotently. Returns true if changed.
#[must_use]
fn install_hook_generic(
    v: &mut serde_json::Value,
    hook_type: &str,
    matcher: &str,
    command: &str,
) -> bool {
    let hooks = v["hooks"][hook_type]
        .as_array_mut()
        .cloned()
        .unwrap_or_default();

    let already_present = hooks.iter().any(|h| {
        h["hooks"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|e| e["command"].as_str())
            .map(|c| c == command)
            .unwrap_or(false)
    });

    let mut new_hooks = hooks;
    if !already_present {
        new_hooks.push(hook_entry(matcher, command));
    }

    v["hooks"][hook_type] = serde_json::Value::Array(new_hooks);
    !already_present
}

/// Remove a hook by command. Returns true if changed.
fn remove_hook_generic(v: &mut serde_json::Value, hook_type: &str, command: &str) -> bool {
    let hooks = v["hooks"][hook_type]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let original_len = hooks.len();

    let filtered: Vec<serde_json::Value> = hooks
        .into_iter()
        .filter(|h| {
            !h["hooks"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|e| e["command"].as_str())
                .map(|c| c == command)
                .unwrap_or(false)
        })
        .collect();

    let changed = filtered.len() != original_len;
    v["hooks"][hook_type] = serde_json::Value::Array(filtered);
    changed
}

/// Remove the ecotokens MCP server entry. Returns true if changed.
fn remove_ecotokens_mcp_server(v: &mut serde_json::Value) -> bool {
    if let Some(servers) = v["mcpServers"].as_object_mut() {
        return servers.remove("ecotokens").is_some();
    }
    false
}

// ============================================================================
// Claude Code - PreToolUse hook
// ============================================================================

/// Install the ecotokens MCP server entry into a settings JSON file (idempotent).
pub fn install_mcp_server(settings_path: &Path) -> InstallResult {
    let binary = std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("ecotokens"))
        .to_string_lossy()
        .into_owned();
    let mut v = read_settings(settings_path);
    if !has_ecotokens_mcp_server(&v) {
        v["mcpServers"]["ecotokens"] = serde_json::json!({
            "command": binary,
            "args": ["mcp-server"]
        });
    }
    write_settings(settings_path, &v)
}

/// Install the PreToolUse hook into ~/.claude/settings.json (idempotent).
pub fn install_hook(settings_path: &Path, claude_json_path: &Path) -> InstallResult {
    let _ = claude_json_path; // kept for signature compatibility with uninstall callers
    let mut v = read_settings(settings_path);
    let _ = install_hook_generic(&mut v, "PreToolUse", HOOK_MATCHER, HOOK_COMMAND);
    write_settings(settings_path, &v)
}

/// Install the PostToolUse hook for Read/Grep/Glob into settings.json (idempotent).
pub fn install_post_hook(settings_path: &Path) -> InstallResult {
    let mut v = read_settings(settings_path);
    let _ = install_hook_generic(&mut v, "PostToolUse", POST_HOOK_MATCHER, POST_HOOK_COMMAND);
    write_settings(settings_path, &v)
}

/// Check if the ecotokens hook is present in settings.json.
pub fn is_hook_installed(settings_path: &Path) -> bool {
    has_hook_command(&read_settings(settings_path), "PreToolUse", HOOK_COMMAND)
}

/// Check if the ecotokens PostToolUse hook is present in settings.json.
pub fn is_post_hook_installed(settings_path: &Path) -> bool {
    has_hook_command(
        &read_settings(settings_path),
        "PostToolUse",
        POST_HOOK_COMMAND,
    )
}

/// Check if the ecotokens MCP server is registered in ~/.claude.json.
pub fn is_mcp_registered(claude_json_path: &Path) -> bool {
    has_ecotokens_mcp_server(&read_settings(claude_json_path))
}

/// Remove all ecotokens hooks and MCP server entry from ~/.claude/settings.json.
/// Also cleans up ~/.claude.json for backward compatibility with older installs.
pub fn uninstall_hook(settings_path: &Path, claude_json_path: &Path) -> InstallResult {
    if settings_path.exists() {
        let mut v = read_settings(settings_path);
        remove_hook_generic(&mut v, "PreToolUse", HOOK_COMMAND);
        remove_hook_generic(&mut v, "PostToolUse", POST_HOOK_COMMAND);
        remove_hook_generic(&mut v, "SessionStart", SESSION_START_COMMAND);
        remove_hook_generic(&mut v, "SessionEnd", SESSION_END_COMMAND);
        remove_ecotokens_mcp_server(&mut v);
        write_settings(settings_path, &v)?;
    }

    // Rétrocompatibilité : anciennes installs où le MCP était dans ~/.claude.json
    if claude_json_path.exists() {
        let mut cv = read_settings(claude_json_path);
        if remove_ecotokens_mcp_server(&mut cv) {
            write_settings(claude_json_path, &cv)?;
        }
    }

    Ok(())
}

// ============================================================================
// Claude Code Session hooks (SessionStart / SessionEnd for auto-watch)
// ============================================================================

const SESSION_START_COMMAND: &str = "ecotokens session-start";
const SESSION_END_COMMAND: &str = "ecotokens session-end";

/// Install SessionStart and SessionEnd hooks in ~/.claude/settings.json (idempotent).
pub fn install_session_hooks(settings_path: &Path) -> InstallResult {
    let mut v = read_settings(settings_path);
    let _ = install_hook_generic(&mut v, "SessionStart", "", SESSION_START_COMMAND);
    let _ = install_hook_generic(&mut v, "SessionEnd", "", SESSION_END_COMMAND);
    write_settings(settings_path, &v)
}

/// Check whether both session hooks are present in settings.json.
pub fn are_session_hooks_installed(settings_path: &Path) -> bool {
    let v = read_settings(settings_path);
    has_hook_command(&v, "SessionStart", SESSION_START_COMMAND)
        && has_hook_command(&v, "SessionEnd", SESSION_END_COMMAND)
}

/// Remove SessionStart and SessionEnd hooks from ~/.claude/settings.json (idempotent).
pub fn uninstall_session_hooks(settings_path: &Path) -> InstallResult {
    if !settings_path.exists() {
        return Ok(());
    }
    let mut v = read_settings(settings_path);
    remove_hook_generic(&mut v, "SessionStart", SESSION_START_COMMAND);
    remove_hook_generic(&mut v, "SessionEnd", SESSION_END_COMMAND);
    write_settings(settings_path, &v)
}

// ============================================================================
// Gemini CLI Support (BeforeTool hook + shared mcpServers in single file)
// ============================================================================

/// Get the default Gemini CLI settings path: ~/.gemini/settings.json
pub fn default_gemini_settings_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|d| d.join(".gemini").join("settings.json"))
}

/// Install the BeforeTool hook into ~/.gemini/settings.json (idempotent).
pub fn install_gemini_hook(settings_path: &Path) -> InstallResult {
    let mut v = read_settings(settings_path);
    let _ = install_hook_generic(
        &mut v,
        "BeforeTool",
        "run_shell_command",
        GEMINI_HOOK_COMMAND,
    );
    write_settings(settings_path, &v)
}

/// Check if the ecotokens BeforeTool hook is already installed.
pub fn is_gemini_hook_installed(settings_path: &Path) -> bool {
    has_hook_command(
        &read_settings(settings_path),
        "BeforeTool",
        GEMINI_HOOK_COMMAND,
    )
}

/// Check if the ecotokens MCP server is registered in ~/.gemini/settings.json.
pub fn is_gemini_mcp_registered(settings_path: &Path) -> bool {
    has_ecotokens_mcp_server(&read_settings(settings_path))
}

/// Install the AfterTool post-hook for read_file/search_file_content/list_directory (idempotent).
pub fn install_gemini_post_hook(settings_path: &Path) -> InstallResult {
    let mut v = read_settings(settings_path);
    let _ = install_hook_generic(
        &mut v,
        "AfterTool",
        GEMINI_POST_HOOK_MATCHER,
        GEMINI_POST_HOOK_COMMAND,
    );
    write_settings(settings_path, &v)
}

/// Check if the Gemini AfterTool post-hook is installed.
pub fn is_gemini_post_hook_installed(settings_path: &Path) -> bool {
    has_hook_command(
        &read_settings(settings_path),
        "AfterTool",
        GEMINI_POST_HOOK_COMMAND,
    )
}

/// Remove the ecotokens hook, post-hook, and MCP server from ~/.gemini/settings.json.
pub fn uninstall_gemini(settings_path: &Path) -> InstallResult {
    if !settings_path.exists() {
        return Ok(());
    }
    let mut v = read_settings(settings_path);
    remove_hook_generic(&mut v, "BeforeTool", GEMINI_HOOK_COMMAND);
    remove_hook_generic(&mut v, "AfterTool", GEMINI_POST_HOOK_COMMAND);
    remove_ecotokens_mcp_server(&mut v);
    write_settings(settings_path, &v)
}

// ============================================================================
// Qwen Code Support (PreToolUse hook + shared mcpServers in single file)
// ============================================================================

/// Get the default Qwen Code settings path: ~/.qwen/settings.json
pub fn default_qwen_settings_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|d| d.join(".qwen").join("settings.json"))
}

/// Install the PreToolUse hook into ~/.qwen/settings.json (idempotent).
pub fn install_qwen_hook(settings_path: &Path) -> InstallResult {
    let mut v = read_settings(settings_path);
    let _ = install_hook_generic(&mut v, "PreToolUse", "run_shell_command", QWEN_HOOK_COMMAND);
    write_settings(settings_path, &v)
}

/// Check if the ecotokens PreToolUse hook is already installed.
pub fn is_qwen_hook_installed(settings_path: &Path) -> bool {
    has_hook_command(
        &read_settings(settings_path),
        "PreToolUse",
        QWEN_HOOK_COMMAND,
    )
}

/// Check if the ecotokens MCP server is registered in ~/.qwen/settings.json.
pub fn is_qwen_mcp_registered(settings_path: &Path) -> bool {
    has_ecotokens_mcp_server(&read_settings(settings_path))
}

/// Install the PostToolUse post-hook for read_file/search_files/list_dir (idempotent).
pub fn install_qwen_post_hook(settings_path: &Path) -> InstallResult {
    let mut v = read_settings(settings_path);
    let _ = install_hook_generic(
        &mut v,
        "PostToolUse",
        QWEN_POST_HOOK_MATCHER,
        QWEN_POST_HOOK_COMMAND,
    );
    write_settings(settings_path, &v)
}

/// Check if the Qwen PostToolUse post-hook is installed.
pub fn is_qwen_post_hook_installed(settings_path: &Path) -> bool {
    has_hook_command(
        &read_settings(settings_path),
        "PostToolUse",
        QWEN_POST_HOOK_COMMAND,
    )
}

/// Remove the ecotokens hook, post-hook, and MCP server from ~/.qwen/settings.json.
pub fn uninstall_qwen(settings_path: &Path) -> InstallResult {
    if !settings_path.exists() {
        return Ok(());
    }
    let mut v = read_settings(settings_path);
    remove_hook_generic(&mut v, "PreToolUse", QWEN_HOOK_COMMAND);
    remove_hook_generic(&mut v, "PostToolUse", QWEN_POST_HOOK_COMMAND);
    remove_ecotokens_mcp_server(&mut v);
    write_settings(settings_path, &v)
}

// ============================================================================
// Hermes Agent Support (user plugin in ~/.hermes/plugins/ecotokens/)
// ============================================================================

const HERMES_PLUGIN_MANIFEST: &str = r#"name: ecotokens
version: "0.1.0"
description: "Compress Hermes Agent tool outputs with ecotokens before they enter model context"
author: "ecotokens"
kind: standalone
provides_hooks:
  - transform_terminal_output
  - transform_tool_result
  - on_session_start
  - on_session_end
"#;

fn hermes_plugin_init_content(binary: &str) -> String {
    format!(
        r#"# Hermes Agent plugin generated by ecotokens.
# The plugin is intentionally fail-open: if ecotokens is missing, slow, or
# returns an error, Hermes receives the original tool output.

from __future__ import annotations

import os
import subprocess
from typing import Any

ECOTOKENS_BIN = os.environ.get("ECOTOKENS_BIN", {binary:?})
MIN_CHARS = int(os.environ.get("ECOTOKENS_HERMES_MIN_CHARS", "2000"))
TIMEOUT_SECONDS = float(os.environ.get("ECOTOKENS_HERMES_TIMEOUT", "10"))


def _should_filter(text: Any) -> bool:
    return isinstance(text, str) and len(text) >= MIN_CHARS


def _filter_existing_output(command: str, output: str, exit_code: int = 0, cwd: str | None = None, hook_type: str = "transform-terminal-output") -> str | None:
    if not _should_filter(output):
        return None
    args = [
        ECOTOKENS_BIN,
        "filter-output",
        "--command",
        command or "hermes-tool",
        "--exit-code",
        str(exit_code),
        "--hook-type",
        hook_type,
    ]
    if cwd:
        args.extend(["--cwd", cwd])
    try:
        completed = subprocess.run(
            args,
            input=output,
            text=True,
            capture_output=True,
            timeout=TIMEOUT_SECONDS,
            check=False,
        )
    except Exception:
        return None

    if completed.returncode != 0:
        return None
    if not completed.stdout:
        return None
    return completed.stdout


def transform_terminal_output(command: str = "", output: str = "", exit_code: int = 0, cwd: str | None = None, **_: Any) -> str | None:
    first_token = (command.split() or [""])[0]
    if first_token in (ECOTOKENS_BIN, "ecotokens"):
        return None
    return _filter_existing_output(command, output, exit_code, cwd, "transform-terminal-output")


def transform_tool_result(tool_name: str = "", result: str = "", cwd: str | None = None, **_: Any) -> str | None:
    # The terminal tool has a more precise hook above that sees raw stdout
    # before Hermes builds its JSON wrapper. Avoid double filtering it here.
    if tool_name == "terminal":
        return None
    return _filter_existing_output(f"hermes-tool:{{tool_name}}", result, 0, cwd, "transform-tool-result")


def on_session_start(session_id: str = "", model: str = "", platform: str = "", **_: Any) -> None:
    # Start ecotokens watch in background if auto-watch is enabled in config.
    # Fail-open: any error is silently ignored so Hermes is never blocked.
    try:
        import subprocess as _sp
        _sp.Popen(
            [ECOTOKENS_BIN, "session-start"],
            stdout=_sp.DEVNULL,
            stderr=_sp.DEVNULL,
            close_fds=True,
        )
    except Exception:
        pass


def on_session_end(session_id: str = "", completed: bool = False, interrupted: bool = False, model: str = "", platform: str = "", **_: Any) -> None:
    # Stop the background watcher started by on_session_start.
    # Use a short timeout so Hermes can exit even if ecotokens hangs.
    try:
        import subprocess as _sp
        _sp.run(
            [ECOTOKENS_BIN, "session-end"],
            stdout=_sp.DEVNULL,
            stderr=_sp.DEVNULL,
            timeout=5,
            check=False,
        )
    except Exception:
        pass


def register(ctx):
    ctx.register_hook("transform_terminal_output", transform_terminal_output)
    ctx.register_hook("transform_tool_result", transform_tool_result)
    ctx.register_hook("on_session_start", on_session_start)
    ctx.register_hook("on_session_end", on_session_end)
"#
    )
}

/// Get the default Hermes Agent plugin directory: ~/.hermes/plugins/ecotokens
pub fn default_hermes_plugin_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HERMES_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|d| d.join(".hermes")))
        .map(|d| d.join("plugins").join("ecotokens"))
}

/// Get the default Hermes Agent config file: ~/.hermes/config.yaml
pub fn default_hermes_config_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HERMES_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|d| d.join(".hermes")))
        .map(|d| d.join("config.yaml"))
}

/// Add `plugin` to the `plugins.enabled` list in YAML content.
/// Preserves all existing content and indentation. Idempotent.
fn yaml_add_to_plugins_enabled(content: &str, plugin: &str) -> String {
    let plugin_item = format!("- {}", plugin);

    // Already present — no-op.
    if content.lines().any(|l| l.trim() == plugin_item) {
        return content.to_string();
    }

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

    // Locate the top-level `plugins:` key.
    let plugins_idx = lines.iter().position(|l| {
        let t = l.trim();
        t == "plugins:" || t.starts_with("plugins: ")
    });

    if let Some(pi) = plugins_idx {
        // Look for `  enabled:` inside the plugins block.
        let enabled_rel = lines[pi + 1..].iter().position(|l| {
            l.starts_with("  ") && {
                let t = l.trim();
                t == "enabled:" || t.starts_with("enabled:") && t.contains(':')
            }
        });

        if let Some(ei_rel) = enabled_rel {
            let ei = pi + 1 + ei_rel;
            // Skip existing list items to find the insertion point.
            let skip = lines[ei + 1..]
                .iter()
                .take_while(|l| l.starts_with("    -"))
                .count();
            lines.insert(ei + 1 + skip, format!("    - {}", plugin));
        } else {
            // `plugins:` exists but has no `enabled:` key — add it.
            lines.insert(pi + 1, "  enabled:".to_string());
            lines.insert(pi + 2, format!("    - {}", plugin));
        }
    } else {
        // No `plugins:` section at all — append one.
        if !lines.is_empty() && !lines.last().map(|l| l.is_empty()).unwrap_or(true) {
            lines.push(String::new());
        }
        lines.push("plugins:".to_string());
        lines.push("  enabled:".to_string());
        lines.push(format!("    - {}", plugin));
    }

    let mut result = lines.join("\n");
    if content.ends_with('\n') || content.is_empty() {
        result.push('\n');
    }
    result
}

/// Add `ecotokens` to `plugins.enabled` in the Hermes config file.
/// Creates the file and any missing structure if needed. Idempotent.
pub fn enable_hermes_plugin_in_config(config_path: &Path) -> InstallResult {
    let content = if config_path.exists() {
        std::fs::read_to_string(config_path)?
    } else {
        String::new()
    };
    let new_content = yaml_add_to_plugins_enabled(&content, "ecotokens");
    if new_content == content {
        return Ok(());
    }
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(config_path, new_content)
}

/// Return true if `ecotokens` appears in `plugins.enabled` in the Hermes config.
pub fn is_hermes_plugin_enabled_in_config(config_path: &Path) -> bool {
    if !config_path.exists() {
        return false;
    }
    std::fs::read_to_string(config_path)
        .map(|c| c.lines().any(|l| l.trim() == "- ecotokens"))
        .unwrap_or(false)
}

/// Install the ecotokens Hermes Agent plugin (idempotent).
pub fn install_hermes_plugin(plugin_dir: &Path) -> InstallResult {
    std::fs::create_dir_all(plugin_dir)?;
    std::fs::write(plugin_dir.join("plugin.yaml"), HERMES_PLUGIN_MANIFEST)?;
    let binary = std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("ecotokens"))
        .to_string_lossy()
        .into_owned();
    std::fs::write(
        plugin_dir.join("__init__.py"),
        hermes_plugin_init_content(&binary),
    )
}

/// Check if the ecotokens Hermes Agent plugin is installed.
pub fn is_hermes_plugin_installed(plugin_dir: &Path) -> bool {
    plugin_dir.join("plugin.yaml").exists() && plugin_dir.join("__init__.py").exists()
}

/// Remove the ecotokens Hermes Agent plugin directory.
pub fn uninstall_hermes_plugin(plugin_dir: &Path) -> InstallResult {
    if plugin_dir.exists() {
        std::fs::remove_dir_all(plugin_dir)?;
    }
    Ok(())
}

// ============================================================================
// Codex Support (plugin in ~/.codex/plugins/ecotokens/)
// ============================================================================

const CODEX_PLUGIN_MANIFEST: &str = r#"{
  "name": "ecotokens",
  "version": "0.1.0",
  "description": "Keep the ecotokens index warm during Codex sessions.",
  "author": {
    "name": "ecotokens"
  },
  "license": "MIT",
  "keywords": [
    "codex",
    "watch",
    "index"
  ],
  "interface": {
    "displayName": "ecotokens",
    "shortDescription": "Starts ecotokens watch with Codex sessions",
    "longDescription": "Installs a Codex SessionStart hook that calls ecotokens session-start so auto-watch can keep the project index up to date.",
    "developerName": "ecotokens",
    "category": "Developer Tools",
    "capabilities": [
      "Read"
    ],
    "defaultPrompt": []
  }
}
"#;

/// Get the default Codex plugin directory: ~/.codex/plugins/ecotokens
pub fn default_codex_plugin_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|d| d.join(".codex")))
        .map(|d| d.join("plugins").join("ecotokens"))
}

/// Install the ecotokens Codex plugin (idempotent).
pub fn install_codex_plugin(plugin_dir: &Path) -> InstallResult {
    std::fs::create_dir_all(plugin_dir.join(".codex-plugin"))?;
    std::fs::write(
        plugin_dir.join(".codex-plugin").join("plugin.json"),
        CODEX_PLUGIN_MANIFEST,
    )?;
    // Clean up stale hooks files written by older installs.
    for stale in &[
        plugin_dir.join("hooks.json"),
        plugin_dir.join("hooks").join("hooks.json"),
    ] {
        if stale.exists() {
            let _ = std::fs::remove_file(stale);
        }
    }
    Ok(())
}

/// Check if the ecotokens Codex plugin is installed.
pub fn is_codex_plugin_installed(plugin_dir: &Path) -> bool {
    plugin_dir
        .join(".codex-plugin")
        .join("plugin.json")
        .exists()
}

/// Remove the ecotokens Codex plugin directory.
pub fn uninstall_codex_plugin(plugin_dir: &Path) -> InstallResult {
    if plugin_dir.exists() {
        std::fs::remove_dir_all(plugin_dir)?;
    }
    Ok(())
}

// ============================================================================
// Pi Support (extension TypeScript déposée dans ~/.pi/agent/extensions/)
// ============================================================================

const PI_EXTENSION_CONTENT: &str = include_str!("pi_extension.ts");

/// Get the default Pi extension path: ~/.pi/agent/extensions/ecotokens.ts
pub fn default_pi_extension_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|d| {
        d.join(".pi")
            .join("agent")
            .join("extensions")
            .join("ecotokens.ts")
    })
}

/// Install the ecotokens extension for Pi (idempotent).
pub fn install_pi_extension(extension_path: &Path) -> InstallResult {
    if let Some(parent) = extension_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(extension_path, PI_EXTENSION_CONTENT)
}

/// Check if the ecotokens Pi extension is installed.
pub fn is_pi_extension_installed(extension_path: &Path) -> bool {
    extension_path.exists()
}

/// Remove the ecotokens Pi extension.
pub fn uninstall_pi(extension_path: &Path) -> InstallResult {
    if extension_path.exists() {
        std::fs::remove_file(extension_path)?;
    }
    Ok(())
}
