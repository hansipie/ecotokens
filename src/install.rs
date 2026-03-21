use std::path::Path;

pub type InstallResult = std::io::Result<()>;

const HOOK_COMMAND: &str = "ecotokens hook";
const GEMINI_HOOK_COMMAND: &str = "ecotokens hook-gemini";
const HOOK_MATCHER: &str = "Bash";

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
    std::fs::write(path, serde_json::to_string_pretty(v).unwrap())
}

fn ecotokens_hook_entry() -> serde_json::Value {
    serde_json::json!({
        "matcher": HOOK_MATCHER,
        "hooks": [{
            "type": "command",
            "command": HOOK_COMMAND
        }]
    })
}

/// Install the PreToolUse hook into ~/.claude/settings.json (idempotent).
pub fn install_hook(settings_path: &Path, claude_json_path: &Path) -> InstallResult {
    let _ = claude_json_path; // kept for signature compatibility with uninstall callers
    let mut v = read_settings(settings_path);

    let hooks = v["hooks"]["PreToolUse"]
        .as_array_mut()
        .cloned()
        .unwrap_or_default();

    // Check if already present
    let already_present = hooks.iter().any(|h| {
        h["hooks"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|e| e["command"].as_str())
            .map(|c| c == HOOK_COMMAND)
            .unwrap_or(false)
    });

    let mut new_hooks = hooks;
    if !already_present {
        new_hooks.push(ecotokens_hook_entry());
    }

    v["hooks"]["PreToolUse"] = serde_json::Value::Array(new_hooks);
    write_settings(settings_path, &v)
}

