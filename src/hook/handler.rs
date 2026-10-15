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

/// Decode the shell-quoted token at the start of `s` and return it with the
/// rest of `s`. Accepted forms:
/// - `'…'` with `'\''` or `'"'"'` escapes (the latter is what
///   [`shell_single_quote`] emits);
/// - `"…"` decoded with bash double-quote rules: `\` escapes only `$`, `` ` ``,
///   `"`, `\` and newline, and stays literal before anything else.
///
/// Returns `None` for anything else, including a double-quoted token with an
/// unescaped `$` or `` ` ``: the outer shell expands those before `bash -c`
/// runs, so moving them to an inner shell would change what runs.
fn parse_quoted(s: &str) -> Option<(String, &str)> {
    let mut it = s.char_indices().peekable();
    let (_, quote) = it.next()?;
    let mut out = String::new();
    match quote {
        '\'' => loop {
            let (i, c) = it.next()?;
            if c != '\'' {
                out.push(c);
                continue;
            }
            let rest = &s[i..];
            if let Some(esc) = ["'\"'\"'", "'\\''"].iter().find(|e| rest.starts_with(**e)) {
                out.push('\'');
                for _ in 1..esc.len() {
                    it.next();
                }
                continue;
            }
            return Some((out, &s[i + 1..]));
        },
        '"' => loop {
            let (i, c) = it.next()?;
            match c {
                '"' => return Some((out, &s[i + 1..])),
                '$' | '`' => return None,
                '\\' => match it.peek()?.1 {
                    n @ ('$' | '`' | '"' | '\\') => {
                        out.push(n);
                        it.next();
                    }
                    '\n' => {
                        it.next();
                    }
                    _ => out.push('\\'),
                },
                other => out.push(other),
            }
        },
        _ => None,
    }
}

/// Parse an option value at the start of `s`: a quoted token or a bare word
/// free of shell metacharacters. Returns the rest of `s`.
fn skip_option_value(s: &str) -> Option<&str> {
    if s.starts_with(['\'', '"']) {
        return parse_quoted(s).map(|(_, rest)| rest);
    }
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    let word = &s[..end];
    let safe = !word.is_empty() && is_simple_command(word) && !word.contains(['\'', '"', '#']);
    safe.then(|| &s[end..])
}

/// Parse one `ecotokens filter [--agent A] [--cwd C] -- bash -c '<inner>'`
/// layer — exactly the shape the hook and the plugins emit — and return
/// `<inner>`. Anything else (other options, a marker inside a comment,
/// commands before the marker, text after the quoted token) is not a wrapper.
fn parse_wrapper(s: &str) -> Option<String> {
    let mut rest = s.trim().strip_prefix("ecotokens filter ")?;
    loop {
        rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix("-- bash -c ") {
            let (inner, after) = parse_quoted(r.trim_start())?;
            return after.trim().is_empty().then_some(inner);
        }
        let r = rest
            .strip_prefix("--agent ")
            .or_else(|| rest.strip_prefix("--cwd "))?;
        rest = skip_option_value(r.trim_start())?;
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

/// Peel `ecotokens filter … -- bash -c '<inner>'` layers (see
/// [`parse_wrapper`]) until a command that is not a strict wrapper is reached,
/// and return it: the original shell command, an `ecotokens` command that is
/// not a wrapper (`ecotokens gain`, `ecotokens filter --debug -- …`), or a
/// wrapper-like command that cannot be peeled without changing what it runs
/// (a trailing `&& …`, a `$(…)` in double quotes), which is then kept whole.
fn peel_ecotokens_wrapper(cmd: &str) -> String {
    let mut s = cmd.trim().to_string();
    while let Some(inner) = parse_wrapper(&s) {
        s = inner;
    }
    s
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
/// (`ecotokens gain; cat secrets`) or a wrapper layer that cannot be peeled
/// safely is still wrapped whole, so its output keeps going through
/// `masking::mask`.
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

    // A layer that cannot be peeled safely is kept whole: it may carry a
    // trailing `&& …` that must still go through masking.
    let cmd = if is_ecotokens_cmd(raw) {
        peel_ecotokens_wrapper(raw)
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

#[cfg(test)]
mod tests {
    use super::{handle_hook_input, parse_wrapper, peel_ecotokens_wrapper, HookInput, HookOutput};
    use serde::Deserialize;

    /// Shared with the TS plugins: tests/plugins/peel.test.mjs consumes the
    /// same file, so the three copies of the grammar must agree on every case.
    const VECTORS: &str = include_str!("../../tests/plugins/peel_vectors.json");

    #[derive(Deserialize)]
    struct Vectors {
        peelable: Vec<Peelable>,
        unpeelable: Vec<Unpeelable>,
    }

    #[derive(Deserialize)]
    struct Peelable {
        cmd: String,
        inner: String,
    }

    #[derive(Deserialize)]
    struct Unpeelable {
        cmd: String,
        /// `bare`: the hook must let the command run as is; `whole`: it must be
        /// wrapped so its output still goes through masking.
        expect: String,
    }

    #[test]
    fn shared_peel_vectors() {
        let v: Vectors = serde_json::from_str(VECTORS).expect("valid vectors JSON");
        assert!(!v.peelable.is_empty() && !v.unpeelable.is_empty());

        for Peelable { cmd, inner } in &v.peelable {
            assert_eq!(
                peel_ecotokens_wrapper(cmd),
                *inner,
                "peel must reach the inner command: {cmd}"
            );
        }

        for Unpeelable { cmd, expect } in &v.unpeelable {
            assert!(
                parse_wrapper(cmd).is_none(),
                "must not parse as a wrapper: {cmd}"
            );
            assert_eq!(
                peel_ecotokens_wrapper(cmd),
                cmd.as_str(),
                "an unpeelable command must stay whole: {cmd}"
            );
            let bare = matches!(
                handle_hook_input(
                    &HookInput {
                        command: cmd.clone(),
                        cwd: None,
                    },
                    &[],
                    false,
                    "claude"
                ),
                HookOutput::Passthrough
            );
            assert_eq!(
                bare,
                expect == "bare",
                "routing mismatch for {cmd}: expected {expect}"
            );
        }
    }
}
