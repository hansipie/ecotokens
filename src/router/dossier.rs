//! The dossier a helper hands over when it stops instead of looping: goal,
//! constraints, the current diff, commands run, exact errors and discarded
//! hypotheses, written as a handoff file so the next agent (or a bigger
//! model) starts from facts instead of from the previous agent's account.
//!
//! The diff is collected here, from the repository, never taken from the
//! input; everything written or printed is masked.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::handoff::format::{mask_text, render, trim_to_limit};
use crate::handoff::{
    store, FailedAttempt, Handoff, Kind, Status, MAX_CHARS_CEILING, NOT_PROVIDED,
};

/// Longest diff kept, in characters.
pub const MAX_DIFF_CHARS: usize = 3_000;
/// Most git output read before the rest is dropped, in bytes.
const MAX_DIFF_BYTES: usize = 256 * 1024;
const MAX_GOAL_CHARS: usize = 1_500;
const MAX_ITEMS: usize = 12;
const MAX_ITEM_CHARS: usize = 500;
/// Standard input larger than this is not a dossier.
const MAX_INPUT_BYTES: u64 = 1024 * 1024;

/// What the helper reports. Every key is optional.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DossierInput {
    pub goal: Option<String>,
    pub constraints: Vec<String>,
    pub commands: Vec<String>,
    pub errors: Vec<String>,
    pub discarded: Vec<String>,
}

fn string_list(map: &serde_json::Map<String, Value>, key: &str) -> Result<Vec<String>, String> {
    let Some(value) = map.get(key) else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or_else(|| format!("`{key}` must be an array of strings"))?;
    items
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("`{key}` must be an array of strings"))
        })
        .collect()
}

/// Parses the JSON object read on standard input. Unknown keys (such as a
/// `diff` someone tried to pass) are ignored.
pub fn parse_input(text: &str) -> Result<DossierInput, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("input is not valid JSON: {e}"))?;
    let Value::Object(map) = value else {
        return Err("input must be a JSON object".into());
    };
    let goal = match map.get("goal") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return Err("`goal` must be a string".into()),
    };
    Ok(DossierInput {
        goal,
        constraints: string_list(&map, "constraints")?,
        commands: string_list(&map, "commands")?,
        errors: string_list(&map, "errors")?,
        discarded: string_list(&map, "discarded")?,
    })
}

