use std::process::Command;

fn ecotokens() -> String {
    env!("CARGO_BIN_EXE_ecotokens").to_string()
}

// ── T041 — Conformité constitution ────────────────────────────────────────────

/// All subcommands that support --json should output valid JSON.
#[test]
fn config_json_flag_outputs_valid_json() {
    let out = Command::new(ecotokens())
        .args(["config", "--json"])
        .output()
        .expect("config --json should run");
    assert!(out.status.success(), "exit code should be 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(
        v.is_ok(),
        "config --json must produce valid JSON, got: {stdout}"
    );
}

#[test]
fn gain_json_flag_outputs_valid_json() {
    let out = Command::new(ecotokens())
        .args(["gain", "--json"])
        .output()
        .expect("gain --json should run");
    assert!(out.status.success(), "exit code should be 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(
        v.is_ok(),
        "gain --json must produce valid JSON, got: {stdout}"
    );
}

/// Errors must go to stderr, not stdout.
#[test]
fn missing_subcommand_error_on_stderr() {
    let out = Command::new(ecotokens())
        .args(["symbol", "nonexistent::id#fn"])
        .output()
        .expect("symbol should run");
    // Either exit 1 with message on stderr, or exit 0 with "not found" on stderr
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() {
        assert!(
            !stderr.is_empty(),
            "errors should appear on stderr, not stdout: stdout={stdout}"
        );
    }
}

/// FR-009 — no network calls: filter pipeline must work without network.
#[test]
fn filter_runs_without_network_access() {
    use tempfile::TempDir;
    let tmp = TempDir::new().unwrap();
    let fixture = tmp.path().join("data.txt");
    std::fs::write(&fixture, "hello\nworld\n").unwrap();

    // Run with network disabled via env (no actual network isolation here,
    // but we verify the command doesn't block or fail without connectivity)
    let out = Command::new(ecotokens())
        .args(["filter", "--", "cat", fixture.to_str().unwrap()])
        .env("http_proxy", "http://127.0.0.1:1") // invalid proxy → network errors if any
        .env("https_proxy", "http://127.0.0.1:1")
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .output()
        .expect("filter should run");
    assert!(
        out.status.success(),
        "filter should succeed without network: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Return codes: successful commands exit 0.
#[test]
fn successful_commands_exit_zero() {
    let cmds: &[&[&str]] = &[&["config"], &["config", "--json"], &["gain", "--json"]];
    for cmd in cmds {
        let out = Command::new(ecotokens())
            .args(*cmd)
            .output()
            .expect("command should run");
        assert!(
            out.status.success(),
            "command {:?} should exit 0, got: {}",
            cmd,
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
