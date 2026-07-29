use ecotokens::hook::post_handler::{
    codex_bash_output_text, handle_post_input, metrics_command, PostFilterResult, PostHookInput,
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

#[test]
fn codex_bash_output_accepts_string_tool_response() {
    let output = serde_json::json!("shell output\n");
    assert_eq!(codex_bash_output_text(&output), "shell output\n");
}

#[test]
fn codex_bash_output_keeps_legacy_object_fields() {
    assert_eq!(
        codex_bash_output_text(&serde_json::json!({"output": "from output"})),
        "from output"
    );
    assert_eq!(
        codex_bash_output_text(&serde_json::json!({"stdout": "from stdout"})),
        "from stdout"
    );
}

// ── secret masking (regression) ───────────────────────────────────────────────
//
// Native Read/Grep results used to reach `handle_read`/`handle_grep` unmasked,
// so a secret in a file or in a grep match flowed straight into the context
// ecotokens injects and into the interception rows it persists. The dispatcher
// now masks every payload before it reaches a handler, mirroring the Bash path
// in `filter::run_filter_pipeline_with_cwd`.

const AWS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";

/// Both halves of a `Filtered` result: what gets injected into the model's
/// context, and what gets persisted as `content_before` in the metrics store.
/// Panics on `Passthrough` so these tests can never pass vacuously.
fn expect_filtered(result: &PostFilterResult) -> (&str, &str) {
    match result {
        PostFilterResult::Filtered {
            output,
            content_before,
            ..
        } => (output.as_str(), content_before.as_str()),
        PostFilterResult::Passthrough => {
            panic!("expected a Filtered result; Passthrough would make this test vacuous")
        }
    }
}

#[test]
fn read_content_is_masked_before_reaching_the_handler() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("creds.rs");

    // One symbol (small outline) over a long body (large content), so the
    // handler actually takes the Filtered branch.
    let mut src = String::from("fn load_config() {\n");
    for i in 0..80 {
        src.push_str(&format!("    let v{i} = {i};\n"));
    }
    src.push_str(&format!("    let key = \"{AWS_KEY}\";\n}}\n"));
    std::fs::write(&file, &src).unwrap();

    let input = make_input(
        "Read",
        serde_json::json!({ "file_path": file.to_str().unwrap() }),
        serde_json::json!({ "file": { "content": src } }),
    );
    let (result, family) = handle_post_input(&input, 0);
    assert_eq!(family, CommandFamily::NativeRead);

    let (output, content_before) = expect_filtered(&result);
    assert!(
        !output.contains(AWS_KEY),
        "injected output leaked the AWS key"
    );
    assert!(
        !content_before.contains(AWS_KEY),
        "persisted content_before leaked the AWS key"
    );
}

#[test]
fn grep_output_is_masked_before_reaching_the_handler() {
    // `filter_grep` keeps at most 10 matches per file, so many matches in few
    // files is what actually compacts. The secret is the sole match in its own
    // file, keeping it inside the compacted output rather than behind a
    // "+N more" elision.
    let mut out = format!("config.py:12:AWS_KEY = \"{AWS_KEY}\"\n");
    for i in 0..50 {
        out.push_str(&format!(
            "src/noise.rs:{i}:let credential_{i} = compute();\n"
        ));
    }

    let input = make_input(
        "Grep",
        serde_json::json!({ "pattern": "AKIA" }),
        serde_json::json!({ "output": out }),
    );
    let (result, family) = handle_post_input(&input, 0);
    assert_eq!(family, CommandFamily::Grep);

    let (output, content_before) = expect_filtered(&result);
    assert!(
        output.contains("config.py"),
        "the secret's match line should survive compaction, masked"
    );
    assert!(
        !output.contains(AWS_KEY),
        "injected output leaked the AWS key"
    );
    assert!(
        !content_before.contains(AWS_KEY),
        "persisted content_before leaked the AWS key"
    );
}

// ── Codex stderr (regression) ─────────────────────────────────────────────────
//
// `codex_bash_output_text` feeds `run_filter_pipeline_with_cwd`, the only place
// masking runs on the Codex path. It used to read `.output`/`.stdout` only, so a
// secret surfacing solely on stderr never entered the pipeline and was never
// redacted.

#[test]
fn codex_bash_output_includes_stderr() {
    assert_eq!(
        codex_bash_output_text(&serde_json::json!({"stderr": "boom: bad token"})),
        "boom: bad token"
    );
}

#[test]
fn codex_bash_output_combines_stdout_and_stderr() {
    let combined = codex_bash_output_text(&serde_json::json!({
        "stdout": "ok\n",
        "stderr": "warning: deprecated",
    }));
    assert!(combined.contains("ok"), "stdout must be kept: {combined}");
    assert!(
        combined.contains("warning: deprecated"),
        "stderr must be kept: {combined}"
    );
    // stdout already ends with a newline — no blank line should be introduced.
    assert_eq!(combined, "ok\nwarning: deprecated");
}

#[test]
fn codex_bash_output_separates_stdout_without_trailing_newline() {
    assert_eq!(
        codex_bash_output_text(&serde_json::json!({"output": "a", "stderr": "b"})),
        "a\nb"
    );
}