/// Keeps `text` from opening a section of its own in the file.
fn neutralize(text: &str) -> String {
    text.lines()
        .map(|l| {
            if l.starts_with('#') {
                format!(" {l}")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}…")
}

fn bullets(items: &[String]) -> Option<String> {
    let lines: Vec<String> = items
        .iter()
        .filter(|i| !i.trim().is_empty())
        .take(MAX_ITEMS)
        .map(|i| {
            format!(
                "- {}",
                neutralize(&clip(i.trim(), MAX_ITEM_CHARS)).replace('\n', "\n  ")
            )
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn git_output(cwd: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let mut child = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = Vec::new();
    child
        .stdout
        .take()?
        .take(MAX_DIFF_BYTES as u64 + 1)
        .read_to_end(&mut out)
        .ok()?;
    if out.len() > MAX_DIFF_BYTES {
        // More than we will ever keep: stop git instead of waiting for it.
        let _ = child.kill();
    }
    let status = child.wait().ok()?;
    (status.success() || out.len() > MAX_DIFF_BYTES).then_some(out)
}

/// The text of the "Current diff" section: the uncommitted changes of the
/// repository around `cwd`, bounded, or a sentence saying why there is none.
pub fn collect_diff(cwd: &Path) -> String {
    let inside = git_output(cwd, &["rev-parse", "--is-inside-work-tree"])
        .is_some_and(|o| String::from_utf8_lossy(&o).trim() == "true");
    if !inside {
        return "not a git repository: no diff was collected".to_string();
    }
    let raw = git_output(cwd, &["diff", "HEAD", "--no-color", "--no-ext-diff"])
        // A repository without a first commit has no HEAD.
        .or_else(|| git_output(cwd, &["diff", "--no-color", "--no-ext-diff"]));
    let Some(raw) = raw else {
        return "the diff could not be collected".to_string();
    };
    let text = String::from_utf8_lossy(&raw);
    if text.trim().is_empty() {
        return "no uncommitted changes".to_string();
    }
    let mut kept = String::new();
    let mut truncated = text.len() >= MAX_DIFF_BYTES;
    for line in text.lines() {
        if kept.chars().count() + line.chars().count() + 1 > MAX_DIFF_CHARS {
            truncated = true;
            break;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    if kept.is_empty() {
        // A single line longer than the whole budget.
        kept = clip(text.lines().next().unwrap_or_default(), MAX_DIFF_CHARS);
        kept.push('\n');
        truncated = true;
    }
    let mut section = format!("```diff\n{kept}```");
    if truncated {
        section.push_str("\n(diff truncated: run `git diff HEAD` in the repository for the rest)");
    }
    section
}

pub fn branch_of(cwd: &Path) -> Option<String> {
    let out = git_output(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let name = String::from_utf8_lossy(&out).trim().to_string();
    (!name.is_empty() && name != "HEAD").then_some(name)
}

/// A name that two dossiers written in the same second do not share.
fn session_id(now: DateTime<Utc>) -> String {
    format!(
        "router-dossier-{}-{:08x}{:x}",
        now.format("%Y%m%dT%H%M%S"),
        now.timestamp_subsec_nanos(),
        std::process::id()
    )
}

/// Builds the handoff for a dossier. It is stored as already consumed: it is
/// read on purpose (`/handoff-load`), never injected by a later SessionStart,
/// and the handoff retention rules clean it up like any consumed handoff.
pub fn build(input: &DossierInput, diff: String, cwd: &str, now: DateTime<Utc>) -> Handoff {
    // The id keeps the sub-second part; the file header only has whole seconds.
    let id = session_id(now);
    let now = crate::handoff::whole_seconds(now);
    let mut h = Handoff::new(&id, cwd, now);
    h.status = Status::Consumed;
    h.consumed = Some(now);
    h.header_extra
        .push(("kind".to_string(), "router-dossier".to_string()));
    h.objective = input
        .goal
        .as_deref()
        .map(|g| neutralize(&clip(g.trim(), MAX_GOAL_CHARS)))
        .filter(|g| !g.is_empty());
    h.problem = bullets(&input.errors);
    h.failed_attempts = input
        .discarded
        .iter()
        .filter(|d| !d.trim().is_empty())
        .take(MAX_ITEMS)
        .map(|d| {
            FailedAttempt::new(
                Kind::Hypothesis,
                &neutralize(&clip(d.trim(), MAX_ITEM_CHARS)).replace('\n', " "),
            )
        })
        .collect();
    let section = |items: &[String]| bullets(items).unwrap_or_else(|| NOT_PROVIDED.to_string());
    h.extra_sections = vec![
        ("Constraints".to_string(), section(&input.constraints)),
        ("Current diff".to_string(), diff),
        ("Commands run".to_string(), section(&input.commands)),
    ];
    h
}

/// A written dossier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub path: PathBuf,
    /// The masked Markdown, exactly as stored.
    pub markdown: String,
    pub chars: usize,
}

/// Writes the dossier into the handoff directory `dir`.
pub fn write(dir: &Path, mut handoff: Handoff) -> io::Result<Written> {
    let _ = trim_to_limit(&mut handoff, MAX_CHARS_CEILING);
    // The diff is the one part the trimmer does not touch: shrink it last.
    while render(&handoff).chars().count() > MAX_CHARS_CEILING {
        let Some(pos) = handoff
            .extra_sections
            .iter()
            .position(|(name, body)| name == "Current diff" && body.chars().count() > 200)
        else {
            break;
        };
        let body = &mut handoff.extra_sections[pos].1;
        let keep = body.chars().count() / 2;
        *body = format!(
            "{}\n… (diff cut to fit)",
            body.chars().take(keep).collect::<String>()
        );
    }
    let path = store::write_handoff(dir, &handoff)?;
    let markdown = mask_text(&render(&handoff));
    let chars = markdown.chars().count();
    Ok(Written {
        path,
        markdown,
        chars,
    })
}

/// Reads the dossier JSON from standard input, bounded.
pub fn read_stdin() -> io::Result<String> {
    let mut buf = String::new();
    io::stdin()
        .take(MAX_INPUT_BYTES + 1)
        .read_to_string(&mut buf)?;
    if buf.len() as u64 > MAX_INPUT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "input is larger than 1 MiB",
        ));
    }
    Ok(buf)
}
