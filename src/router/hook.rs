//! `ecotokens hook-prompt`: the Claude Code `UserPromptSubmit` hook.
//!
//! It must never slow down or block a message. With the router off, no key,
//! a slash command, a recent Jev failure, or any error, it prints nothing
//! and exits 0, which leaves the message untouched.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::{delegation_context, route, skip_reason, Decision, Routing};
use crate::config::Settings;
use crate::jev::Judge;

/// After a service failure, Jev is not asked again for this long. Each hook
/// call is a new process, so the in-process breaker alone would pay the
/// timeout on every message while TypeSafe is down.
pub const JEV_DOWN_TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Deserialize)]
struct PromptHookInput {
    #[serde(default)]
    prompt: String,
}

#[derive(Debug, Serialize)]
struct PromptHookOutput {
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: PromptHookSpecificOutput,
}

#[derive(Debug, Serialize)]
struct PromptHookSpecificOutput {
    #[serde(rename = "hookEventName")]
    hook_event_name: &'static str,
    #[serde(rename = "additionalContext")]
    additional_context: String,
}

pub fn jev_down_marker_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ecotokens").join("router_jev_down"))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Whether a failure was recorded less than [`JEV_DOWN_TTL`] ago.
pub fn is_jev_down(marker: &Path) -> bool {
    std::fs::read_to_string(marker)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .is_some_and(|t| now_secs().saturating_sub(t) < JEV_DOWN_TTL.as_secs())
}

pub fn mark_jev_down(marker: &Path) {
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(marker, now_secs().to_string());
}

/// Hook output for a routing, or `None` when nothing should be injected.
pub fn render_output(routing: &Routing) -> Option<String> {
    let context = delegation_context(routing)?;
    serde_json::to_string(&PromptHookOutput {
        hook_specific_output: PromptHookSpecificOutput {
            hook_event_name: "UserPromptSubmit",
            additional_context: context,
        },
    })
    .ok()
}

/// Testable core: the routing to record (if any) and the text to print.
pub fn process(
    prompt: &str,
    settings: &Settings,
    judge: Option<&dyn Judge>,
    marker: &Path,
) -> (Option<Routing>, Option<String>) {
    if !settings.router_enabled {
        return (None, None);
    }
    let Some(judge) = judge else {
        return (None, None);
    };
    if skip_reason(prompt).is_some() {
        return (Some(Routing::without_jev(Decision::Skipped)), None);
    }
    if is_jev_down(marker) {
        return (Some(Routing::without_jev(Decision::JevDown)), None);
    }
    let timeout = Duration::from_millis(settings.router_timeout_ms);
    let routing = route(prompt, settings, judge, timeout);
    if routing.service_failure {
        mark_jev_down(marker);
    }
    let output = render_output(&routing);
    (Some(routing), output)
}

/// Entry point of `ecotokens hook-prompt`.
pub fn handle_prompt() {
    let settings = Settings::load();
    if !settings.router_enabled {
        return;
    }
    let mut buf = String::new();
    let limit = crate::hook::MAX_STDIN_BYTES as u64;
    if std::io::stdin()
        .take(limit + 1)
        .read_to_string(&mut buf)
        .is_err()
        || buf.len() as u64 > limit
    {
        return;
    }
    let Ok(input) = serde_json::from_str::<PromptHookInput>(&buf) else {
        return;
    };
    let Some(marker) = jev_down_marker_path() else {
        return;
    };
    let judge = super::judge_for_router(&settings, false);
    let (routing, output) = process(&input.prompt, &settings, judge.as_deref(), &marker);
    if let (Some(routing), Some(path)) = (routing, super::stats::router_db_path()) {
        let _ = super::stats::record(&path, &routing);
    }
    if let Some(output) = output {
        print!("{output}");
    }
}
