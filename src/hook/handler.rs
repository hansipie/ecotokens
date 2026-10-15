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

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Decode a single shell-quoted token: `'…'` with `'\''` or `'"'"'` escapes
/// (the latter is what [`shell_single_quote`] emits), or `"…"` with backslash
/// escapes (JSON-style: `\"`, `\\`, `\n`, `\uXXXX`, …).
/// Returns `None` when `s` is not exactly one such quoted token.
fn unquote_token(s: &str) -> Option<String> {
    let s = s.trim();
    let chars: Vec<char> = s.chars().collect();
    let quote = *chars.first()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let mut out = String::new();
    let mut i = 1usize;
    loop {
        let c = *chars.get(i)?;
        if quote == '\'' {
            if c == '\'' {
                if chars.get(i + 1) == Some(&'"')
                    && chars.get(i + 2) == Some(&'\'')
                    && chars.get(i + 3) == Some(&'"')
                    && chars.get(i + 4) == Some(&'\'')
                {
                    out.push('\'');
                    i += 5;
                    continue;
                }
                if chars.get(i + 1) == Some(&'\\')
                    && chars.get(i + 2) == Some(&'\'')
                    && chars.get(i + 3) == Some(&'\'')
                {
                    out.push('\'');
                    i += 4;
                    continue;
                }
                i += 1;
                break;
            }
            out.push(c);
            i += 1;
        } else {
            match c {
                '\\' => {
                    let n = *chars.get(i + 1)?;
                    match n {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        'u' => {
                            let hex: String = chars.iter().skip(i + 2).take(4).collect();
                            out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                            i += 6;
                            continue;
                        }
                        other => out.push(other),
                    }
                    i += 2;
                }
                '"' => {
                    i += 1;
                    break;
                }
                other => {
                    out.push(other);
                    i += 1;
                }
            }
        }
    }

    if chars[i..].iter().all(|c| c.is_whitespace()) {
        Some(out)
    } else {
        None
    }
}

