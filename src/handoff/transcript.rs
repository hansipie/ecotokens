//! Deterministic extraction of facts from a Claude Code session transcript.
//!
//! The transcript is an internal Claude Code format (JSONL, one object per
//! line), so everything here is tolerant: a line that does not have the
//! expected shape is skipped and counted, never an error. The rules are
//! documented in `specs/011-session-handoff/research.md` (R3).

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::BufRead;
use std::path::Path;

use serde_json::Value;

use super::{FailedAttempt, KeyFile, Kind};

const MAX_KEY_FILES: usize = 10;
const MAX_FAILURES: usize = 8;
const MAX_FAILURE_CHARS: usize = 160;

/// Commands whose failure says nothing about the task: the model was looking
/// for something.
const EXPLORATION: [&str; 17] = [
    "grep", "rg", "find", "ls", "cat", "test", "[", "command", "diff", "head", "tail", "wc",
    "stat", "which", "type", "tree", "echo",
];

/// Everything extracted in one pass over a transcript.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TranscriptFacts {
    /// The transcript could be opened.
    pub found: bool,
    pub branch: Option<String>,
    /// Modified files first (latest first), then the most read ones, at most 10.
    pub key_files: Vec<KeyFile>,
    /// Mechanical failed attempts, newest first, at most 8.
    pub failures: Vec<FailedAttempt>,
    /// Non-blank lines that could not be used.
    pub skipped_lines: usize,
}

enum ToolUse {
    Bash(String),
    Edit { file: String, old: u64, new: u64 },
    Write(String),
    Read(String),
}

