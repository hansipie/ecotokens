use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HookInput {
    pub command: String,
}

#[derive(Debug, Clone)]
pub enum HookOutput {
    Passthrough,
    Rewrite(String),
}

/// Gemini hook payload structure.
#[derive(Debug, Deserialize)]
struct GeminiHookPayload {
    tool_name: String,
    tool_input: serde_json::Value,
}

/// Gemini hook response structure.
#[derive(Debug, Serialize)]
struct GeminiHookResponse {
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: GeminiHookSpecificOutput,
}

#[derive(Debug, Serialize)]
struct GeminiHookSpecificOutput {
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
    let rewritten = format!("ecotokens filter -- {cmd}");
    HookOutput::Rewrite(rewritten)
}

/// Top-level hook stdin→stdout handler (reads Claude Code PreToolUse JSON).
pub fn handle() {
    use std::io::Read;

    let mut stdin = String::new();
    std::io::stdin()
        .read_to_string(&mut stdin)
        .unwrap_or_default();

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
    let settings = crate::config::Settings::load();
    let input = HookInput { command };
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

/// Helper to emit Gemini allow response.
fn emit_gemini_allow(updated_input: Option<serde_json::Value>) {
    let response = GeminiHookResponse {
        hook_specific_output: GeminiHookSpecificOutput {
            hook_event_name: "BeforeTool".to_string(),
            decision: "allow".to_string(),
            tool_input: updated_input,
        },
    };
    if let Ok(s) = serde_json::to_string(&response) {
        println!("{s}");
    }
}

/// Qwen Code hook payload structure (PreToolUse, same layout as Gemini).
#[derive(Debug, Deserialize)]
struct QwenHookPayload {
    tool_name: String,
    tool_input: serde_json::Value,
}

/// Qwen Code hook response structure.
#[derive(Debug, Serialize)]
struct QwenHookResponse {
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: QwenHookSpecificOutput,
}

#[derive(Debug, Serialize)]
struct QwenHookSpecificOutput {
    #[serde(rename = "hookEventName")]
    hook_event_name: String,
    decision: String,
    #[serde(rename = "toolInput", skip_serializing_if = "Option::is_none")]
    tool_input: Option<serde_json::Value>,
}

/// Helper to emit Qwen allow response.
fn emit_qwen_allow(updated_input: Option<serde_json::Value>) {
    let response = QwenHookResponse {
        hook_specific_output: QwenHookSpecificOutput {
            hook_event_name: "PreToolUse".to_string(),
            decision: "allow".to_string(),
            tool_input: updated_input,
        },
    };
    if let Ok(s) = serde_json::to_string(&response) {
        println!("{s}");
    }
}

/// Top-level hook stdin→stdout handler for Qwen Code PreToolUse events.
/// Reads a JSON payload with `tool_name` and `tool_input`,
/// rewrites `tool_input.command` for shell tools, and emits a Qwen-compatible response.
pub fn handle_qwen() {
    use std::io::Read;

    let mut stdin = String::new();
    std::io::stdin()
        .read_to_string(&mut stdin)
        .unwrap_or_default();

    let payload: QwenHookPayload = match serde_json::from_str(&stdin) {
        Ok(p) => p,
        Err(_) => {
            emit_qwen_allow(None);
            return;
        }
    };

    if payload.tool_name != "run_shell_command" {
        emit_qwen_allow(None);
        return;
    }

    let command = payload.tool_input["command"]
        .as_str()
        .unwrap_or("")
        .to_string();
    if command.is_empty() {
        emit_qwen_allow(None);
        return;
    }

    let settings = crate::config::Settings::load();
    let input = HookInput { command };
    let debug = settings.debug;
    let output = handle_hook_input(&input, &settings.exclusions, debug);

    match output {
        HookOutput::Passthrough => emit_qwen_allow(None),
        HookOutput::Rewrite(new_cmd) => {
            if debug {
                eprintln!(
                    "[ecotokens debug] rewriting (qwen): {} → {}",
                    input.command, new_cmd
                );
            }
            let mut tool_input = payload.tool_input.clone();
            tool_input["command"] = serde_json::Value::String(new_cmd);
            emit_qwen_allow(Some(tool_input));
        }
    }
}

/// Top-level hook stdin→stdout handler for Gemini CLI BeforeTool events.
/// Reads a JSON payload with `tool_name` and `tool_input`,
/// rewrites `tool_input.command` for shell tools, and emits a Gemini-compatible response.
pub fn handle_gemini() {
    use std::io::Read;

    let mut stdin = String::new();
    std::io::stdin()
        .read_to_string(&mut stdin)
        .unwrap_or_default();

    let payload: GeminiHookPayload = match serde_json::from_str(&stdin) {
        Ok(p) => p,
        Err(_) => {
            // Cannot parse — passthrough
            emit_gemini_allow(None);
            return;
        }
    };

    // Only intercept shell commands; let all other tools pass through unmodified.
    if payload.tool_name != "run_shell_command" {
        emit_gemini_allow(None);
        return;
    }

    let command = payload.tool_input["command"]
        .as_str()
        .unwrap_or("")
        .to_string();
    if command.is_empty() {
        emit_gemini_allow(None);
        return;
    }

    let settings = crate::config::Settings::load();
    let input = HookInput { command };
    let debug = settings.debug;
    let output = handle_hook_input(&input, &settings.exclusions, debug);

    match output {
        HookOutput::Passthrough => emit_gemini_allow(None),
        HookOutput::Rewrite(new_cmd) => {
            if debug {
                eprintln!(
                    "[ecotokens debug] rewriting (gemini): {} → {}",
                    input.command, new_cmd
                );
            }
            // Merge the rewritten command into tool_input, preserving other fields.
            let mut tool_input = payload.tool_input.clone();
            tool_input["command"] = serde_json::Value::String(new_cmd);
            emit_gemini_allow(Some(tool_input));
        }
    }
}