/// Check if the ecotokens hook is present in settings.json.
pub fn is_hook_installed(settings_path: &Path) -> bool {
    let v = read_settings(settings_path);
    v["hooks"]["PreToolUse"]
        .as_array()
        .map(|hooks| {
            hooks.iter().any(|h| {
                h["hooks"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|e| e["command"].as_str())
                    .map(|c| c == HOOK_COMMAND)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn has_ecotokens_mcp_server(v: &serde_json::Value) -> bool {
    v["mcpServers"]
        .as_object()
        .map(|m| m.contains_key("ecotokens"))
        .unwrap_or(false)
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

        let hooks = v["hooks"]["PreToolUse"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let filtered: Vec<serde_json::Value> = hooks
            .into_iter()
            .filter(|h| {
                !h["hooks"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|e| e["command"].as_str())
                    .map(|c| c == HOOK_COMMAND)
                    .unwrap_or(false)
            })
            .collect();

        v["hooks"]["PreToolUse"] = serde_json::Value::Array(filtered);
        write_settings(settings_path, &v)?;
    }

    if claude_json_path.exists() {
        let mut cv = read_settings(claude_json_path);
        if let Some(servers) = cv["mcpServers"].as_object_mut() {
            servers.remove("ecotokens");
        }
        write_settings(claude_json_path, &cv)?;
    }

    Ok(())
}

// ============================================================================
// Claude Code Session hooks (SessionStart / SessionEnd for auto-watch)
// ============================================================================

const SESSION_START_COMMAND: &str = "ecotokens session-start";
const SESSION_END_COMMAND: &str = "ecotokens session-end";

fn session_start_hook_entry() -> serde_json::Value {
    serde_json::json!({
        "matcher": "",
        "hooks": [{ "type": "command", "command": SESSION_START_COMMAND }]
    })
}

fn session_end_hook_entry() -> serde_json::Value {
    serde_json::json!({
        "matcher": "",
        "hooks": [{ "type": "command", "command": SESSION_END_COMMAND }]
    })
}

fn hook_command_matches(h: &serde_json::Value, cmd: &str) -> bool {
    h["hooks"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|e| e["command"].as_str())
        .map(|c| c == cmd)
        .unwrap_or(false)
}

/// Install SessionStart and SessionEnd hooks in ~/.claude/settings.json (idempotent).
pub fn install_session_hooks(settings_path: &Path) -> InstallResult {
    let mut v = read_settings(settings_path);

    // SessionStart
    let mut starts = v["hooks"]["SessionStart"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if !starts
        .iter()
        .any(|h| hook_command_matches(h, SESSION_START_COMMAND))
    {
        starts.push(session_start_hook_entry());
    }
    v["hooks"]["SessionStart"] = serde_json::Value::Array(starts);

    // SessionEnd
    let mut ends = v["hooks"]["SessionEnd"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if !ends
        .iter()
        .any(|h| hook_command_matches(h, SESSION_END_COMMAND))
    {
        ends.push(session_end_hook_entry());
    }
    v["hooks"]["SessionEnd"] = serde_json::Value::Array(ends);

    write_settings(settings_path, &v)
}

/// Check whether both session hooks are present in settings.json.
pub fn are_session_hooks_installed(settings_path: &Path) -> bool {
    let v = read_settings(settings_path);
    let start_ok = v["hooks"]["SessionStart"]
        .as_array()
        .map(|a| {
            a.iter()
                .any(|h| hook_command_matches(h, SESSION_START_COMMAND))
        })
        .unwrap_or(false);
    let end_ok = v["hooks"]["SessionEnd"]
        .as_array()
        .map(|a| {
            a.iter()
                .any(|h| hook_command_matches(h, SESSION_END_COMMAND))
        })
        .unwrap_or(false);
    start_ok && end_ok
}

/// Remove SessionStart and SessionEnd hooks from ~/.claude/settings.json (idempotent).
#[allow(dead_code)]
pub fn uninstall_session_hooks(settings_path: &Path) -> InstallResult {
    if !settings_path.exists() {
        return Ok(());
    }
    let mut v = read_settings(settings_path);

    if let Some(arr) = v["hooks"]["SessionStart"].as_array() {
        let filtered: Vec<_> = arr
            .iter()
            .filter(|h| !hook_command_matches(h, SESSION_START_COMMAND))
            .cloned()
            .collect();
        v["hooks"]["SessionStart"] = serde_json::Value::Array(filtered);
    }
    if let Some(arr) = v["hooks"]["SessionEnd"].as_array() {
        let filtered: Vec<_> = arr
            .iter()
            .filter(|h| !hook_command_matches(h, SESSION_END_COMMAND))
            .cloned()
            .collect();
        v["hooks"]["SessionEnd"] = serde_json::Value::Array(filtered);
    }

    write_settings(settings_path, &v)
}

// ============================================================================
// Gemini CLI Support (BeforeTool hook + shared mcpServers in single file)
// ============================================================================

/// Get the default Gemini CLI settings path: ~/.gemini/settings.json
pub fn default_gemini_settings_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|d| d.join(".gemini").join("settings.json"))
}

/// Create a Gemini BeforeTool hook entry (similar structure to Claude but
/// for the BeforeTool event which handles shell command interception).
fn gemini_hook_entry() -> serde_json::Value {
    serde_json::json!({
        "matcher": "run_shell_command",
        "hooks": [{
            "type": "command",
            "command": "ecotokens hook-gemini"
        }]
    })
}

/// Install the BeforeTool hook into ~/.gemini/settings.json (idempotent).
/// The hook is used to intercept and rewrite shell commands.
pub fn install_gemini_hook(settings_path: &Path) -> InstallResult {
    let mut v = read_settings(settings_path);

    let hooks = v["hooks"]["BeforeTool"]
        .as_array_mut()
        .cloned()
        .unwrap_or_default();

    // Check if already present
    let already_present = hooks.iter().any(|h| {
        h["hooks"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|e| e["command"].as_str())
            .map(|c| c == GEMINI_HOOK_COMMAND)
            .unwrap_or(false)
    });

    let mut new_hooks = hooks;
    if !already_present {
        new_hooks.push(gemini_hook_entry());
    }

    v["hooks"]["BeforeTool"] = serde_json::Value::Array(new_hooks);
    write_settings(settings_path, &v)?;

    Ok(())
}

/// Check if the ecotokens BeforeTool hook is already installed in
/// ~/.gemini/settings.json.
pub fn is_gemini_hook_installed(settings_path: &Path) -> bool {
    let v = read_settings(settings_path);
    v["hooks"]["BeforeTool"]
        .as_array()
        .map(|hooks| {
            hooks.iter().any(|h| {
                h["hooks"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|e| e["command"].as_str())
                    .map(|c| c == GEMINI_HOOK_COMMAND)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Check if the ecotokens MCP server is registered in ~/.gemini/settings.json.
pub fn is_gemini_mcp_registered(settings_path: &Path) -> bool {
    has_ecotokens_mcp_server(&read_settings(settings_path))
}

/// Remove the ecotokens hook and MCP server from ~/.gemini/settings.json.
/// Idempotent: no error if they're not present.
pub fn uninstall_gemini(settings_path: &Path) -> InstallResult {
    if !settings_path.exists() {
        return Ok(());
    }

    let mut v = read_settings(settings_path);

    // Remove hook from BeforeTool hooks array
    if let Some(hooks) = v["hooks"]["BeforeTool"].as_array() {
        let filtered: Vec<serde_json::Value> = hooks
            .iter()
            .filter(|h| {
                !h["hooks"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|e| e["command"].as_str())
                    .map(|c| c == GEMINI_HOOK_COMMAND)
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        v["hooks"]["BeforeTool"] = serde_json::Value::Array(filtered);
    }

    // Remove MCP server entry
    if let Some(servers) = v["mcpServers"].as_object_mut() {
        servers.remove("ecotokens");
    }

    write_settings(settings_path, &v)?;

    Ok(())
}
