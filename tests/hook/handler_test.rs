use ecotokens::hook::handler::{handle_hook_input, HookInput, HookOutput};

fn make_input(command: &str) -> HookInput {
    HookInput {
        command: command.to_string(),
        cwd: None,
    }
}

#[test]
fn parses_valid_json_stdin() {
    let json = r#"{"tool_input": {"command": "git status"}}"#;
    let input: HookInput = serde_json::from_str(
        &serde_json::from_str::<serde_json::Value>(json).unwrap()["tool_input"].to_string(),
    )
    .unwrap();
    assert_eq!(input.command, "git status");
}

#[test]
fn excluded_command_returns_passthrough() {
    let exclusions = vec!["grep".to_string(), "ls".to_string()];
    let input = make_input("ls -la");
    let out = handle_hook_input(&input, &exclusions, false, "claude");
    assert!(
        matches!(out, HookOutput::Passthrough),
        "excluded cmd should passthrough"
    );
}

#[test]
fn non_excluded_command_returns_rewrite() {
    let exclusions: Vec<String> = vec![];
    let input = make_input("git status");
    let out = handle_hook_input(&input, &exclusions, false, "claude");
    match &out {
        HookOutput::Rewrite(cmd) => {
            assert!(
                cmd.contains("ecotokens"),
                "rewritten cmd should call ecotokens"
            );
            assert!(
                cmd.contains("bash -c 'git status'"),
                "rewritten cmd should shell-wrap original command"
            );
        }
        _ => panic!("expected Rewrite, got {out:?}"),
    }
}

#[test]
fn command_with_shell_operators_is_wrapped_in_bash_c() {
    let exclusions: Vec<String> = vec![];
    let input = make_input("cd /tmp && echo hello");
    let out = handle_hook_input(&input, &exclusions, false, "claude");

    match out {
        HookOutput::Rewrite(cmd) => {
            assert_eq!(
                cmd,
                "ecotokens filter --agent claude -- bash -c 'cd /tmp && echo hello'"
            );
        }
        _ => panic!("expected Rewrite"),
    }
}

#[test]
fn command_with_heredoc_is_wrapped_in_bash_c() {
    // Regression test for https://github.com/hansipie/ecotokens/issues/54:
    // a heredoc command like `python3 << 'EOF'\n...\nEOF` must not have its
    // stdin consumed by ecotokens filter. bash -c handles the heredoc
    // internally so the interpreter receives the correct stdin.
    let exclusions: Vec<String> = vec![];
    let cmd = "uv run python3 << 'PYEOF'\nprint('hello')\nPYEOF";
    let input = make_input(cmd);
    let out = handle_hook_input(&input, &exclusions, false, "claude");

    match out {
        HookOutput::Rewrite(rewritten) => {
            assert!(
                rewritten.starts_with("ecotokens filter --agent claude -- bash -c '"),
                "heredoc command must be wrapped in bash -c"
            );
            assert!(
                rewritten.contains("uv run python3"),
                "original command must be preserved in the rewrite"
            );
        }
        _ => panic!("expected Rewrite"),
    }
}

#[test]
fn command_with_output_redirect_is_wrapped_in_bash_c() {
    // Regression test for https://github.com/hansipie/ecotokens/issues/52:
    // a command like `cmd > file` must not have its ecotokens annotations
    // injected into the redirect target. bash -c handles the redirect
    // internally so ecotokens stdout never reaches the file.
    let exclusions: Vec<String> = vec![];
    let input = make_input("notebooklm source list --json > /tmp/output.json");
    let out = handle_hook_input(&input, &exclusions, false, "claude");

    match out {
        HookOutput::Rewrite(cmd) => {
            assert_eq!(
                cmd,
                "ecotokens filter --agent claude -- bash -c 'notebooklm source list --json > /tmp/output.json'"
            );
        }
        _ => panic!("expected Rewrite"),
    }
}

