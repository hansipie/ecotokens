use ecotokens::hook::post_handler::{
    handle_post_input, metrics_command, PostFilterResult, PostHookInput,
};
use ecotokens::metrics::store::CommandFamily;

fn make_input(
    tool_name: &str,
    tool_input: serde_json::Value,
    tool_response: serde_json::Value,
) -> PostHookInput {
    PostHookInput {
        tool_name: tool_name.to_string(),
        tool_input,
        tool_response,
        cwd: None,
    }
}

#[test]
fn post_handler_unknown_tool_passthrough() {
    let input = make_input(
        "Bash",
        serde_json::json!({"command": "echo hello"}),
        serde_json::json!({"output": "hello"}),
    );
    let (result, _family) = handle_post_input(&input, 1);
    assert!(
        matches!(result, PostFilterResult::Passthrough),
        "unknown tool should passthrough"
    );
}

#[test]
fn post_handler_glob_routes_glob_handler() {
    let input = make_input(
        "Glob",
        serde_json::json!({"pattern": "**/*.rs"}),
        serde_json::json!({"filenames": ["src/main.rs", "src/lib.rs"]}),
    );
    let (_result, family) = handle_post_input(&input, 1);
    assert_eq!(family, CommandFamily::Fs);
}

#[test]
fn post_handler_grep_routes_grep_handler() {
    let input = make_input(
        "Grep",
        serde_json::json!({"pattern": "fn main", "path": "."}),
        serde_json::json!({"output": ""}),
    );
    let (_result, family) = handle_post_input(&input, 1);
    assert_eq!(family, CommandFamily::Grep);
}

#[test]
fn post_handler_read_routes_read_handler() {
    let input = make_input(
        "Read",
        serde_json::json!({"file_path": "/tmp/nonexistent_ecotokens_test.rs"}),
        serde_json::json!({"type": "text", "file": {"filePath": "/tmp/nonexistent.rs", "content": "fn main() {}", "numLines": 1, "startLine": 1, "totalLines": 1}}),
    );
    let (result, family) = handle_post_input(&input, 1);
    // Non-indexed file → Passthrough (and family = NativeRead)
    assert!(
        matches!(result, PostFilterResult::Passthrough),
        "non-indexed file should passthrough"
    );
    assert_eq!(family, CommandFamily::NativeRead);
}

#[test]
fn post_handler_malformed_tool_response_passthrough() {
    // tool_response with missing file key → should not panic, return Passthrough
    let input = make_input(
        "Read",
        serde_json::json!({"file_path": "src/main.rs"}),
        serde_json::json!(null),
    );
    let (result, _family) = handle_post_input(&input, 1);
    assert!(
        matches!(result, PostFilterResult::Passthrough),
        "malformed tool_response should passthrough"
    );
}

#[test]
fn post_handler_metrics_command_includes_read_path() {
    let input = make_input(
        "Read",
        serde_json::json!({"file_path": "src/main.rs"}),
        serde_json::json!({"file": {"content": "fn main() {}"}}),
    );

    assert_eq!(metrics_command(&input), "Read src/main.rs");
}

// Pi format: tool_input uses "path" and tool_response uses "output"
#[test]
fn post_handler_read_pi_format_routes_read_handler() {
    let input = make_input(
        "Read",
        serde_json::json!({"path": "/tmp/nonexistent_ecotokens_pi_test.rs"}),
        serde_json::json!({"output": "fn main() {}"}),
    );
    let (result, family) = handle_post_input(&input, 1);
    assert!(
        matches!(result, PostFilterResult::Passthrough),
        "non-indexed Pi read should passthrough (not panic on empty content)"
    );
    assert_eq!(family, CommandFamily::NativeRead);
}

#[test]
fn post_handler_metrics_command_pi_path_field() {
    let input = make_input(
        "Read",
        serde_json::json!({"path": "src/main.rs"}),
        serde_json::json!({"output": "fn main() {}"}),
    );
    assert_eq!(metrics_command(&input), "Read src/main.rs");
}

// Pi format for find/Glob: tool_response uses "output" with newline-separated paths
#[test]
fn post_handler_glob_pi_format_routes_glob_handler() {
    let input = make_input(
        "Glob",
        serde_json::json!({"pattern": "**/*.rs", "path": "."}),
        serde_json::json!({"output": "src/main.rs\nsrc/lib.rs\n"}),
    );
    let (_result, family) = handle_post_input(&input, 1);
    assert_eq!(family, CommandFamily::Fs);
}