/// True when `s` is an `ecotokens` invocation (word boundary: `ecotokens-foo`
/// and `ecotokensx` are ordinary commands).
fn is_ecotokens_cmd(s: &str) -> bool {
    s == "ecotokens"
        || s.strip_prefix("ecotokens")
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

/// True when `s` has no shell operator, substitution or newline, i.e. it is a
/// single command that cannot chain anything else after (or inside) it.
fn is_simple_command(s: &str) -> bool {
    !s.chars().any(|c| {
        matches!(
            c,
            ';' | '&' | '|' | '<' | '>' | '`' | '$' | '(' | ')' | '\n' | '\\'
        )
    })
}

/// Peel `ecotokens filter … -- bash -c '<inner>'` layers until a command that
/// is not a peelable wrapper is reached.
///
/// Returns `Some(inner)`: the original shell command, or an `ecotokens`
/// command that is not a wrapper (`ecotokens gain`, a direct
/// `ecotokens filter …` without `bash -c`). Returns `None` for a corrupt chain
/// (a wrapper whose `bash -c` argument is not exactly one quoted token, e.g. a
/// trailing `&& …`), which callers must not trust.
fn peel_ecotokens_wrapper(cmd: &str) -> Option<String> {
    const MARKER: &str = " -- bash -c ";
    let mut s = cmd.trim().to_string();
    while s.starts_with("ecotokens filter ") {
        let Some(idx) = s.find(MARKER) else { break };
        s = unquote_token(&s[idx + MARKER.len()..])?;
    }
    Some(s)
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
///
/// Re-entrancy: a command that is already an `ecotokens filter … -- bash -c …`
/// wrapper (written by an earlier rewrite *or* by the model itself — models
/// copy their already-wrapped tool calls out of the transcript and wrap them
/// again) is peeled back to the original shell command and re-wrapped exactly
/// once. This heals nested chains instead of growing them.
///
/// Security note: an excluded command is returned as `Passthrough`, so it is
/// never rewritten to run under `ecotokens filter` — the only place
/// `masking::mask` is applied on the Bash path. Its output therefore reaches the
/// model entirely unredacted, and ecotokens cannot do better here: it never sees
/// that output at all. Excluding a command is a deliberate opt-out of both
/// filtering *and* secret masking. Note also that matching is by prefix, so
/// `git` excludes every command starting with those characters.
///
/// A *simple* `ecotokens` CLI call (`ecotokens gain`, a direct
/// `ecotokens filter …`: no operator, substitution or newline) is not wrapped
/// into another `ecotokens filter`. Anything chained to it
/// (`ecotokens gain; cat secrets`) or a corrupt wrapper chain is still wrapped
/// whole, so its output keeps going through `masking::mask`.
pub fn handle_hook_input(
    input: &HookInput,
    exclusions: &[String],
    _debug: bool,
    agent: &str,
) -> HookOutput {
    let raw = input.command.trim();

    // Check exclusion list (prefix match) — see the security note above: this
    // bypasses masking, not just filtering.
    let excluded = |s: &str| {
        exclusions
            .iter()
            .any(|e| s.starts_with(e.as_str()) || s == e.as_str())
    };
    if excluded(raw) {
        return HookOutput::Passthrough;
    }

    // A corrupt wrapper chain is wrapped as is rather than trusted: it may carry
    // a trailing `&& …` that must still go through masking.
    let cmd = if is_ecotokens_cmd(raw) {
        peel_ecotokens_wrapper(raw).unwrap_or_else(|| raw.to_string())
    } else {
        raw.to_string()
    };

    // An excluded command stays excluded even when the model resubmits it
    // inside an `ecotokens filter` wrapper.
    if cmd != raw && excluded(&cmd) {
        return HookOutput::Passthrough;
    }

    // Already an `ecotokens` CLI call: no second filter layer. Unwrap any
    // leftover wrapper chain around it so the call runs bare.
    if is_ecotokens_cmd(&cmd) && is_simple_command(&cmd) {
        return if cmd == raw {
            HookOutput::Passthrough
        } else {
            HookOutput::Rewrite(cmd)
        };
    }

    // Rewrite to ecotokens filter
    let rewritten = match &input.cwd {
        Some(cwd) => format!(
            "ecotokens filter --agent {} --cwd {} -- bash -c {}",
            agent,
            shell_single_quote(cwd),
            shell_single_quote(&cmd)
        ),
        None => format!(
            "ecotokens filter --agent {} -- bash -c {}",
            agent,
            shell_single_quote(&cmd)
        ),
    };
    HookOutput::Rewrite(rewritten)
}

/// Inner handler shared by Claude Code and Codex PreToolUse hooks (same JSON format).
fn handle_with_agent(agent: &str) {
    use super::MAX_STDIN_BYTES;
    use std::io::Read;

    let mut stdin = String::new();
    std::io::stdin()
        .take(MAX_STDIN_BYTES as u64 + 1)
        .read_to_string(&mut stdin)
        .unwrap_or_default();

    if stdin.len() > MAX_STDIN_BYTES {
        // `println!` (not `print!`) so the framing matches the normal success
        // path and newline-delimited protocols don't mis-parse the output.
        println!("{stdin}");
        return;
    }

    let v: serde_json::Value = match serde_json::from_str(&stdin) {
        Ok(v) => v,
        Err(_) => {
            println!("{stdin}");
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
    let output = handle_hook_input(&input, &settings.exclusions, debug, agent);

    let response = match output {
        HookOutput::Passthrough => serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow"
            }
        }),
        HookOutput::Rewrite(new_cmd) => {
            if debug {
                // Both sides are masked: masking otherwise only ever covers a
                // command's *output*, never the command line itself, so a secret
                // passed as a literal argument (`curl -H "Authorization: Bearer …"`)
                // would leak here. `new_cmd` embeds the original shell-quoted, so
                // it carries the same secret and needs the same treatment.
                let (cmd, _) = crate::masking::mask(&input.command);
                let (rewritten, _) = crate::masking::mask(&new_cmd);
                eprintln!("[ecotokens debug] rewriting ({agent}): {cmd} → {rewritten}");
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

/// Top-level hook stdin→stdout handler (reads Claude Code PreToolUse JSON).
pub fn handle() {
    handle_with_agent("claude");
}

/// Top-level hook stdin→stdout handler for Codex PreToolUse events.
pub fn handle_codex() {
    handle_with_agent("codex");
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
    let output = handle_hook_input(&input, &settings.exclusions, debug, label);

    match output {
        HookOutput::Passthrough => emit_allow(hook_event_name, None),
        HookOutput::Rewrite(new_cmd) => {
            if debug {
                // See `handle_with_agent`: the command line itself is never
                // covered by the output-masking pipeline, so mask both sides.
                let (cmd, _) = crate::masking::mask(&input.command);
                let (rewritten, _) = crate::masking::mask(&new_cmd);
                eprintln!("[ecotokens debug] rewriting ({label}): {cmd} → {rewritten}");
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
