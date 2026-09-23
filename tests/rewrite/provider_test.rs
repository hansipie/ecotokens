use ecotokens::rewrite::provider::{
    OllamaProvider, RewriteProvider, StubOutcome, StubProvider, WarnOnce,
};
use std::time::Duration;

/// T077 (FR-038): the automatic pipeline warns about an unreachable model at
/// most once per session, not once per interception. Each test gets its own
/// `WarnOnce` instance (rather than sharing the process-wide singleton used
/// by the real pipeline) so this is safe under parallel test execution.
#[test]
fn warn_once_prints_only_on_the_first_call() {
    let w = WarnOnce::new();
    assert!(w.warn("first"), "the first call must warn");
    assert!(!w.warn("second"), "subsequent calls must be silent");
    assert!(!w.warn("third"), "subsequent calls must be silent");
}

#[test]
fn stub_returns_canned_response() {
    let stub = StubProvider::with_response("hello world");
    let out = stub.generate("prompt", Duration::from_secs(1)).unwrap();
    assert_eq!(out, "hello world");
}

#[test]
fn stub_returns_error() {
    let stub = StubProvider::new();
    stub.push(StubOutcome::Error("boom".to_string()));
    let err = stub.generate("prompt", Duration::from_secs(1)).unwrap_err();
    assert!(format!("{err}").contains("boom"));
}

#[test]
fn stub_returns_timeout() {
    let stub = StubProvider::new();
    stub.push(StubOutcome::Timeout);
    let err = stub.generate("prompt", Duration::from_secs(1)).unwrap_err();
    assert!(format!("{err}").contains("timed out"));
}

#[test]
fn stub_returns_empty_string() {
    let stub = StubProvider::with_response("");
    let out = stub.generate("prompt", Duration::from_secs(1)).unwrap();
    assert_eq!(out, "");
}

#[test]
fn stub_returns_truncated_response() {
    let stub = StubProvider::with_response("short");
    let out = stub
        .generate("a long prompt with lots of content", Duration::from_secs(1))
        .unwrap();
    assert_eq!(out, "short");
}

#[test]
fn stub_records_call_count() {
    let stub = StubProvider::with_response("x");
    assert_eq!(stub.call_count(), 0);
    let _ = stub.generate("p1", Duration::from_secs(1));
    let _ = stub.generate("p2", Duration::from_secs(1));
    assert_eq!(stub.call_count(), 2);
}

mod localhost_guard {
    use super::*;

    #[test]
    fn accepts_localhost() {
        OllamaProvider::new(Some("http://localhost:11434"), "m".into())
            .expect("localhost must be accepted");
    }

    #[test]
    fn accepts_127_0_0_1() {
        OllamaProvider::new(Some("http://127.0.0.1:11434"), "m".into())
            .expect("127.0.0.1 must be accepted");
    }

    #[test]
    fn accepts_ipv6_loopback() {
        OllamaProvider::new(Some("http://[::1]:11434"), "m".into()).expect("::1 must be accepted");
    }

    #[test]
    fn rejects_a_remote_host_before_any_request() {
        let err = OllamaProvider::new(Some("http://example.com:11434"), "m".into())
            .expect_err("a remote host must be rejected");
        assert!(err.contains("localhost"), "got: {err}");
    }

    #[test]
    fn rejects_a_public_ip() {
        let err = OllamaProvider::new(Some("http://8.8.8.8:11434"), "m".into())
            .expect_err("a public IP must be rejected");
        assert!(err.contains("localhost"), "got: {err}");
    }

    #[test]
    fn default_url_is_localhost() {
        OllamaProvider::new(None, "m".into()).expect("default URL must be localhost");
    }
}
