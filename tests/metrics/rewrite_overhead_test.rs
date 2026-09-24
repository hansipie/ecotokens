#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use tempfile::tempdir;

const PROSE: &str = "The quarterly results show strong growth across every region this year. \
    Customer satisfaction improved significantly, driven mainly by faster response times and a \
    redesigned onboarding flow that new users have consistently praised in feedback surveys \
    collected over the last six months by the product team.";

fn spawn_expanding_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let expanded = PROSE.to_string() + " " + &"Additional elaborated context. ".repeat(30);
    let body = serde_json::json!({ "response": expanded }).to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
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
    port
}

/// T080 (FR-041a, SC-012): auto-stage token expansion is reported as a
/// non-zero `rewrite_overhead_tokens` figure in `ecotokens gain --json`,
/// never silently excluded like the local `Cli`/`Mcp` rewrite rows are.
#[test]
fn auto_stage_expansion_is_visible_in_gain_json() {
    let tmp = tempdir().unwrap();
    let config_dir = tmp.path().join("ecotokens");
    std::fs::create_dir_all(&config_dir).unwrap();
    let port = spawn_expanding_server();
    std::fs::write(
        config_dir.join("config.json"),
        serde_json::json!({
            "rewrite_auto_enabled": true,
            "rewrite_auto_mode": "paraphrase",
            "rewrite_auto_min_tokens": 1,
            "rewrite_url": format!("http://127.0.0.1:{port}"),
        })
        .to_string(),
    )
    .unwrap();

    // One interception that triggers the auto-rewrite stage, writing to the
    // isolated metrics DB under this temp XDG_CONFIG_HOME.
    let mut child = Command::new(ecotokens_bin())
        .args([
            "filter-output",
            "--command",
            "overhead-test",
            "--exit-code",
            "0",
        ])
        .env("XDG_CONFIG_HOME", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn filter-output");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(PROSE.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("wait");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let gain_out = Command::new(ecotokens_bin())
        .args(["gain", "--json"])
        .env("XDG_CONFIG_HOME", tmp.path())
        .output()
        .expect("run gain --json");
    assert!(gain_out.status.success());

    let report: Value = serde_json::from_str(&String::from_utf8_lossy(&gain_out.stdout))
        .expect("valid JSON gain report");

    let overhead = report["rewrite_overhead_tokens"]
        .as_u64()
        .expect("rewrite_overhead_tokens must be present and numeric");
    assert!(
        overhead > 0,
        "auto-stage expansion must be surfaced as overhead, got {overhead}"
    );

    // And it must not have polluted the ordinary savings fields — this is
    // the SC-008 guarantee extended to the auto-stage row (see
    // tests/metrics/rewrite_exclusion_test.rs for the unit-level version).
    assert!(report["total_savings_pct"].as_f64().unwrap() >= 0.0);
}
