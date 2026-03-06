use ecotokens::hook::handler::{handle_hook_input, HookInput, HookOutput};

fn make_input(command: &str) -> HookInput {
    HookInput { command: command.to_string() }
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
    assert!(matches!(out, HookOutput::Passthrough), "excluded cmd should passthrough");
}

#[test]
fn non_excluded_command_returns_rewrite() {
    let exclusions: Vec<String> = vec![];
    let input = make_input("git status");
    let out = handle_hook_input(&input, &exclusions, false);
    match &out {
        HookOutput::Rewrite(cmd) => {
            assert!(cmd.contains("ecotokens"), "rewritten cmd should call ecotokens");
            assert!(cmd.contains("git status"), "rewritten cmd should include original");
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
    assert!(matches!(out1, HookOutput::Rewrite(_)), "should rewrite before exclusion added");

    exclusions.push("grep".to_string());
    let out2 = handle_hook_input(&input, &exclusions, false);
    assert!(matches!(out2, HookOutput::Passthrough), "should passthrough after exclusion added");
}
