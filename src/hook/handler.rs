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
    std::io::stdin().read_to_string(&mut stdin).unwrap_or_default();

    let v: serde_json::Value = match serde_json::from_str(&stdin) {
        Ok(v) => v,
        Err(_) => {
            // Cannot parse — passthrough
            print!("{stdin}");
            return;
        }
    };

    let command = v["tool_input"]["command"].as_str().unwrap_or("").to_string();
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
                eprintln!("[ecotokens debug] rewriting: {} → {}", input.command, new_cmd);
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
