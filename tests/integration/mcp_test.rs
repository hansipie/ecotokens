#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn setup_indexed_fixture() -> (TempDir, TempDir) {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();

    fs::write(
        src.path().join("lib.rs"),
        "pub fn greet(name: &str) -> String { format!(\"hello {name}\") }\npub struct Config;\n",
    )
    .unwrap();

    let status = Command::new(ecotokens_bin())
        .args([
            "index",
            "--path",
            &src.path().to_string_lossy(),
            "--index-dir",
            &idx.path().to_string_lossy(),
        ])
        .status()
        .expect("ecotokens index should run");
    assert!(status.success(), "ecotokens index failed");

    (src, idx)
}

/// Send a newline-delimited JSON-RPC message and read the response line.
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
    assert!(
        !line.is_empty(),
        "expected response from MCP server, got empty line"
    );
    serde_json::from_str(line.trim()).expect(&format!("failed to parse response JSON: {line}"))
}

fn spawn_mcp(idx: &TempDir) -> std::process::Child {
    Command::new(ecotokens_bin())
        .args(["mcp", "--index-dir", &idx.path().to_string_lossy()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("should start mcp server")
}

fn init_request() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0.1.0" }
        }
    })
}

fn initialized_notification() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    })
}

// ── T055 — MCP server tests ─────────────────────────────────────────────────

#[test]
fn mcp_server_responds_to_initialize() {
    let (_src, idx) = setup_indexed_fixture();
    let mut child = spawn_mcp(&idx);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let resp = send_jsonrpc(&mut stdin, &mut stdout, &init_request());
    assert!(
        resp.get("result").is_some(),
        "initialize should return result, got: {resp}"
    );
    let result = &resp["result"];
    assert!(
        result["serverInfo"]["name"]
            .as_str()
            .unwrap_or("")
            .contains("ecotokens"),
        "server name should contain ecotokens, got: {result}"
    );

    drop(stdin);
    let _ = child.kill();
}

#[test]
fn mcp_tool_outline_returns_symbols() {
    let (src, idx) = setup_indexed_fixture();
    let mut child = spawn_mcp(&idx);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let _ = send_jsonrpc(&mut stdin, &mut stdout, &init_request());
    // Send initialized notification (no response expected for notifications)
    let notif = serde_json::to_string(&initialized_notification()).unwrap();
    writeln!(stdin, "{notif}").unwrap();
    stdin.flush().unwrap();

    let call = serde_json::json!({
        "jsonrpc": "2.0", "id": 2,
        "method": "tools/call",
        "params": {
            "name": "ecotokens_outline",
            "arguments": { "path": src.path().join("lib.rs").to_string_lossy() }
        }
    });
    let resp = send_jsonrpc(&mut stdin, &mut stdout, &call);
    assert!(
        resp.get("result").is_some(),
        "ecotokens_outline should return result, got: {resp}"
    );

    drop(stdin);
    let _ = child.kill();
}

#[test]
fn mcp_tool_search_returns_results() {
    let (_src, idx) = setup_indexed_fixture();
    let mut child = spawn_mcp(&idx);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let _ = send_jsonrpc(&mut stdin, &mut stdout, &init_request());
    let notif = serde_json::to_string(&initialized_notification()).unwrap();
    writeln!(stdin, "{notif}").unwrap();
    stdin.flush().unwrap();

    let call = serde_json::json!({
        "jsonrpc": "2.0", "id": 2,
        "method": "tools/call",
        "params": {
            "name": "ecotokens_search",
            "arguments": { "query": "greet", "top_k": 3 }
        }
    });
    let resp = send_jsonrpc(&mut stdin, &mut stdout, &call);
    assert!(
        resp.get("result").is_some(),
        "ecotokens_search should return result, got: {resp}"
    );

    drop(stdin);
    let _ = child.kill();
}

#[test]
fn mcp_tool_symbol_returns_snippet() {
    let (_src, idx) = setup_indexed_fixture();
    let mut child = spawn_mcp(&idx);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let _ = send_jsonrpc(&mut stdin, &mut stdout, &init_request());
    let notif = serde_json::to_string(&initialized_notification()).unwrap();
    writeln!(stdin, "{notif}").unwrap();
    stdin.flush().unwrap();

    let call = serde_json::json!({
        "jsonrpc": "2.0", "id": 2,
        "method": "tools/call",
        "params": {
            "name": "ecotokens_symbol",
            "arguments": { "id": "lib.rs::greet#fn" }
        }
    });
    let resp = send_jsonrpc(&mut stdin, &mut stdout, &call);
    assert!(
        resp.get("result").is_some(),
        "ecotokens_symbol should return result, got: {resp}"
    );

    drop(stdin);
    let _ = child.kill();
}

