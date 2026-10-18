//! `ecotokens hook-prompt`: the Claude Code `UserPromptSubmit` hook.
//!
//! It must never slow down or block a message. With the router off, no key,
//! a slash command, a recent Jev failure, or any error, it prints nothing
//! and exits 0, which leaves the message untouched.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::location::{self, GateResult};
use super::policy::Policy;
use super::{apply_availability, delegation_context, route_with, skip_reason, Decision, Routing};
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
    /// The session's working directory: Jev calls are recorded in its project.
    #[serde(default)]
    cwd: Option<PathBuf>,
    /// Opaque session id, kept to match the transcript later.
    #[serde(default)]
    session_id: Option<String>,
}

/// What the hook knows about where it runs, besides the prompt itself.
pub struct HookEnv {
    /// The session's working directory.
    pub cwd: Option<PathBuf>,
    pub session_id: Option<String>,
    /// Where the helper agents live; `None` when the home folder is unknown.
    pub agents_dir: Option<PathBuf>,
    pub policy: Policy,
    /// The settings file could be read, so the location lists can be
    /// trusted; when false the gate treats the folder as private.
    pub lists_trusted: bool,
}

impl HookEnv {
    /// Environment with the default policy and no session id.
    // Used by the library crate and tests; the binary builds the struct itself.
    #[allow(dead_code)]
    pub fn new(cwd: Option<PathBuf>, agents_dir: PathBuf) -> Self {
        HookEnv {
            cwd,
            session_id: None,
            agents_dir: Some(agents_dir),
            policy: Policy::default_policy(),
            lists_trusted: true,
        }
    }
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
    env: &HookEnv,
) -> (Option<Routing>, Option<String>) {
    if !settings.router_enabled {
        return (None, None);
    }
    // The gate comes first: a private or not-allowed folder never reaches the
    // judge, the down marker or the statistics, and its row keeps only the kind.
    match location::gate(env.cwd.as_deref(), settings, env.lists_trusted) {
        GateResult::Private(_) => {
            return (Some(Routing::without_jev(Decision::Private)), None);
        }
        GateResult::NotAllowed => {
            return (Some(Routing::without_jev(Decision::NotAllowed)), None);
        }
        GateResult::Open => {}
    }
    let Some(judge) = judge else {
        return (None, None);
    };
    if skip_reason(prompt).is_some() {
        return (
            Some(finish(Routing::without_jev(Decision::Skipped), env)),
            None,
        );
    }
    if is_jev_down(marker) {
        return (
            Some(finish(Routing::without_jev(Decision::JevDown), env)),
            None,
        );
    }
    let timeout = Duration::from_millis(settings.router_timeout_ms);
    let mut routing = route_with(prompt, settings, judge, timeout, &env.policy);
    if routing.service_failure {
        mark_jev_down(marker);
    }
    apply_availability(&mut routing, env.agents_dir.as_deref());
    let routing = finish(routing, env);
    let output = render_output(&routing);
    (Some(routing), output)
}

/// Stamps what every recorded decision carries.
fn finish(mut routing: Routing, env: &HookEnv) -> Routing {
    routing.policy_version = Some(env.policy.version().to_string());
    routing.session_id = env.session_id.clone();
    routing
}

/// Everything [`run_prompt`] needs from the outside world, so the whole hook
/// can be exercised without touching the real configuration or the network.
pub struct PromptRuntime<'a> {
    pub settings: &'a Settings,
    pub judge: Option<&'a dyn Judge>,
    /// `router.db`; `None` when the configuration directory is unknown.
    pub db: Option<&'a Path>,
    pub marker: &'a Path,
    pub agents_dir: Option<PathBuf>,
    pub policy: Policy,
    /// The settings file could be read (see [`HookEnv::lists_trusted`]).
    pub lists_trusted: bool,
    /// Bind the Jev statistics to the session's project. Process-wide, so
    /// only the real hook does it, and only once the gate is open.
    pub bind_project: bool,
}

/// The whole hook on its raw standard input: the text to print, if any.
/// Every failure (oversized or malformed input, no judge, a statistics
/// database that cannot be written) leaves the message untouched.
pub fn run_prompt(raw: &str, rt: &PromptRuntime) -> Option<String> {
    if !rt.settings.router_enabled || raw.len() as u64 > crate::hook::MAX_STDIN_BYTES as u64 {
        return None;
    }
    let input = serde_json::from_str::<PromptHookInput>(raw).ok()?;
    // Before anything else, including the project bookkeeping below: a
    // private folder must leave no trace but a bare row.
    let gate_result = location::gate(input.cwd.as_deref(), rt.settings, rt.lists_trusted);
    if gate_result != GateResult::Open {
        let decision = match gate_result {
            GateResult::NotAllowed => Decision::NotAllowed,
            _ => Decision::Private,
        };
        if let Some(db) = rt.db {
            let _ = super::stats::record(db, &Routing::without_jev(decision));
        }
        return None;
    }
    if rt.bind_project {
        if let Some(cwd) = &input.cwd {
            crate::jev::stats::set_project_dir(cwd);
        }
    }
    let env = HookEnv {
        cwd: input.cwd.clone(),
        session_id: input.session_id.clone(),
        agents_dir: rt.agents_dir.clone(),
        policy: rt.policy.clone(),
        lists_trusted: rt.lists_trusted,
    };
    let (routing, output) = process(&input.prompt, rt.settings, rt.judge, rt.marker, &env);
    if let (Some(routing), Some(db)) = (routing, rt.db) {
        let _ = super::stats::record(db, &routing);
        if !matches!(routing.decision, Decision::Skipped | Decision::JevDown) {
            let ok = routing.error.is_none() && !routing.service_failure;
            let _ = crate::jev::stats::record(
                db,
                &crate::jev::stats::CallRecord {
                    purpose: crate::jev::stats::Purpose::Router,
                    ok,
                    error_kind: (!ok).then_some("error"),
                    http_status: None,
                    latency_ms: routing.latency_ms,
                    usage: routing.usage,
                    agent: routing.target_label(),
                },
            );
        }
    }
    output
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
    let (Some(marker), Some(agents_dir)) =
        (jev_down_marker_path(), super::agents::default_agents_dir())
    else {
        return;
    };
    let judge = super::judge_for_router(&settings, false);
    let db = super::stats::router_db_path();
    let rt = PromptRuntime {
        settings: &settings,
        judge: judge.as_deref(),
        db: db.as_deref(),
        marker: &marker,
        agents_dir: Some(agents_dir),
        policy: Policy::load(),
        lists_trusted: Settings::config_path().map_or(true, |p| Settings::config_file_trusted(&p)),
        bind_project: true,
    };
    if let Some(output) = run_prompt(&buf, &rt) {
        print!("{output}");
    }
}