#[test]
fn cwd_and_command_are_shell_quoted_in_rewrite() {
    let exclusions: Vec<String> = vec![];
    let input = HookInput {
        command: "printf 'hello'".to_string(),
        cwd: Some("/tmp/dir with spaces".to_string()),
    };
    let out = handle_hook_input(&input, &exclusions, false, "claude");

    match out {
        HookOutput::Rewrite(cmd) => {
            assert_eq!(
                cmd,
                r#"ecotokens filter --agent claude --cwd '/tmp/dir with spaces' -- bash -c 'printf '"'"'hello'"'"''"#
            );
        }
        _ => panic!("expected Rewrite"),
    }
}

#[test]
fn debug_flag_does_not_change_rewrite_logic() {
    let exclusions: Vec<String> = vec![];
    let input = make_input("cargo test");
    let out_normal = handle_hook_input(&input, &exclusions, false, "claude");
    let out_debug = handle_hook_input(&input, &exclusions, true, "claude");
    match (&out_normal, &out_debug) {
        (HookOutput::Rewrite(a), HookOutput::Rewrite(b)) => assert_eq!(a, b),
        _ => panic!("both should be Rewrite"),
    }
}

#[test]
fn hot_reload_exclusion_respected() {
    // Simulate reading fresh exclusion list between calls
    let mut exclusions: Vec<String> = vec![];
    let input = make_input("grep foo bar");
    let out1 = handle_hook_input(&input, &exclusions, false, "claude");
    assert!(
        matches!(out1, HookOutput::Rewrite(_)),
        "should rewrite before exclusion added"
    );

    exclusions.push("grep".to_string());
    let out2 = handle_hook_input(&input, &exclusions, false, "claude");
    assert!(
        matches!(out2, HookOutput::Passthrough),
        "should passthrough after exclusion added"
    );
}

// ── Re-entrancy: peeling of already-wrapped commands ──────────────────────────

/// Simulate the model/plugin double-quote wrap style (`JSON.stringify`-like):
/// quote the payload for a `bash -c "…"` layer, escaping backslashes and
/// double quotes.
fn nest_double_quoted(cmd: &str, levels: usize) -> String {
    let mut s = cmd.to_string();
    for _ in 0..levels {
        s = format!(
            "ecotokens filter --agent opencode --cwd \"/tmp/x\" -- bash -c \"{}\"",
            s.replace('\\', "\\\\").replace('"', "\\\"")
        );
    }
    s
}

#[test]
fn single_quote_wrapper_is_normalized_to_one_wrap() {
    let exclusions: Vec<String> = vec![];
    let input = make_input("ecotokens filter --agent claude -- bash -c 'git status'");
    let out = handle_hook_input(&input, &exclusions, false, "claude");
    match out {
        HookOutput::Rewrite(cmd) => assert_eq!(
            cmd,
            "ecotokens filter --agent claude -- bash -c 'git status'"
        ),
        _ => panic!("expected Rewrite"),
    }
}

#[test]
fn deeply_nested_wrapper_is_healed_to_a_single_wrap() {
    // Regression: models copy their already-wrapped tool calls from the
    // transcript and wrap them again, growing exponentially-escaped chains
    // (observed up to 9 levels in the wild, see #124).
    let exclusions: Vec<String> = vec![];
    let inner = "ls -la app/src/main.kt && head -15 app/src/main.kt";
    let nested = nest_double_quoted(inner, 9);
    assert!(
        nested.matches("ecotokens filter").count() == 9,
        "fixture must contain 9 wrapper levels"
    );

    let input = make_input(&nested);
    let out = handle_hook_input(&input, &exclusions, false, "claude");
    match out {
        HookOutput::Rewrite(cmd) => {
            assert_eq!(
                cmd,
                format!("ecotokens filter --agent claude -- bash -c '{inner}'")
            );
            assert_eq!(
                cmd.matches("ecotokens filter").count(),
                1,
                "healed command must contain exactly one wrapper"
            );
        }
        _ => panic!("expected Rewrite"),
    }
}

