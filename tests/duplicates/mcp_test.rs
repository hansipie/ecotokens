#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn setup_duplicate_fixture() -> (TempDir, TempDir) {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();

    let func = "fn compute(x: i32) -> i32 {\n    let a = x * 2;\n    let b = a + 1;\n    let c = b - x;\n    let d = c * 3;\n    d\n}\n";
    fs::write(src.path().join("a.rs"), func).unwrap();
    fs::write(src.path().join("b.rs"), func).unwrap();

    let status = Command::new(ecotokens_bin())
        .args([
            "index",
            "--path",
            &src.path().to_string_lossy(),
            "--index-dir",
            &idx.path().to_string_lossy(),
        ])
        .env("ECOTOKENS_BATCH", "1")
        .status()
        .expect("ecotokens index should run");
    assert!(status.success());

    (src, idx)
}

fn send_jsonrpc(
    stdin: &mut impl Write,
    stdout: &mut impl BufRead,
    request: &serde_json::Value,
) -> serde_json::Value {
    let req_str = serde_json::to_string(request).unwrap();
    writeln!(stdin, "{req_str}").unwrap();
    stdin.flush().unwrap();

    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert!(!line.is_empty(), "expected response from MCP server");
    serde_json::from_str(line.trim()).expect(&format!("failed to parse JSON: {line}"))
}

#[test]
fn test_mcp_duplicates_tool() {
    let (_src, idx) = setup_duplicate_fixture();

    let mut child = Command::new(ecotokens_bin())
        .args(["mcp", "--index-dir", &idx.path().to_string_lossy()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("should start mcp server");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut stdout = BufReader::new(stdout);

    // Initialize
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0.1.0" }
        }
    });
    send_jsonrpc(&mut stdin, &mut stdout, &init);

    // Send initialized notification
    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let msg = serde_json::to_string(&initialized).unwrap();
    writeln!(stdin, "{msg}").unwrap();
    stdin.flush().unwrap();

    // Call ecotokens_duplicates
    let call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "ecotokens_duplicates",
            "arguments": {
                "threshold": 70,
                "min_lines": 5,
                "top_k": 10
            }
        }
    });
    let resp = send_jsonrpc(&mut stdin, &mut stdout, &call);

    child.kill().ok();
    child.wait().ok();

    let content = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("");
    assert!(
        content.contains("group") || content.contains("duplicate") || content.contains("similar"),
        "response should contain duplicate info: {content}"
    );
}
