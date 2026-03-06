use std::process::Command;
use tempfile::TempDir;

fn ecotokens() -> String {
    env!("CARGO_BIN_EXE_ecotokens").to_string()
}

// ── T045d — SC-006 : fallback silencieux ─────────────────────────────────────

#[test]
fn filter_with_unreadable_command_exits_cleanly() {
    // Running a command that doesn't exist: filter should exit with non-zero
    // but NOT produce a panic (no "thread panicked" in stderr)
    let out = Command::new(ecotokens())
        .args(["filter", "--", "this_command_does_not_exist_xyz"])
        .output()
        .expect("ecotokens itself should not crash");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("thread") || !stderr.contains("panicked"),
        "should not produce a Rust panic, got: {stderr}"
    );
}

#[test]
fn filter_passes_through_original_content_on_error() {
    let tmp = TempDir::new().unwrap();
    let fixture = tmp.path().join("data.txt");
    std::fs::write(&fixture, "important output line\n").unwrap();

    let out = Command::new(ecotokens())
        .args(["filter", "--", "cat", fixture.to_str().unwrap()])
        .output()
        .expect("filter should run");

    assert!(out.status.success(), "filter of cat should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("important output line"),
        "content should be passed through: {stdout}"
    );
}

#[test]
fn hook_with_invalid_json_does_not_panic() {
    // Send garbage JSON to hook — should handle gracefully (passthrough)
    let out = Command::new(ecotokens())
        .args(["hook"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("hook should start");

    // Write garbage then close stdin
    use std::io::Write;
    let mut child = out;
    if let Some(ref mut stdin) = child.stdin {
        let _ = stdin.write_all(b"{not valid json}");
    }
    let output = child.wait_with_output().unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "hook should not panic on invalid JSON, got: {stderr}"
    );
}
