#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn filter_output_filters_existing_stdout_from_stdin() {
    let mut child = Command::new(ecotokens_bin())
        .args([
            "filter-output",
            "--command",
            "cargo test",
            "--exit-code",
            "0",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ecotokens filter-output");

    {
        let stdin = child.stdin.as_mut().expect("stdin should be piped");
        for i in 0..500 {
            writeln!(stdin, "test noisy_case_{i} ... ok").unwrap();
        }
        writeln!(stdin, "test important_failure ... FAILED").unwrap();
        writeln!(stdin, "failures: important_failure").unwrap();
    }

    let out = child.wait_with_output().expect("filter-output should exit");
    assert!(
        out.status.success(),
        "filter-output should not propagate the original exit code, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("important_failure"));
    assert!(
        stdout.len() < 8_000,
        "filtered output should be compact, got {} chars",
        stdout.len()
    );
}

#[test]
fn filter_output_accepts_cwd_and_debug_flags() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut child = Command::new(ecotokens_bin())
        .args([
            "filter-output",
            "--command",
            "git status",
            "--exit-code",
            "0",
            "--cwd",
            dir.path().to_str().unwrap(),
            "--debug",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ecotokens filter-output");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"On branch main\nnothing to commit\n")
        .unwrap();

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("On branch main"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("tokens_before"));
}