#[test]
fn mixed_quote_styles_are_peeled() {
    // Single-quote layer (shQuote) wrapping a double-quote layer (old plugin
    // JSON.stringify) wrapping the real command.
    let exclusions: Vec<String> = vec![];
    let double = nest_double_quoted("echo hello", 1);
    let mixed = format!("ecotokens filter --agent opencode -- bash -c '{double}'");
    let input = make_input(&mixed);
    let out = handle_hook_input(&input, &exclusions, false, "claude");
    match out {
        HookOutput::Rewrite(cmd) => {
            assert_eq!(
                cmd,
                "ecotokens filter --agent claude -- bash -c 'echo hello'"
            )
        }
        _ => panic!("expected Rewrite"),
    }
}

#[test]
fn opaque_ecotokens_commands_pass_through() {
    let exclusions: Vec<String> = vec![];
    // Non-wrapper ecotokens subcommand: must not be wrapped into another filter.
    for cmd in [
        "ecotokens gain",
        "ecotokens gain --json",
        "ecotokens filter --debug -- cargo test",
        "ecotokens search 'foo'",
    ] {
        let input = make_input(cmd);
        let out = handle_hook_input(&input, &exclusions, false, "claude");
        assert!(
            matches!(out, HookOutput::Passthrough),
            "opaque ecotokens command should pass through: {cmd}"
        );
    }
}

#[test]
fn excluded_inner_command_stays_excluded_when_resubmitted_wrapped() {
    let exclusions = vec!["git".to_string()];
    let input = make_input("ecotokens filter --agent claude -- bash -c 'git status'");
    let out = handle_hook_input(&input, &exclusions, false, "claude");
    assert!(
        matches!(out, HookOutput::Passthrough),
        "exclusion must apply to the peeled inner command"
    );
}

#[test]
fn wrapper_emitted_by_the_hook_with_apostrophes_is_peeled() {
    // The hook quotes with `'"'"'`; resubmitting its own output must heal, not
    // pass through.
    let exclusions: Vec<String> = vec![];
    let first = match handle_hook_input(
        &make_input("echo 'hi' && echo \"it's\""),
        &exclusions,
        false,
        "claude",
    ) {
        HookOutput::Rewrite(cmd) => cmd,
        _ => panic!("expected Rewrite"),
    };
    assert!(
        first.contains("'\"'\"'"),
        "fixture must use the '\"'\"' escape: {first}"
    );

    let nested = format!(
        "ecotokens filter --agent claude -- bash -c '{}'",
        first.replace('\'', "'\\''")
    );
    for cmd in [first.as_str(), nested.as_str()] {
        match handle_hook_input(&make_input(cmd), &exclusions, false, "claude") {
            HookOutput::Rewrite(out) => assert_eq!(out, first),
            _ => panic!("expected Rewrite for {cmd}"),
        }
    }
}

#[test]
fn ecotokens_prefix_requires_a_word_boundary() {
    let exclusions: Vec<String> = vec![];
    for cmd in ["ecotokens-build.sh", "ecotokensx status"] {
        let out = handle_hook_input(&make_input(cmd), &exclusions, false, "claude");
        assert!(
            matches!(out, HookOutput::Rewrite(_)),
            "{cmd} is not an ecotokens CLI call and must be wrapped"
        );
    }
}

#[test]
fn chained_ecotokens_command_is_still_wrapped() {
    let exclusions: Vec<String> = vec![];
    for cmd in [
        "ecotokens gain; cat ~/.ssh/id_rsa",
        "ecotokens gain && env",
        "ecotokens gain | tee out",
        "ecotokens search $(cat secret)",
    ] {
        match handle_hook_input(&make_input(cmd), &exclusions, false, "claude") {
            HookOutput::Rewrite(out) => assert!(
                out.starts_with("ecotokens filter --agent claude -- bash -c "),
                "{cmd} must be wrapped whole: {out}"
            ),
            _ => panic!("chained ecotokens command must not pass through: {cmd}"),
        }
    }
}

