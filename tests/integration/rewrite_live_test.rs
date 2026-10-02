//! Live-model tests (T092/T094): exercise the real `OllamaProvider` against
//! an actual local Ollama instance, on both the CLI and MCP entry points.
//! `#[ignore]`d by default — these need a real local model and are not part
//! of the deterministic default suite (research.md §1). Run explicitly with:
//!
//! ```bash
//! ollama serve
//! ollama pull llama3.2:3b   # or set ECOTOKENS_TEST_MODEL to a pulled tag
//! cargo test --test rewrite_live_test -- --ignored
//! ```

#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use ecotokens::mcp::server::handle_rewrite;
use ecotokens::mcp::tools::RewriteParams;
use ecotokens::rewrite::provider::OllamaProvider;
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::tempdir;

fn live_model() -> String {
    std::env::var("ECOTOKENS_TEST_MODEL").unwrap_or_else(|_| "llama3.2:3b".to_string())
}

/// T094: the CLI, driven by the real `OllamaProvider`, round-trips a
/// paraphrase against an actual local model and produces plausible output.
#[test]
#[ignore]
fn cli_paraphrase_round_trips_against_a_real_model() {
    let tmp = tempdir().unwrap();
    let config_dir = tmp.path().join("ecotokens");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.json"),
        serde_json::json!({ "rewrite_model": live_model() }).to_string(),
    )
    .unwrap();

    let input = "The team plans to migrate the database next week if all tests pass.";
    let mut child = Command::new(ecotokens_bin())
        .args(["rewrite", "--mode", "paraphrase", "--json"])
        .env("XDG_CONFIG_HOME", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let v: Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    // Model output is non-deterministic — a real model may occasionally
    // fail open (e.g. a too-terse response), which is itself a valid,
    // already-covered outcome. Assert the *shape* holds either way.
    assert!(matches!(
        v["status"].as_str(),
        Some("transformed") | Some("fallback")
    ));
    assert!(!v["text"].as_str().unwrap().is_empty());
}

/// T094: same scenario through the MCP entry point (`handle_rewrite`),
/// proving the agent-facing path works against a real model too — not just
/// the CLI.
#[test]
#[ignore]
fn mcp_handle_rewrite_round_trips_against_a_real_model() {
    // Direct, in-process call to `handle_rewrite`/`rewrite()`: isolate
    // metrics recording from the developer's real config dir (only the CLI
    // test above gets this via its subprocess's own `XDG_CONFIG_HOME`).
    std::env::set_var(
        "XDG_CONFIG_HOME",
        std::env::temp_dir().join(format!("ecotokens-test-metrics-{}", std::process::id())),
    );
    let settings = ecotokens::config::Settings::default();
    let model = live_model();
    let provider =
        OllamaProvider::new(None, model.clone()).expect("localhost URL must be accepted");

    let params = RewriteParams {
        text: "Please fix the login bug as soon as possible, it is blocking everyone.".into(),
        mode: "tone".into(),
        target: Some("plain".into()),
        model: None,
    };

    let json = handle_rewrite(params, &settings, model, &provider)
        .expect("validation must pass — mode/target are valid");
    let v: Value = serde_json::from_str(&json).expect("valid JSON response");

    assert!(matches!(
        v["status"].as_str(),
        Some("transformed") | Some("fallback")
    ));
    assert!(
        v.get("diff_path").is_none(),
        "MCP response must omit diff_path"
    );
    assert!(!v["text"].as_str().unwrap().is_empty());
}
