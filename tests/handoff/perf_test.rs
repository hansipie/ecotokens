#[path = "common.rs"]
mod common;
use common::*;

use std::time::Instant;

use ecotokens::handoff::cli::write;
use ecotokens::handoff::store::write_session_record;
use ecotokens::handoff::transcript::extract;
use ecotokens::handoff::SessionRecord;
use serde_json::json;
use tempfile::TempDir;

const CWD: &str = "/work";

/// About 5 MB of transcript in the shape of a real session: edits, reads and
/// shell calls, each with a result of 1.5 KB, one Bash call in seven failing.
fn big_transcript(dir: &std::path::Path) -> std::path::PathBuf {
    let envelope = |kind: &str, message: serde_json::Value| {
        json!({
            "type": kind, "sessionId": "big", "cwd": CWD, "gitBranch": "main",
            "timestamp": "2026-10-04T10:00:00.000Z", "isSidechain": false,
            "message": message,
        })
        .to_string()
    };
    let mut text = String::new();
    let mut i = 0usize;
    while text.len() < 5 * 1024 * 1024 {
        i += 1;
        let id = format!("toolu_{i}");
        let (name, input) = match i % 4 {
            0 => (
                "Edit",
                json!({"file_path": format!("{CWD}/src/file_{}.rs", i % 60),
                       "old_string": "a".repeat(200), "new_string": "b".repeat(200)}),
            ),
            1 => (
                "Read",
                json!({"file_path": format!("{CWD}/src/file_{}.rs", i % 90)}),
            ),
            2 => (
                "Bash",
                json!({"command": format!("cargo test --lib module_{}", i % 40)}),
            ),
            _ => (
                "Bash",
                json!({"command": format!("grep -rn pattern_{i} src")}),
            ),
        };
        let fails = i % 7 == 0 && name == "Bash";
        let body = format!(
            "{}{}",
            if fails { "Exit code 101\n" } else { "" },
            "x".repeat(1500)
        );
        let mut result = json!({"type": "tool_result", "tool_use_id": id, "content": body});
        if fails {
            result["is_error"] = json!(true);
        }
        text.push_str(&envelope(
            "assistant",
            json!({"role": "assistant", "content": [
                {"type": "tool_use", "id": id, "name": name, "input": input}
            ]}),
        ));
        text.push('\n');
        text.push_str(&envelope(
            "user",
            json!({"role": "user", "content": [result]}),
        ));
        text.push('\n');
    }
    let path = dir.join("big.jsonl");
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn extraction_and_write_stay_fast_on_a_five_megabyte_transcript() {
    let dir = TempDir::new().unwrap();
    let transcript = big_transcript(dir.path());
    let size = std::fs::metadata(&transcript).unwrap().len();
    assert!(size >= 5 * 1024 * 1024, "{size}");

    let started = Instant::now();
    let facts = extract(&transcript, CWD);
    let extract_time = started.elapsed();
    assert!(facts.found);
    assert_eq!(facts.key_files.len(), 10);
    assert!(!facts.failures.is_empty() && facts.failures.len() <= 8);
    assert_eq!(facts.skipped_lines, 0);

    let handoffs = dir.path().join("handoff");
    write_session_record(
        &handoffs,
        &SessionRecord {
            session_id: "big".into(),
            transcript_path: transcript.to_string_lossy().into_owned(),
            cwd: CWD.into(),
            source: "startup".into(),
            recorded: fixed_now(),
        },
    )
    .unwrap();
    let started = Instant::now();
    let out = write(&handoffs, &settings(), "big", Some(CWD), true, fixed_now());
    let write_time = started.elapsed();
    assert_eq!(out.code, 0, "{}", out.err);

    // Measured with `cargo test --release --test handoff_perf_test -- --nocapture`;
    // the bound here is deliberately generous so that a debug build never flakes.
    println!(
        "5 MB transcript: extract {} ms, full `handoff write` {} ms",
        extract_time.as_millis(),
        write_time.as_millis()
    );
    assert!(write_time.as_secs() < 10, "{write_time:?}");
}
