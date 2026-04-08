use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HookInput {
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone)]
pub enum HookOutput {
    Passthrough,
    Rewrite(String),
}

/// Shared payload structure for Gemini BeforeTool and Qwen PreToolUse hooks.
#[derive(Debug, Deserialize)]
struct ShellToolPayload {
    tool_name: String,
    tool_input: serde_json::Value,
    #[serde(default)]
    cwd: Option<String>,
}

/// Shared response structure for shell-tool hooks.
#[derive(Debug, Serialize)]
struct ShellHookResponse {
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: ShellHookSpecificOutput,
}

#[derive(Debug, Serialize)]
struct ShellHookSpecificOutput {
    #[serde(rename = "hookEventName")]
    hook_event_name: String,
    decision: String,
    #[serde(rename = "toolInput", skip_serializing_if = "Option::is_none")]
    tool_input: Option<serde_json::Value>,
}

/// Determine hook action for a given command and exclusion list.
pub fn handle_hook_input(input: &HookInput, exclusions: &[String], _debug: bool) -> HookOutput {
    let cmd = input.command.trim();

    // Check exclusion list (prefix match)
    for exclusion in exclusions {
        if cmd.starts_with(exclusion.as_str()) || cmd == exclusion.as_str() {
            return HookOutput::Passthrough;
        }
    }

    // Rewrite to ecotokens filter
    let rewritten = match &input.cwd {
        Some(cwd) => format!("ecotokens filter --cwd \"{cwd}\" -- {cmd}"),
        None => format!("ecotokens filter -- {cmd}"),
    };
    HookOutput::Rewrite(rewritten)
}

/// Top-level hook stdin→stdout handler (reads Claude Code PreToolUse JSON).
pub fn handle() {
    use super::MAX_STDIN_BYTES;
    use std::io::Read;

    let mut stdin = String::new();
    std::io::stdin()
        .take(MAX_STDIN_BYTES as u64 + 1)
        .read_to_string(&mut stdin)
        .unwrap_or_default();

    if stdin.len() > MAX_STDIN_BYTES {
        print!("{stdin}");
        return;
    }

    let v: serde_json::Value = match serde_json::from_str(&stdin) {
        Ok(v) => v,
        Err(_) => {
            // Cannot parse — passthrough
            print!("{stdin}");
            return;
        }
    };

    let command = v["tool_input"]["command"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let cwd = v["cwd"].as_str().map(|s| s.to_string());
    let settings = crate::config::Settings::load();
    let input = HookInput { command, cwd };
    let debug = settings.debug;
    let output = handle_hook_input(&input, &settings.exclusions, debug);

    let response = match output {
        HookOutput::Passthrough => serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow"
            }
        }),
        HookOutput::Rewrite(new_cmd) => {
            if debug {
                eprintln!(
                    "[ecotokens debug] rewriting: {} → {}",
                    input.command, new_cmd
                );
            }
            serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "updatedInput": {
                        "command": new_cmd
                    }
                }
            })
        }
    };

    match serde_json::to_string(&response) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("ecotokens hook: failed to serialize response: {e}"),
    }
}

/// Emit a shell-tool allow response (Gemini or Qwen format).
fn emit_allow(hook_event_name: &str, updated_input: Option<serde_json::Value>) {
    let response = ShellHookResponse {
        hook_specific_output: ShellHookSpecificOutput {
            hook_event_name: hook_event_name.to_string(),
            decision: "allow".to_string(),
            tool_input: updated_input,
        },
    };
    if let Ok(s) = serde_json::to_string(&response) {
        println!("{s}");
    }
}

/// Common handler for Gemini BeforeTool and Qwen PreToolUse shell-tool hooks.
/// Reads a JSON payload with `tool_name` and `tool_input`, rewrites `tool_input.command`
/// for shell tools, and emits a response using `hook_event_name`.
fn handle_shell_tool_hook(hook_event_name: &str, label: &str) {
    use super::MAX_STDIN_BYTES;
    use std::io::Read;

    let mut stdin = String::new();
    std::io::stdin()
        .take(MAX_STDIN_BYTES as u64 + 1)
        .read_to_string(&mut stdin)
        .unwrap_or_default();

    if stdin.len() > MAX_STDIN_BYTES {
        emit_allow(hook_event_name, None);
        return;
    }

    let payload: ShellToolPayload = match serde_json::from_str(&stdin) {
        Ok(p) => p,
        Err(_) => {
            emit_allow(hook_event_name, None);
            return;
        }
    };

    if payload.tool_name != "run_shell_command" {
        emit_allow(hook_event_name, None);
        return;
    }

    let command = payload.tool_input["command"]
        .as_str()
        .unwrap_or("")
        .to_string();
    if command.is_empty() {
        emit_allow(hook_event_name, None);
        return;
    }

    let settings = crate::config::Settings::load();
    let input = HookInput {
        command,
        cwd: payload.cwd,
    };
    let debug = settings.debug;
    let output = handle_hook_input(&input, &settings.exclusions, debug);

    match output {
        HookOutput::Passthrough => emit_allow(hook_event_name, None),
        HookOutput::Rewrite(new_cmd) => {
            if debug {
                eprintln!(
                    "[ecotokens debug] rewriting ({label}): {} → {}",
                    input.command, new_cmd
                );
            }
            let mut tool_input = payload.tool_input.clone();
            tool_input["command"] = serde_json::Value::String(new_cmd);
            emit_allow(hook_event_name, Some(tool_input));
        }
    }
}

/// Top-level hook stdin→stdout handler for Gemini CLI BeforeTool events.
pub fn handle_gemini() {
    handle_shell_tool_hook("BeforeTool", "gemini");
}

/// Top-level hook stdin→stdout handler for Qwen Code PreToolUse events.
pub fn handle_qwen() {
    handle_shell_tool_hook("PreToolUse", "qwen");
}