#[test]
fn corrupt_wrapper_chain_with_trailing_command_is_wrapped_whole() {
    let exclusions: Vec<String> = vec![];
    let cmd = "ecotokens filter --agent claude -- bash -c 'ls' && cat secrets";
    match handle_hook_input(&make_input(cmd), &exclusions, false, "claude") {
        HookOutput::Rewrite(out) => {
            assert!(out.contains("cat secrets"));
            assert!(out.starts_with("ecotokens filter --agent claude -- bash -c '"));
        }
        _ => panic!("corrupt chain must be wrapped, not passed through"),
    }
}

#[test]
fn wrapper_around_simple_ecotokens_call_is_unwrapped() {
    let exclusions: Vec<String> = vec![];
    let nested = nest_double_quoted("ecotokens gain", 3);
    match handle_hook_input(&make_input(&nested), &exclusions, false, "claude") {
        HookOutput::Rewrite(out) => assert_eq!(out, "ecotokens gain"),
        _ => panic!("expected Rewrite to the bare ecotokens call"),
    }
}

/// The command the shell will actually run after the hook: the rewrite, or the
/// original command on passthrough.
fn effective_command(cmd: &str) -> String {
    match handle_hook_input(&make_input(cmd), &[], false, "claude") {
        HookOutput::Rewrite(out) => out,
        HookOutput::Passthrough => cmd.to_string(),
    }
}

#[test]
fn wrapper_marker_inside_a_comment_is_not_peeled() {
    // Regression: the ` -- bash -c ` marker was searched anywhere in the
    // command, so commented-out code became the command to run.
    let cmd = "ecotokens filter -- echo OK # -- bash -c 'echo SHOULD_NOT_RUN'";
    let out = effective_command(cmd);
    assert!(
        out.contains("echo OK #"),
        "the commented-out part must stay a comment: {out}"
    );
}

#[test]
fn commands_before_a_wrapper_marker_are_not_dropped() {
    let cmd = "ecotokens filter -- echo FIRST && echo SECOND -- bash -c 'echo ONLY_THIS'";
    let out = effective_command(cmd);
    assert!(
        out.contains("echo FIRST && echo SECOND"),
        "commands before the marker must still run: {out}"
    );
}

#[test]
fn double_quoted_backslash_is_kept_literal() {
    // Inside bash double quotes `\b` is two characters, not an escape.
    let cmd = r#"ecotokens filter --agent claude -- bash -c "printf '%s' 'a\b'""#;
    let out = effective_command(cmd);
    assert!(out.contains(r"a\b"), "backslash must be preserved: {out}");
}

#[test]
fn double_quoted_backslash_n_is_not_turned_into_a_newline() {
    let cmd = r#"ecotokens filter --agent claude -- bash -c "printf '%s' 'x\ny'""#;
    let out = effective_command(cmd);
    assert!(
        out.contains(r"x\ny") && !out.contains('\n'),
        "`\\n` must stay two characters: {out}"
    );
}

