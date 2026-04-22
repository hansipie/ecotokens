use std::path::Path;

pub type InstallResult = std::io::Result<()>;

const HOOK_COMMAND: &str = "ecotokens hook";
const GEMINI_HOOK_COMMAND: &str = "ecotokens hook-gemini";
const HOOK_MATCHER: &str = "Bash";

const QWEN_HOOK_COMMAND: &str = "ecotokens hook-qwen";
const POST_HOOK_COMMAND: &str = "ecotokens hook-post";
const POST_HOOK_MATCHER: &str = "Read|Grep|Glob";

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

/// Check if the ecotokens MCP server is registered in ~/.claude.json.
pub fn is_mcp_registered(claude_json_path: &Path) -> bool {
    has_ecotokens_mcp_server(&read_settings(claude_json_path))
}

/// Remove the ecotokens PreToolUse hook from ~/.claude/settings.json and
/// the MCP server entry from ~/.claude.json (both idempotent).
pub fn uninstall_hook(settings_path: &Path, claude_json_path: &Path) -> InstallResult {
    if settings_path.exists() {
        let mut v = read_settings(settings_path);
        remove_hook_generic(&mut v, "PreToolUse", HOOK_COMMAND);
        write_settings(settings_path, &v)?;
    }

    if claude_json_path.exists() {
        let mut cv = read_settings(claude_json_path);
        remove_ecotokens_mcp_server(&mut cv);
        write_settings(claude_json_path, &cv)?;
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

/// Shared logic: remove a hook + the MCP server entry from a settings file.
fn uninstall_hook_and_mcp(settings_path: &Path, hook_type: &str, command: &str) -> InstallResult {
    if !settings_path.exists() {
        return Ok(());
    }
    let mut v = read_settings(settings_path);
    remove_hook_generic(&mut v, hook_type, command);
    remove_ecotokens_mcp_server(&mut v);
    write_settings(settings_path, &v)
}

/// Remove the ecotokens hook and MCP server from ~/.gemini/settings.json.
pub fn uninstall_gemini(settings_path: &Path) -> InstallResult {
    uninstall_hook_and_mcp(settings_path, "BeforeTool", GEMINI_HOOK_COMMAND)
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

/// Remove the ecotokens hook and MCP server from ~/.qwen/settings.json.
pub fn uninstall_qwen(settings_path: &Path) -> InstallResult {
    uninstall_hook_and_mcp(settings_path, "PreToolUse", QWEN_HOOK_COMMAND)
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
