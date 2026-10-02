#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use ecotokens::tokens::count_tokens;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

const PROSE: &str = "The quarterly results show strong growth across every region this year. \
    Customer satisfaction improved significantly, driven mainly by faster response times and a \
    redesigned onboarding flow that new users have consistently praised in feedback surveys \
    collected over the last six months by the product team.";

/// Responds to every connection with a valid Ollama-shaped JSON response,
/// counting how many connections it received.
fn spawn_counting_server(response_text: &str) -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let count = Arc::new(AtomicUsize::new(0));
    let count_clone = count.clone();
    let body = format!(r#"{{"response": {:?}}}"#, response_text);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            count_clone.fetch_add(1, Ordering::SeqCst);
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    (port, count)
}

/// Accepts connections but never responds — simulates a hung/unreachable model.
fn spawn_never_responds() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let count = Arc::new(AtomicUsize::new(0));
    let count_clone = count.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            count_clone.fetch_add(1, Ordering::SeqCst);
            if let Ok(stream) = stream {
                std::thread::sleep(std::time::Duration::from_secs(5));
                drop(stream);
            }
        }
    });
    (port, count)
}

fn write_config(dir: &std::path::Path, json: &str) {
    let config_dir = dir.join("ecotokens");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    std::fs::write(config_dir.join("config.json"), json).expect("write config.json");
}

fn run_filter_output(config_home: &std::path::Path, input: &str) -> std::process::Output {
    let mut child = Command::new(ecotokens_bin())
        .args([
            "filter-output",
            "--command",
            "auto-test",
            "--exit-code",
            "0",
        ])
        .env("XDG_CONFIG_HOME", config_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ecotokens filter-output");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().expect("wait")
}

/// T073 (FR-034, SC-010): with `rewrite_auto_enabled = false` (the default),
/// the pipeline makes zero provider calls and behavior is unchanged, even
/// when a `rewrite_url` happens to be configured.
#[test]
fn auto_disabled_by_default_makes_zero_provider_calls() {
    let tmp = tempdir().unwrap();
    let (port, count) = spawn_counting_server("SHOULD NEVER BE USED");
    write_config(
        tmp.path(),
        &format!(r#"{{"rewrite_url": "http://127.0.0.1:{port}"}}"#),
    );

    let out = run_filter_output(tmp.path(), PROSE);
    assert!(out.status.success());
    assert_eq!(
        count.load(Ordering::SeqCst),
        0,
        "auto-rewrite must not fire when disabled"
    );
}

/// T075 (FR-036): content below `rewrite_auto_min_tokens` passes through
/// with no provider call, even with auto-rewrite enabled.
#[test]
fn content_below_min_tokens_makes_zero_provider_calls() {
    let tmp = tempdir().unwrap();
    let (port, count) = spawn_counting_server("SHOULD NEVER BE USED");
    write_config(
        tmp.path(),
        &serde_json::json!({
            "rewrite_auto_enabled": true,
            "rewrite_auto_mode": "paraphrase",
            "rewrite_auto_min_tokens": 100_000,
            "rewrite_url": format!("http://127.0.0.1:{port}"),
        })
        .to_string(),
    );

    let out = run_filter_output(tmp.path(), PROSE);
    assert!(out.status.success());
    assert_eq!(
        count.load(Ordering::SeqCst),
        0,
        "below-threshold content must skip the model"
    );
}

/// T076 (FR-037): a stage exceeding `rewrite_auto_timeout_ms` passes content
/// through unchanged rather than hanging the interception.
#[test]
fn timeout_passes_content_through_unchanged() {
    let tmp = tempdir().unwrap();
    let (port, _count) = spawn_never_responds();
    write_config(
        tmp.path(),
        &serde_json::json!({
            "rewrite_auto_enabled": true,
            "rewrite_auto_mode": "paraphrase",
            "rewrite_auto_min_tokens": 1,
            "rewrite_auto_timeout_ms": 200,
            "rewrite_url": format!("http://127.0.0.1:{port}"),
        })
        .to_string(),
    );

    let start = std::time::Instant::now();
    let out = run_filter_output(tmp.path(), PROSE);
    let elapsed = start.elapsed();

    assert!(out.status.success());
    assert!(
        elapsed < std::time::Duration::from_secs(3),
        "must fail open on timeout rather than hang: took {elapsed:?}"
    );
}

/// T078 (FR-039): content carrying masked secrets is never auto-transformed.
#[test]
fn content_carrying_a_secret_makes_zero_provider_calls() {
    let tmp = tempdir().unwrap();
    let (port, count) = spawn_counting_server("SHOULD NEVER BE USED");
    write_config(
        tmp.path(),
        &serde_json::json!({
            "rewrite_auto_enabled": true,
            "rewrite_auto_mode": "paraphrase",
            "rewrite_auto_min_tokens": 1,
            "rewrite_url": format!("http://127.0.0.1:{port}"),
        })
        .to_string(),
    );

    let secret = format!("sk-ant-api03-{}AA", "x".repeat(93));
    let input = format!(
        "Here is the API key you asked about for the integration: {secret}. \
        Please make sure to store it securely and rotate it every ninety days as our \
        security policy requires for all long-lived service credentials."
    );

    let out = run_filter_output(tmp.path(), &input);
    assert!(out.status.success());
    assert_eq!(
        count.load(Ordering::SeqCst),
        0,
        "content carrying a secret must never reach the model"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains(&secret),
        "the raw secret must never appear in output"
    );
}

/// T079: the specific regression research.md §10 predicts — an *expanding*
/// transformation must survive the pipeline rather than being silently
/// clamped back to the shorter pre-rewrite text by the anti-expansion clamp
/// (which runs strictly before this stage, not after).
#[test]
fn an_expanding_transformation_survives_the_pipeline() {
    let tmp = tempdir().unwrap();
    let expanded =
        PROSE.to_string() + " " + &"Additional elaborated context and detail. ".repeat(20);
    let (port, count) = spawn_counting_server(&expanded);
    write_config(
        tmp.path(),
        &serde_json::json!({
            "rewrite_auto_enabled": true,
            "rewrite_auto_mode": "paraphrase",
            "rewrite_auto_min_tokens": 1,
            "rewrite_url": format!("http://127.0.0.1:{port}"),
        })
        .to_string(),
    );

    let out = run_filter_output(tmp.path(), PROSE);
    assert!(out.status.success());
    assert!(
        count.load(Ordering::SeqCst) >= 1,
        "the model must have been called"
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let out_tokens = count_tokens(&stdout);
    let in_tokens = count_tokens(PROSE);
    assert!(
        out_tokens > in_tokens,
        "expansion must survive: {out_tokens} tokens out vs {in_tokens} in"
    );
}
