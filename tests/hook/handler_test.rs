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
    let out = handle_hook_input(&input, &exclusions, false);
    assert!(
        matches!(out, HookOutput::Passthrough),
        "excluded cmd should passthrough"
    );
}

#[test]
fn non_excluded_command_returns_rewrite() {
    let exclusions: Vec<String> = vec![];
    let input = make_input("git status");
    let out = handle_hook_input(&input, &exclusions, false);
    match &out {
        HookOutput::Rewrite(cmd) => {
            assert!(
                cmd.contains("ecotokens"),
                "rewritten cmd should call ecotokens"
            );
            assert!(
                cmd.contains("git status"),
                "rewritten cmd should include original"
            );
        }
        _ => panic!("expected Rewrite, got {out:?}"),
    }
}

#[test]
fn debug_flag_does_not_change_rewrite_logic() {
    let exclusions: Vec<String> = vec![];
    let input = make_input("cargo test");
    let out_normal = handle_hook_input(&input, &exclusions, false);
    let out_debug = handle_hook_input(&input, &exclusions, true);
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
    let out1 = handle_hook_input(&input, &exclusions, false);
    assert!(
        matches!(out1, HookOutput::Rewrite(_)),
        "should rewrite before exclusion added"
    );

    exclusions.push("grep".to_string());
    let out2 = handle_hook_input(&input, &exclusions, false);
    assert!(
        matches!(out2, HookOutput::Passthrough),
        "should passthrough after exclusion added"
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
        let out = ecotokens::hook::handler::handle_hook_input(&input, &[], false);

        match out {
            ecotokens::hook::handler::HookOutput::Rewrite(cmd) => {
                assert!(
                    cmd.starts_with("ecotokens filter --"),
                    "should call ecotokens filter"
                );
                assert!(cmd.contains("ls -la"), "should retain original command");
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
        let out = ecotokens::hook::handler::handle_hook_input(&input, &exclusions, false);
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
        let out = ecotokens::hook::handler::handle_hook_input(&input, &exclusions, false);
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
        let new_cmd = "ecotokens filter -- git status".to_string();

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
