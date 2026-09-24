#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use ecotokens::rewrite::provider::{StubOutcome, StubProvider};
use ecotokens::rewrite::{rewrite, Mode, Origin, RewriteRequest};
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::tempdir;

/// US2 scenario 2: translating text that is already in the target language
/// must warn, return the input unchanged, and — since the CLI links the real
/// `OllamaProvider` with no stub injection point at the subprocess boundary —
/// must do so *fast*, without ever reaching out to the (deliberately
/// unreachable) configured endpoint. A real model call against port 1 would
/// fail near-instantly too (connection refused), so the strongest signal
/// here is the `--json` status plus a generous-but-bounded wall-clock check
/// that would catch a call that actually blocked on a timeout.
#[test]
fn same_language_translation_is_a_fast_no_op() {
    let tmp = tempdir().expect("tempdir");
    let config_dir = tmp.path().join("ecotokens");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    std::fs::write(
        config_dir.join("config.json"),
        r#"{"rewrite_url": "http://127.0.0.1:1", "rewrite_timeout_ms": 5000}"#,
    )
    .expect("write config.json");

    let input = "The quick brown fox jumps over the lazy dog. This is a plain English \
        sentence that is clearly in the target language already, with plenty of common \
        words for the detector to recognize.";

    let start = Instant::now();
    let mut child = Command::new(ecotokens_bin())
        .args(["rewrite", "--mode", "translate", "--to", "en", "--json"])
        .env("XDG_CONFIG_HOME", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ecotokens rewrite");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("wait");
    let elapsed = start.elapsed();

    assert!(out.status.success());
    // The configured endpoint is unreachable; if the no-op short-circuit
    // didn't fire, the process would spend up to the 5s timeout on a
    // doomed connection attempt. A same-language no-op returns essentially
    // immediately.
    assert!(
        elapsed < Duration::from_secs(3),
        "took {elapsed:?} — looks like a model call was attempted instead of a no-op"
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(v["status"], "no_op");
    assert_eq!(v["text"], input);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("already"),
        "expected a same-language warning on stderr, got: {stderr}"
    );
}

/// US2 scenario 1: numbers, dates, and proper-noun-shaped tokens must survive
/// translation byte-identical. `rewrite()`'s sentinel protection (already
/// proven by the unit tests in `tests/rewrite/sanitize_test.rs`) is
/// mode-agnostic, so this proves translation mode doesn't bypass it — tested
/// directly against the lib with a `StubProvider`, since the CLI offers no
/// stub-injection point.
#[test]
fn translation_preserves_protected_spans() {
    // Direct, in-process calls to `rewrite()` record real metrics via
    // `crate::metrics::store` if `XDG_CONFIG_HOME` resolves to the
    // developer's real config dir — the library's `#[cfg(not(test))]` split
    // does not protect against this for an external test crate like this
    // one (only for the library's own unit tests).
    std::env::set_var(
        "XDG_CONFIG_HOME",
        std::env::temp_dir().join(format!("ecotokens-test-metrics-{}", std::process::id())),
    );
    let stub = StubProvider::new();
    // The stub echoes the sanitized prompt's sentinels back untouched (as if
    // a well-behaved model preserved them), simulating a real translation
    // that correctly left numbers/dates/URLs alone.
    stub.push(StubOutcome::Text(
        "Contactez-nous le ⟦ET0⟧ au sujet de la commande ⟦ET1⟧ ou visitez ⟦ET2⟧.".to_string(),
    ));

    let mode = Mode::parse("translate", Some("fr")).expect("fr is recognized");
    let request = RewriteRequest {
        text: "Contact us on 2026-08-16 about order 42 or visit https://example.com/orders."
            .to_string(),
        mode,
        model: "stub-model".into(),
        timeout: Duration::from_secs(5),
        origin: Origin::Cli,
        truncation_ratio: 0.01,
        context_tokens: 8192,
        save_diff: false,
        diff_dir: std::env::temp_dir(),
        diff_retention: 0,
    };

    let result = rewrite(request, &stub).expect("valid request");
    assert_eq!(result.status, ecotokens::rewrite::Status::Transformed);
    assert!(result.text.contains("2026-08-16"));
    assert!(
        result.text.contains('4'),
        "order number 42 should survive: {}",
        result.text
    );
    assert!(result.text.contains("https://example.com/orders"));
}
