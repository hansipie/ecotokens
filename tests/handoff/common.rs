#![allow(dead_code)]
//! Shared helpers for the handoff tests: fixed clock, handoff fixtures and a
//! builder for Claude Code transcript JSONL files. Everything works on temp
//! directories passed in by the test, never on the real configuration.

use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use ecotokens::config::Settings;
use ecotokens::handoff::{FailedAttempt, Handoff, KeyFile, Kind};
use serde_json::json;

/// 2026-10-04T18:20:11Z, the clock used by every deterministic test.
pub fn fixed_now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 4, 18, 20, 11).unwrap()
}

pub fn hours_before(now: DateTime<Utc>, hours: i64) -> DateTime<Utc> {
    now - chrono::Duration::hours(hours)
}

/// Settings with the feature switched on.
pub fn settings() -> Settings {
    Settings {
        handoff_enabled: true,
        ..Settings::default()
    }
}

/// A fully filled handoff, created at `fixed_now()`.
pub fn sample_handoff(session: &str, cwd: &str) -> Handoff {
    let mut h = Handoff::new(session, cwd, fixed_now());
    h.branch = Some("feat/x".into());
    h.objective = Some("Add a manual handoff command.".into());
    h.problem =
        Some("A hook cannot supply a summary.\nThe state must be saved by a command.".into());
    h.key_files = vec![
        KeyFile::modified("src/install.rs"),
        KeyFile::read("src/router/hook.rs", 3),
    ];
    h.failed_attempts = vec![
        FailedAttempt::new(Kind::TestOrBuild, "cargo test: exit 101"),
        FailedAttempt::new(Kind::Command, "ecotokens router status: exit 1"),
        FailedAttempt::new(
            Kind::Hypothesis,
            "A single matcher cannot cover startup and clear.",
        ),
    ];
    h.next_steps = Some("Write the failing tests for the store.".into());
    h
}

/// Builds a transcript in the shape observed on real Claude Code sessions:
/// one JSON object per line, `assistant` lines carrying `tool_use` items and
/// `user` lines carrying the matching `tool_result`.
pub struct TranscriptBuilder {
    session: String,
    cwd: String,
    branch: Option<String>,
    lines: Vec<String>,
    next_id: usize,
    sidechain: bool,
}

impl TranscriptBuilder {
    pub fn new(session: &str, cwd: &str, branch: Option<&str>) -> Self {
        TranscriptBuilder {
            session: session.into(),
            cwd: cwd.into(),
            branch: branch.map(str::to_string),
            lines: Vec::new(),
            next_id: 1,
            sidechain: false,
        }
    }

    /// Lines added afterwards belong to a subagent (`isSidechain: true`).
    pub fn sidechain(mut self, on: bool) -> Self {
        self.sidechain = on;
        self
    }

    fn envelope(&self, kind: &str, message: serde_json::Value) -> String {
        let mut v = json!({
            "type": kind,
            "sessionId": self.session,
            "cwd": self.cwd,
            "timestamp": "2026-10-04T18:00:00.000Z",
            "isSidechain": self.sidechain,
            "message": message,
        });
        if let Some(b) = &self.branch {
            v["gitBranch"] = json!(b);
        }
        v.to_string()
    }

    fn tool_call(
        mut self,
        name: &str,
        input: serde_json::Value,
        is_error: Option<bool>,
        content: &str,
    ) -> Self {
        let id = format!("toolu_{}", self.next_id);
        self.next_id += 1;
        let call = self.envelope(
            "assistant",
            json!({"role": "assistant", "content": [
                {"type": "tool_use", "id": id, "name": name, "input": input}
            ]}),
        );
        let mut result = json!({"type": "tool_result", "tool_use_id": id, "content": content});
        if let Some(e) = is_error {
            result["is_error"] = json!(e);
        }
        let res = self.envelope("user", json!({"role": "user", "content": [result]}));
        self.lines.push(call);
        self.lines.push(res);
        self
    }

    pub fn edit(self, file: &str, old: &str, new: &str) -> Self {
        self.tool_call(
            "Edit",
            json!({"file_path": file, "old_string": old, "new_string": new}),
            Some(false),
            "The file has been updated.",
        )
    }

    /// An `Edit` that Claude Code rejected (tool misuse, not a failed approach).
    pub fn edit_rejected(self, file: &str, old: &str, new: &str) -> Self {
        self.tool_call(
            "Edit",
            json!({"file_path": file, "old_string": old, "new_string": new}),
            Some(true),
            "<tool_use_error>String to replace not found in file.</tool_use_error>",
        )
    }

    pub fn write(self, file: &str) -> Self {
        self.tool_call(
            "Write",
            json!({"file_path": file, "content": "x"}),
            Some(false),
            "File created.",
        )
    }

    pub fn read(self, file: &str) -> Self {
        self.tool_call("Read", json!({"file_path": file}), None, "contents")
    }

    pub fn bash_ok(self, command: &str) -> Self {
        self.tool_call(
            "Bash",
            json!({"command": command, "description": "run"}),
            Some(false),
            "ok",
        )
    }

    pub fn bash_fail(self, command: &str, code: i32) -> Self {
        self.tool_call(
            "Bash",
            json!({"command": command, "description": "run"}),
            Some(true),
            &format!("Exit code {code}\nsomething went wrong"),
        )
    }

    /// A line that is not valid JSON, to exercise tolerance.
    pub fn garbage(mut self, line: &str) -> Self {
        self.lines.push(line.to_string());
        self
    }

    pub fn build(self, dir: &Path) -> PathBuf {
        let path = dir.join(format!("{}.jsonl", self.session));
        std::fs::write(&path, self.lines.join("\n") + "\n").unwrap();
        path
    }
}

/// Minimal hook input for `source` at `cwd`.
pub fn hook_input(session: &str, cwd: &str, source: &str, transcript: &str) -> String {
    json!({
        "session_id": session,
        "cwd": cwd,
        "source": source,
        "transcript_path": transcript,
        "hook_event_name": "SessionStart",
    })
    .to_string()
}