#[test]
fn mcp_tool_trace_callers_returns_edges() {
    let (_src, idx) = setup_indexed_fixture();
    let mut child = spawn_mcp(&idx);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let _ = send_jsonrpc(&mut stdin, &mut stdout, &init_request());
    let notif = serde_json::to_string(&initialized_notification()).unwrap();
    writeln!(stdin, "{notif}").unwrap();
    stdin.flush().unwrap();

    let call = serde_json::json!({
        "jsonrpc": "2.0", "id": 2,
        "method": "tools/call",
        "params": {
            "name": "ecotokens_trace_callers",
            "arguments": { "symbol": "greet" }
        }
    });
    let resp = send_jsonrpc(&mut stdin, &mut stdout, &call);
    assert!(
        resp.get("result").is_some(),
        "ecotokens_trace_callers should return result, got: {resp}"
    );

    drop(stdin);
    let _ = child.kill();
}

#[test]
fn mcp_tool_trace_callees_returns_edges() {
    let (_src, idx) = setup_indexed_fixture();
    let mut child = spawn_mcp(&idx);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let _ = send_jsonrpc(&mut stdin, &mut stdout, &init_request());
    let notif = serde_json::to_string(&initialized_notification()).unwrap();
    writeln!(stdin, "{notif}").unwrap();
    stdin.flush().unwrap();

    let call = serde_json::json!({
        "jsonrpc": "2.0", "id": 2,
        "method": "tools/call",
        "params": {
            "name": "ecotokens_trace_callees",
            "arguments": { "symbol": "greet", "depth": 1 }
        }
    });
    let resp = send_jsonrpc(&mut stdin, &mut stdout, &call);
    assert!(
        resp.get("result").is_some(),
        "ecotokens_trace_callees should return result, got: {resp}"
    );

    drop(stdin);
    let _ = child.kill();
}

#[test]
fn mcp_tool_run_executes_and_filters_command() {
    let (_src, idx) = setup_indexed_fixture();
    let mut child = spawn_mcp(&idx);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let _ = send_jsonrpc(&mut stdin, &mut stdout, &init_request());
    let notif = serde_json::to_string(&initialized_notification()).unwrap();
    writeln!(stdin, "{notif}").unwrap();
    stdin.flush().unwrap();

    let call = serde_json::json!({
        "jsonrpc": "2.0", "id": 2,
        "method": "tools/call",
        "params": {
            "name": "ecotokens_run",
            "arguments": { "command": "echo hello ecotokens" }
        }
    });
    let resp = send_jsonrpc(&mut stdin, &mut stdout, &call);
    assert!(
        resp.get("result").is_some(),
        "ecotokens_run should return result, got: {resp}"
    );
    let content = resp["result"]["content"][0]["text"].as_str().unwrap_or("");
    assert!(
        content.contains("hello ecotokens"),
        "filtered output should contain command output, got: {content}"
    );

    drop(stdin);
    let _ = child.kill();
}

#[test]
fn mcp_tool_run_honors_cwd_parameter() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();

    fs::write(src.path().join("hello.txt"), "hi").unwrap();

    let status = Command::new(ecotokens_bin())
        .args([
            "index",
            "--path",
            &src.path().to_string_lossy(),
            "--index-dir",
            &idx.path().to_string_lossy(),
        ])
        .status()
        .expect("ecotokens index should run");
    assert!(status.success(), "ecotokens index failed");

    let mut child = spawn_mcp(&idx);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let _ = send_jsonrpc(&mut stdin, &mut stdout, &init_request());
    let notif = serde_json::to_string(&initialized_notification()).unwrap();
    writeln!(stdin, "{notif}").unwrap();
    stdin.flush().unwrap();

    let call = serde_json::json!({
        "jsonrpc": "2.0", "id": 3,
        "method": "tools/call",
        "params": {
            "name": "ecotokens_run",
            "arguments": {
                "command": "ls hello.txt",
                "cwd": src.path().to_string_lossy()
            }
        }
    });
    let resp = send_jsonrpc(&mut stdin, &mut stdout, &call);
    assert!(
        resp.get("result").is_some(),
        "ecotokens_run with cwd should return result, got: {resp}"
    );
    let content = resp["result"]["content"][0]["text"].as_str().unwrap_or("");
    assert!(
        content.contains("hello.txt"),
        "output should be produced from provided cwd, got: {content}"
    );

    drop(stdin);
    let _ = child.kill();
}

#[test]
fn mcp_unknown_tool_returns_error() {
    let (_src, idx) = setup_indexed_fixture();
    let mut child = spawn_mcp(&idx);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let _ = send_jsonrpc(&mut stdin, &mut stdout, &init_request());
    let notif = serde_json::to_string(&initialized_notification()).unwrap();
    writeln!(stdin, "{notif}").unwrap();
    stdin.flush().unwrap();

    let call = serde_json::json!({
        "jsonrpc": "2.0", "id": 2,
        "method": "tools/call",
        "params": {
            "name": "nonexistent_tool",
            "arguments": {}
        }
    });
    let resp = send_jsonrpc(&mut stdin, &mut stdout, &call);
    // Unknown tool should return error or result with isError=true
    let has_error =
        resp.get("error").is_some() || resp["result"]["isError"].as_bool().unwrap_or(false);
    assert!(has_error, "unknown tool should return error, got: {resp}");

    drop(stdin);
    let _ = child.kill();
}
