#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::tempdir;

fn isolated_config_dir() -> tempfile::TempDir {
    let tmp = tempdir().expect("tempdir");
    let config_dir = tmp.path().join("ecotokens");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    // Point at a guaranteed-unreachable local port so every case below is a
    // deterministic fail-open, never a real model call.
    std::fs::write(
        config_dir.join("config.json"),
        r#"{"rewrite_url": "http://127.0.0.1:1"}"#,
    )
    .expect("write config.json");
    tmp
}

#[test]
fn reads_from_stdin_by_default() {
    let tmp = isolated_config_dir();
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
        .write_all(b"stdin content")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(v["text"], "stdin content");
}

#[test]
fn reads_from_file_flag() {
    let tmp = isolated_config_dir();
    let file_path = tmp.path().join("input.txt");
    std::fs::write(&file_path, "file content").unwrap();

    // No stdin piped — spawn with a closed stdin (not a tty, but empty/EOF
    // immediately) so the process does not block waiting for input; --file
    // must be honored regardless.
    let out = Command::new(ecotokens_bin())
        .args(["rewrite", "--mode", "paraphrase", "--json", "--file"])
        .arg(&file_path)
        .env("XDG_CONFIG_HOME", tmp.path())
        .stdin(Stdio::null())
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(v["text"], "file content");
}

#[test]
fn both_stdin_and_file_is_an_error() {
    let tmp = isolated_config_dir();
    let file_path = tmp.path().join("input.txt");
    std::fs::write(&file_path, "file content").unwrap();

    let mut child = Command::new(ecotokens_bin())
        .args(["rewrite", "--mode", "paraphrase", "--file"])
        .arg(&file_path)
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
        .write_all(b"also piped")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

// "Neither stdin nor --file supplied" (stdin is an interactive terminal with
// nothing piped) cannot be simulated from this test harness without a pty —
// piping an empty/closed stdin is indistinguishable from a script redirecting
// `/dev/null`, and is covered instead by `empty_stdin_is_a_no_op` below,
// which exercises the CLI's actual behavior for that input shape.

#[test]
fn empty_stdin_is_a_no_op() {
    let tmp = isolated_config_dir();
    let mut child = Command::new(ecotokens_bin())
        .args(["rewrite", "--mode", "paraphrase", "--json"])
        .env("XDG_CONFIG_HOME", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    drop(child.stdin.take()); // close immediately: EOF, zero bytes
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(v["status"], "no_op");
}

#[test]
fn whitespace_only_stdin_is_a_no_op() {
    let tmp = isolated_config_dir();
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
        .write_all(b"   \n\t  \n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(v["status"], "no_op");
}

#[test]
fn invalid_utf8_stdin_is_rejected() {
    let tmp = isolated_config_dir();
    let mut child = Command::new(ecotokens_bin())
        .args(["rewrite", "--mode", "paraphrase"])
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
        .write_all(&[0xff, 0xfe, 0x00, 0xff])
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}
