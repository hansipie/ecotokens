#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use tempfile::tempdir;

fn spawn_never_responds() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            std::thread::sleep(std::time::Duration::from_secs(5));
            drop(stream);
        }
    });
    port
}

fn spawn_bad_response() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\nContent-Type: application/json\r\n\r\nnot-json!",
            );
        }
    });
    port
}

fn spawn_connection_reset() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            drop(stream);
        }
    });
    port
}

fn run_rewrite_against(input: &str, url: &str, timeout_ms: Option<&str>) -> std::process::Output {
    let tmp = tempdir().expect("tempdir");
    let config_dir = tmp.path().join("ecotokens");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    std::fs::write(
        config_dir.join("config.json"),
        format!(r#"{{"rewrite_url": "{url}"}}"#),
    )
    .expect("write config.json");

    let mut args = vec!["rewrite", "--mode", "paraphrase", "--json"];
    if let Some(t) = timeout_ms {
        args.push("--timeout-ms");
        args.push(t);
    }

    let mut child = Command::new(ecotokens_bin())
        .args(&args)
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
    child.wait_with_output().expect("wait")
}

fn assert_fallback_is_byte_identical(input: &str, out: &std::process::Output) {
    assert_eq!(out.status.code(), Some(0), "fallback must still exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(&stdout).expect("valid JSON on fallback");
    assert_eq!(v["status"], "fallback");
    assert_eq!(v["text"], input);
    assert!(v["reason"].is_string());
}

#[test]
fn unreachable_endpoint_fails_open() {
    let input = "Text that must survive an unreachable endpoint.";
    // Nothing is listening on this ephemeral port.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener); // free the port so the connection is refused

    let out = run_rewrite_against(input, &format!("http://127.0.0.1:{port}"), None);
    assert_fallback_is_byte_identical(input, &out);
}

#[test]
fn provider_error_fails_open() {
    let input = "Text that must survive a connection reset.";
    let port = spawn_connection_reset();
    let out = run_rewrite_against(input, &format!("http://127.0.0.1:{port}"), None);
    assert_fallback_is_byte_identical(input, &out);
}

#[test]
fn timeout_fails_open() {
    let input = "Text that must survive a request timeout.";
    let port = spawn_never_responds();
    let out = run_rewrite_against(input, &format!("http://127.0.0.1:{port}"), Some("200"));
    assert_fallback_is_byte_identical(input, &out);
}

#[test]
fn unusable_response_fails_open() {
    let input = "Text that must survive an unparseable response.";
    let port = spawn_bad_response();
    let out = run_rewrite_against(input, &format!("http://127.0.0.1:{port}"), None);
    assert_fallback_is_byte_identical(input, &out);
}
