use std::path::Path;

pub type InstallResult = std::io::Result<()>;

const HOOK_COMMAND: &str = "ecotokens hook";
const HOOK_MATCHER: &str = "Bash";

fn read_settings(path: &Path) -> serde_json::Value {
    if path.exists() {
        let s = std::fs::read_to_string(path).unwrap_or_default();
        serde_json::from_str(&s).unwrap_or(serde_json::json!({}))
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
pub fn install_hook(settings_path: &Path, with_mcp: bool) -> InstallResult {
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

    if with_mcp {
        let mcp_entry = serde_json::json!({
            "command": "ecotokens mcp",
            "type": "stdio"
        });
        let servers = v["mcpServers"].as_object_mut().cloned().unwrap_or_default();
        let mut new_servers = servers;
        new_servers.entry("ecotokens").or_insert(mcp_entry);
        v["mcpServers"] = serde_json::Value::Object(new_servers);
    }

    write_settings(settings_path, &v)
}

/// Remove the ecotokens PreToolUse hook from ~/.claude/settings.json (idempotent).
pub fn uninstall_hook(settings_path: &Path) -> InstallResult {
    if !settings_path.exists() {
        return Ok(());
    }
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
    write_settings(settings_path, &v)
}
