//! Tests for the `ecotokens_rewrite` MCP tool (contracts/mcp-rewrite-tool.md).
//!
//! The `#[tool]`-annotated `EcotokensServer::ecotokens_rewrite` method always
//! constructs a real `OllamaProvider` from live `Settings` (by design — a
//! real tool call must talk to the real configured endpoint), so it cannot
//! take a `StubProvider`. The actual transformation logic lives in the
//! provider-parameterized `handle_rewrite` free function that method calls
//! into, which is what these tests exercise directly with `StubProvider` —
//! this is the same logic the tool uses, minus the OllamaProvider/Settings
//! plumbing that a unit test should not depend on (see the doc comment on
//! `handle_rewrite` in src/mcp/server.rs for the full rationale).

use ecotokens::config::Settings;
use ecotokens::mcp::server::handle_rewrite;
use ecotokens::mcp::tools::RewriteParams;
use ecotokens::rewrite::provider::{StubOutcome, StubProvider};
use ecotokens::rewrite::{rewrite, Origin, RewriteRequest};
use serde_json::Value;
use std::time::Duration;

/// Direct, in-process calls to `handle_rewrite`/`rewrite()` record real
/// metrics via `crate::metrics::store` if `XDG_CONFIG_HOME` resolves to the
/// developer's real config dir. The library's `#[cfg(not(test))]`/
/// `#[cfg(test)]` split does NOT protect against this: `cfg(test)` is not
/// propagated to a crate used as a dependency by an external test binary
/// like this one — only to the crate's own unit tests. `Once` makes this
/// race-free across the parallel test threads in this binary.
fn isolate_metrics_home() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let dir =
            std::env::temp_dir().join(format!("ecotokens-test-metrics-{}", std::process::id()));
        std::env::set_var("XDG_CONFIG_HOME", dir);
    });
}

fn params(text: &str, mode: &str, target: Option<&str>) -> RewriteParams {
    isolate_metrics_home();
    RewriteParams {
        text: text.to_string(),
        mode: mode.to_string(),
        target: target.map(str::to_string),
        model: None,
    }
}

/// T064 — response shape matches contracts/mcp-rewrite-tool.md field-for-field,
/// and `diff_path` is omitted entirely (FR-030, US5 scenario 4).
#[test]
fn response_matches_the_documented_schema_and_omits_diff_path() {
    let settings = Settings::default();
    let stub = StubProvider::with_response("Rewritten prose.");
    let out = handle_rewrite(
        params("Some prose to rewrite.", "paraphrase", None),
        &settings,
        "stub-model".to_string(),
        &stub,
    )
    .expect("valid request must succeed");

    let v: Value = serde_json::from_str(&out).expect("valid JSON");
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
    ] {
        assert!(v.get(field).is_some(), "missing field: {field}");
    }
    assert!(
        v.get("diff_path").is_none(),
        "diff_path must be omitted from the MCP response, got: {v}"
    );
    assert_eq!(v["status"], "transformed");
    assert_eq!(v["mode"], "paraphrase");
}

/// T065 — model failure is a successful tool result with status "fallback",
/// never a tool error (FR-031, US5 scenario 2).
#[test]
fn model_failure_returns_fallback_as_a_successful_result() {
    let settings = Settings::default();
    let stub = StubProvider::new();
    stub.push(StubOutcome::Error("model unreachable".to_string()));

    let out = handle_rewrite(
        params(
            "Text that must survive the model being down.",
            "paraphrase",
            None,
        ),
        &settings,
        "stub-model".to_string(),
        &stub,
    )
    .expect("a model failure must be Ok, not Err — it is not a validation failure");

    let v: Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(v["status"], "fallback");
    assert_eq!(v["text"], "Text that must survive the model being down.");
    assert!(v["reason"].is_string());
}

/// T066 — invalid mode/target is a tool error with zero provider calls
/// (US5 scenario 3).
#[test]
fn invalid_mode_is_an_error_with_zero_provider_calls() {
    let settings = Settings::default();
    let stub = StubProvider::with_response("should never be used");

    let err = handle_rewrite(
        params("Some text.", "not-a-real-mode", None),
        &settings,
        "stub-model".to_string(),
        &stub,
    )
    .expect_err("an invalid mode must be a tool error");

    assert!(err.contains("unknown mode"), "got: {err}");
    assert_eq!(stub.call_count(), 0);
}

#[test]
fn missing_required_target_is_an_error_with_zero_provider_calls() {
    let settings = Settings::default();
    let stub = StubProvider::with_response("should never be used");

    let err = handle_rewrite(
        params("Some text.", "tone", None),
        &settings,
        "stub-model".to_string(),
        &stub,
    )
    .expect_err("tone without a target must be a tool error");

    assert!(err.contains("requires"), "got: {err}");
    assert_eq!(stub.call_count(), 0);
}

#[test]
fn forbidden_target_is_an_error_with_zero_provider_calls() {
    let settings = Settings::default();
    let stub = StubProvider::with_response("should never be used");

    let err = handle_rewrite(
        params("Some text.", "paraphrase", Some("plain")),
        &settings,
        "stub-model".to_string(),
        &stub,
    )
    .expect_err("paraphrase forbids a target");

    assert!(err.contains("does not accept"), "got: {err}");
    assert_eq!(stub.call_count(), 0);
}

/// T067 — CLI and MCP call the identical `rewrite::rewrite()` core with the
/// same stub and produce identical `text`, `status`, and `chunk_count`
/// (FR-029, US5 scenario 1). The CLI itself runs as a subprocess linking the
/// real `OllamaProvider` (see tests/integration/rewrite_test.rs), so a
/// literal subprocess comparison against a stub isn't possible; this test
/// instead verifies the structural invariant directly: both surfaces are
/// thin wrappers around the same `rewrite()` entry point with the same
/// request shape (mode/target/text), which is what actually guarantees
/// behavioral equivalence between them.
#[test]
fn cli_and_mcp_paths_produce_identical_results_for_the_same_input_and_stub() {
    let text = "Please rewrite this paragraph for cross-surface comparison.";
    let settings = Settings::default();

    // MCP path.
    let mcp_stub = StubProvider::with_response("The rewritten paragraph.");
    let mcp_out = handle_rewrite(
        params(text, "paraphrase", None),
        &settings,
        "stub-model".to_string(),
        &mcp_stub,
    )
    .unwrap();
    let mcp_json: Value = serde_json::from_str(&mcp_out).unwrap();

    // CLI path — same entry point `cmd_rewrite` in main.rs calls, with
    // Origin::Cli instead of Origin::Mcp.
    let cli_stub = StubProvider::with_response("The rewritten paragraph.");
    let request = RewriteRequest {
        text: text.to_string(),
        mode: ecotokens::rewrite::Mode::parse("paraphrase", None).unwrap(),
        model: "stub-model".to_string(),
        timeout: Duration::from_secs(30),
        origin: Origin::Cli,
        truncation_ratio: settings.rewrite_truncation_ratio,
        context_tokens: settings.rewrite_context_tokens,
        save_diff: false,
        diff_dir: std::env::temp_dir(),
        diff_retention: settings.rewrite_diff_retention,
    };
    let cli_result = rewrite(request, &cli_stub).unwrap();

    assert_eq!(mcp_json["text"], cli_result.text);
    assert_eq!(
        mcp_json["status"],
        serde_json::to_value(cli_result.status).unwrap()
    );
    assert_eq!(mcp_json["chunk_count"], cli_result.chunk_count);
}