#[test]
fn double_quoted_substitution_is_not_moved_to_the_inner_shell() {
    // The outer shell expands `$(…)` before `bash -c` runs; peeling would make
    // the inner shell expand it instead (`echo a;b` vs `echo 'a;b'`).
    let cmd = r#"ecotokens filter --agent claude -- bash -c "echo $(printf 'a;b')""#;
    let out = effective_command(cmd);
    assert!(
        out.contains(r#""echo $(printf"#),
        "an ambiguous double-quoted wrapper must be kept whole: {out}"
    );
}

#[test]
fn ts_plugins_peel_wrappers_safely() {
    // Runs tests/plugins/peel.test.mjs against the OpenCode plugin and the Pi
    // extension. Needs Node ≥ 22.6 (type stripping); skipped when absent.
    let version = match std::process::Command::new("node").arg("--version").output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => {
            eprintln!("node not found, skipping TS plugin tests");
            return;
        }
    };
    let mut parts = version
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse::<u32>().unwrap_or(0));
    let (major, minor) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    if (major, minor) < (22, 6) {
        eprintln!("node {version} has no type stripping, skipping TS plugin tests");
        return;
    }
    let out = std::process::Command::new("node")
        .args([
            "--experimental-strip-types",
            "--no-warnings",
            "--test",
            "tests/plugins/peel.test.mjs",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run node --test");
    assert!(
        out.status.success(),
        "TS plugin tests failed:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

// ── Gemini BeforeTool hook handler tests ──────────────────────────────────────

mod gemini_handler {
    /// Parse a Gemini BeforeTool payload and assert the hook rewrites
    /// run_shell_command correctly.
    #[test]
    fn shell_command_is_rewritten() {
        let payload = serde_json::json!({
            "tool_name": "run_shell_command",
            "tool_input": { "command": "ls -la" }
        });

        let tool_name = payload["tool_name"].as_str().unwrap_or("");
        assert_eq!(tool_name, "run_shell_command");

        let command = payload["tool_input"]["command"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let input = ecotokens::hook::handler::HookInput { command, cwd: None };
        let out = ecotokens::hook::handler::handle_hook_input(&input, &[], false, "gemini");

        match out {
            ecotokens::hook::handler::HookOutput::Rewrite(cmd) => {
                assert!(
                    cmd.starts_with("ecotokens filter --agent gemini -- bash -c "),
                    "should call ecotokens filter with gemini agent"
                );
                assert!(cmd.contains("'ls -la'"), "should retain original command");
            }
            _ => panic!("expected Rewrite for shell command"),
        }
    }

    /// Non-shell Gemini tools must result in passthrough (excluded via tool_name check).
    #[test]
    fn non_shell_tool_is_passthrough() {
        // The Gemini handler only intercepts run_shell_command; all others pass through.
        // We verify the classification logic directly via handle_hook_input:
        // An empty command (no command field) should be treated as passthrough.
        let input = ecotokens::hook::handler::HookInput {
            command: String::new(),
            cwd: None,
        };
        // An empty command is an edge case — handle_hook_input trims and rewrites non-excluded
        // commands including empty ones. The actual guard for non-shell tools is in handle_gemini().
        // Here we verify that an exclusion prevents rewrite.
        let exclusions = vec!["".to_string()];
        let out = ecotokens::hook::handler::handle_hook_input(&input, &exclusions, false, "gemini");
        assert!(
            matches!(out, ecotokens::hook::handler::HookOutput::Passthrough),
            "empty command matching exclusion should passthrough"
        );
    }

    /// Excluded shell commands must not be rewritten even on Gemini.
    #[test]
    fn excluded_shell_command_passes_through() {
        let input = ecotokens::hook::handler::HookInput {
            command: "echo hello".to_string(),
            cwd: None,
        };
        let exclusions = vec!["echo".to_string()];
        let out = ecotokens::hook::handler::handle_hook_input(&input, &exclusions, false, "gemini");
        assert!(
            matches!(out, ecotokens::hook::handler::HookOutput::Passthrough),
            "excluded command should passthrough on Gemini too"
        );
    }

    /// Verify the Gemini response JSON shape for a rewrite preserves other tool_input fields.
    #[test]
    fn rewrite_result_merges_tool_input_fields() {
        let original_tool_input = serde_json::json!({
            "command": "git status",
            "timeout": 30
        });
        let new_cmd = "ecotokens filter --agent gemini -- bash -c 'git status'".to_string();

        // Simulate what handle_gemini does when rewriting
        let mut tool_input = original_tool_input.clone();
        tool_input["command"] = serde_json::Value::String(new_cmd.clone());

        let response = serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "BeforeTool",
                "decision": "allow",
                "tool_input": tool_input
            }
        });

        assert_eq!(
            response["hookSpecificOutput"]["tool_input"]["command"]
                .as_str()
                .unwrap(),
            new_cmd
        );
        assert_eq!(
            response["hookSpecificOutput"]["tool_input"]["timeout"]
                .as_i64()
                .unwrap(),
            30,
            "other tool_input fields must be preserved"
        );
        assert_eq!(
            response["hookSpecificOutput"]["hookEventName"]
                .as_str()
                .unwrap(),
            "BeforeTool"
        );
        assert_eq!(
            response["hookSpecificOutput"]["decision"].as_str().unwrap(),
            "allow"
        );
    }
}