fn hash(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn relative(path: &str, cwd: &str) -> String {
    let cwd = cwd.trim_end_matches('/');
    path.strip_prefix(cwd)
        .and_then(|rest| rest.strip_prefix('/'))
        .filter(|rest| !cwd.is_empty() && !rest.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// Cuts `s` to `max` characters, ending with `…` when it was shortened.
fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn content_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn exit_code(text: &str) -> Option<i64> {
    text.strip_prefix("Exit code ")?
        .split(|c: char| !c.is_ascii_digit() && c != '-')
        .next()?
        .parse()
        .ok()
}

/// The command with leading `cd <dir> &&` hops removed and whitespace collapsed.
fn normalize(command: &str) -> String {
    let mut rest = command.trim();
    while let Some(tail) = rest.strip_prefix("cd ") {
        match tail.split_once("&&") {
            Some((_, after)) => rest = after.trim_start(),
            None => break,
        }
    }
    rest.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The part of a command line after its last `&&`, `||`, `;` or `|` outside
/// quotes. The exit status of a chain is the status of that last command, so it
/// is the one to judge. `2>&1` and `&>` are redirections, not separators.
fn last_segment(command: &str) -> &str {
    let chars: Vec<(usize, char)> = command.char_indices().collect();
    let mut quote: Option<char> = None;
    let mut start = 0;
    let mut i = 0;
    while let Some(&(_, c)) = chars.get(i) {
        match quote {
            Some(q) if c == q => quote = None,
            Some('"') if c == '\\' => i += 1,
            Some(_) => {}
            None => match c {
                '\'' | '"' => quote = Some(c),
                '\\' => i += 1,
                '&' | '|' | ';' => {
                    let redirect = c == '&'
                        && (i > 0 && chars[i - 1].1 == '>'
                            || chars.get(i + 1).is_some_and(|n| n.1 == '>'));
                    if !redirect {
                        let mut j = i;
                        while chars
                            .get(j + 1)
                            .is_some_and(|n| matches!(n.1, '&' | '|' | ';'))
                        {
                            j += 1;
                        }
                        start = chars.get(j + 1).map_or(command.len(), |n| n.0);
                        i = j;
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    command.get(start..).unwrap_or_default().trim()
}

fn is_exploration(command: &str) -> bool {
    let first = command.split_whitespace().next().unwrap_or("");
    EXPLORATION.contains(&first) || (command.starts_with("git diff") && command.contains("--quiet"))
}

fn is_test_or_build(command: &str) -> bool {
    let t: Vec<&str> = command.split_whitespace().collect();
    let get = |i: usize| t.get(i).copied().unwrap_or("");
    t.contains(&"pytest")
        || match get(0) {
            "cargo" => matches!(
                get(1),
                "test" | "build" | "clippy" | "check" | "nextest" | "bench"
            ),
            "npm" | "pnpm" | "yarn" => {
                get(1) == "test" || (get(1) == "run" && matches!(get(2), "test" | "build" | "lint"))
            }
            "go" => matches!(get(1), "test" | "build" | "vet"),
            "make" | "mvn" | "gradle" | "./gradlew" | "tox" => true,
            _ => false,
        }
}

#[derive(Default)]
struct Collector {
    pending: HashMap<String, ToolUse>,
    event: usize,
    modified: HashMap<String, usize>,
    reads: HashMap<String, (u32, usize)>,
    edits: HashMap<String, Vec<(u64, u64)>>,
    failures: Vec<FailedAttempt>,
}

impl Collector {
    fn tick(&mut self) -> usize {
        self.event += 1;
        self.event
    }

    fn push_failure(&mut self, kind: Kind, text: String) {
        self.failures
            .retain(|f| !(f.kind == kind && f.text == text));
        self.failures.push(FailedAttempt { kind, text });
    }

    fn tool_use(&mut self, item: &Value) {
        let (Some(id), Some(name)) = (
            item.get("id").and_then(Value::as_str),
            item.get("name").and_then(Value::as_str),
        ) else {
            return;
        };
        let input = item.get("input");
        let str_field = |key: &str| {
            input
                .and_then(|i| i.get(key))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let tool = match name {
            "Bash" => str_field("command").map(ToolUse::Bash),
            "Edit" => str_field("file_path").map(|file| ToolUse::Edit {
                file,
                old: hash(&str_field("old_string").unwrap_or_default()),
                new: hash(&str_field("new_string").unwrap_or_default()),
            }),
            "Write" => str_field("file_path").map(ToolUse::Write),
            "NotebookEdit" => str_field("notebook_path").map(ToolUse::Write),
            "Read" => str_field("file_path").map(ToolUse::Read),
            _ => None,
        };
        if let Some(tool) = tool {
            self.pending.insert(id.to_string(), tool);
        }
    }

    fn tool_result(&mut self, item: &Value, cwd: &str) {
        let Some(tool) = item
            .get("tool_use_id")
            .and_then(Value::as_str)
            .and_then(|id| self.pending.remove(id))
        else {
            return;
        };
        let is_error = item.get("is_error").and_then(Value::as_bool) == Some(true);
        let at = self.tick();
        match tool {
            ToolUse::Bash(command) if is_error => {
                let text = item.get("content").map(content_text).unwrap_or_default();
                let Some(code) = exit_code(&text) else { return };
                let command = normalize(&command);
                let last = last_segment(&command);
                if command.is_empty() || is_exploration(last) {
                    return;
                }
                let suffix = format!(": exit {code}");
                let budget = MAX_FAILURE_CHARS.saturating_sub(suffix.chars().count());
                let text = format!("{}{suffix}", shorten(&command, budget));
                let kind = if is_test_or_build(last) {
                    Kind::TestOrBuild
                } else {
                    Kind::Command
                };
                self.push_failure(kind, text);
            }
            ToolUse::Edit { file, old, new } if !is_error => {
                let path = relative(&file, cwd);
                let history = self.edits.entry(file).or_default();
                let verdict = history.iter().rev().find_map(|(prev_old, prev_new)| {
                    if new == *prev_old {
                        Some(Kind::EditReverted)
                    } else if old == *prev_new {
                        Some(Kind::EditRewritten)
                    } else {
                        None
                    }
                });
                history.push((old, new));
                self.modified.insert(path.clone(), at);
                if let Some(kind) = verdict {
                    self.push_failure(kind, path);
                }
            }
            ToolUse::Write(file) if !is_error => {
                self.modified.insert(relative(&file, cwd), at);
            }
            ToolUse::Read(file) if !is_error => {
                let entry = self.reads.entry(relative(&file, cwd)).or_insert((0, 0));
                entry.0 += 1;
                entry.1 = at;
            }
            _ => {}
        }
    }

    fn key_files(&self) -> Vec<KeyFile> {
        let mut modified: Vec<(&String, &usize)> = self.modified.iter().collect();
        modified.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        let mut read: Vec<(&String, &(u32, usize))> = self
            .reads
            .iter()
            .filter(|(path, _)| !self.modified.contains_key(*path))
            .collect();
        read.sort_by(|a, b| {
            b.1 .0
                .cmp(&a.1 .0)
                .then_with(|| b.1 .1.cmp(&a.1 .1))
                .then_with(|| a.0.cmp(b.0))
        });
        modified
            .into_iter()
            .map(|(path, _)| KeyFile::modified(path))
            .chain(
                read.into_iter()
                    .map(|(path, (n, _))| KeyFile::read(path, *n)),
            )
            .take(MAX_KEY_FILES)
            .collect()
    }
}

/// Reads the transcript at `path` and extracts what a handoff needs. Paths are
/// shown relative to `cwd` when they are inside it.
pub fn extract(path: &Path, cwd: &str) -> TranscriptFacts {
    let Ok(file) = std::fs::File::open(path) else {
        return TranscriptFacts::default();
    };
    let mut facts = TranscriptFacts {
        found: true,
        ..TranscriptFacts::default()
    };
    let mut c = Collector::default();

    for line in std::io::BufReader::new(file).split(b'\n') {
        let Ok(bytes) = line else { break };
        let line = String::from_utf8_lossy(&bytes);
        if line.trim().is_empty() {
            continue;
        }
        let Some(obj) = serde_json::from_str::<Value>(&line)
            .ok()
            .filter(Value::is_object)
        else {
            facts.skipped_lines += 1;
            continue;
        };
        if obj.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if let Some(branch) = obj
            .get("gitBranch")
            .and_then(Value::as_str)
            .filter(|b| !b.is_empty())
        {
            facts.branch = Some(branch.to_string());
        }
        let Some(items) = obj
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for item in items {
            match item.get("type").and_then(Value::as_str) {
                Some("tool_use") => c.tool_use(item),
                Some("tool_result") => c.tool_result(item, cwd),
                _ => {}
            }
        }
    }

    facts.key_files = c.key_files();
    c.failures.reverse();
    c.failures.truncate(MAX_FAILURES);
    facts.failures = c.failures;
    facts
}
