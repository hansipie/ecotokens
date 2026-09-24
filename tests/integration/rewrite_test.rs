#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::tempdir;

/// Run `ecotokens rewrite <extra_args>` with `text` piped on stdin, isolated
/// from the real config home and pointed at a local port nothing listens on
/// (port 1 requires privileges no test process has) — deterministic
/// fail-open regardless of whether a real Ollama happens to be running on
/// this machine's default port. Every case here exercises the fail-open path
/// without a stub (the CLI links the production `OllamaProvider`;
/// `StubProvider` is exercised directly by the unit tests in `tests/rewrite/`).
fn run_rewrite(text: &str, extra_args: &[&str]) -> std::process::Output {
    let tmp = tempdir().expect("tempdir");
    let config_dir = tmp.path().join("ecotokens");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    std::fs::write(
        config_dir.join("config.json"),
        r#"{"rewrite_url": "http://127.0.0.1:1"}"#,
    )
    .expect("write config.json");

    let mut child = Command::new(ecotokens_bin())
        .arg("rewrite")
        .args(extra_args)
        .env("XDG_CONFIG_HOME", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ecotokens rewrite");
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(text.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("rewrite should exit")
}

#[test]
fn stdout_carries_only_transformed_text() {
    let out = run_rewrite(
        "Please rewrite this short paragraph of prose.",
        &["--mode", "paraphrase"],
    );
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Fail-open (no model at localhost:11434 in CI): stdout is the original
    // text, byte-identical modulo a trailing newline the CLI guarantees.
    assert_eq!(
        stdout.trim_end(),
        "Please rewrite this short paragraph of prose."
    );
    // No diagnostics leaked onto stdout.
    assert!(!stdout.contains("ecotokens:"));
}

#[test]
fn diagnostics_go_to_stderr_only() {
    let out = run_rewrite("Some prose to transform.", &["--mode", "paraphrase"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Fail-open must explain itself on stderr.
    assert!(!stderr.is_empty(), "expected a fallback reason on stderr");
}

#[test]
fn exit_code_is_zero_on_fallback() {
    let out = run_rewrite("Prose input.", &["--mode", "paraphrase"]);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn exit_code_is_one_on_invalid_mode() {
    let out = run_rewrite("Prose input.", &["--mode", "bogus-mode"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn exit_code_is_one_on_missing_required_target() {
    let out = run_rewrite("Prose input.", &["--mode", "tone"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn exit_code_is_one_on_forbidden_target() {
    let out = run_rewrite("Prose input.", &["--mode", "paraphrase", "--to", "plain"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn json_flag_produces_valid_schema() {
    let out = run_rewrite(
        "Prose input for JSON output.",
        &["--mode", "paraphrase", "--json"],
    );
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(&stdout).expect("valid JSON on stdout");

    for field in [
        "status",
        "reason",
        "mode",
        "target",
        "model",
        "text",
        "tokens_in",
        "tokens_out",
        "chunk_count",
        "duration_ms",
        "diff_path",
    ] {
        assert!(v.get(field).is_some(), "missing field: {field}");
    }
    assert_eq!(v["mode"], "paraphrase");
    assert_eq!(v["chunk_count"], 1);
}

#[test]
fn json_fallback_text_is_byte_identical_to_input() {
    let input = "Exact text that must survive fallback untouched.";
    let out = run_rewrite(input, &["--mode", "paraphrase", "--json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(v["status"], "fallback");
    assert_eq!(v["text"], input);
}
