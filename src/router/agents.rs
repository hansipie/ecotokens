//! Helper agents written to `~/.claude/agents/`: one producer per size plus
//! a read-only analyst and a read-only reviewer. Only files carrying
//! [`MARKER`] are ever overwritten or removed, so agents the user wrote are
//! left alone.
//!
//! The analyst and the reviewer are read-only because their `tools`
//! frontmatter is an explicit allowlist without any write-capable tool, not
//! because their instructions ask politely.

use std::io;
use std::path::{Path, PathBuf};

use super::Size;

pub const MARKER: &str =
    "<!-- managed by ecotokens router: `ecotokens router off` removes this file -->";

/// Read-only tools every restricted helper may use: file reading and the
/// ecotokens code-intelligence tools.
const READ_ONLY_TOOLS: [&str; 9] = [
    "Read",
    "Grep",
    "Glob",
    "mcp__ecotokens__ecotokens_search",
    "mcp__ecotokens__ecotokens_outline",
    "mcp__ecotokens__ecotokens_symbol",
    "mcp__ecotokens__ecotokens_trace_callers",
    "mcp__ecotokens__ecotokens_trace_callees",
    "mcp__ecotokens__ecotokens_duplicates",
];

/// The analyst may also look things up on the web.
const ANALYST_EXTRA_TOOLS: [&str; 2] = ["WebSearch", "WebFetch"];

/// The six managed helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    /// Does the job; tools inherited from the main session.
    Producer(Size),
    /// Read-only: constraints, options, trade-offs, sources.
    Analyst,
    /// Read-only: judges a result from the diff and test output.
    Reviewer,
}

impl From<Size> for AgentKind {
    fn from(size: Size) -> Self {
        AgentKind::Producer(size)
    }
}

impl AgentKind {
    pub const ALL: [AgentKind; 6] = [
        AgentKind::Producer(Size::Tiny),
        AgentKind::Producer(Size::Everyday),
        AgentKind::Producer(Size::Large),
        AgentKind::Producer(Size::Hardest),
        AgentKind::Analyst,
        AgentKind::Reviewer,
    ];

    pub fn name(self) -> &'static str {
        match self {
            AgentKind::Producer(size) => size.agent_name(),
            AgentKind::Analyst => "router-analyst",
            AgentKind::Reviewer => "router-reviewer",
        }
    }

    /// Model tier: the analyst and the reviewer run on the `large` tier.
    pub fn tier(self) -> Size {
        match self {
            AgentKind::Producer(size) => size,
            AgentKind::Analyst | AgentKind::Reviewer => Size::Large,
        }
    }

    /// The `tools` allowlist, `None` for producers (inherit everything).
    pub fn tools(self) -> Option<Vec<&'static str>> {
        match self {
            AgentKind::Producer(_) => None,
            AgentKind::Analyst => Some(
                READ_ONLY_TOOLS
                    .iter()
                    .chain(ANALYST_EXTRA_TOOLS.iter())
                    .copied()
                    .collect(),
            ),
            AgentKind::Reviewer => Some(READ_ONLY_TOOLS.to_vec()),
        }
    }
}

pub fn default_agents_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|d| d.join(".claude").join("agents"))
}

pub fn agent_path(dir: &Path, kind: AgentKind) -> PathBuf {
    dir.join(format!("{}.md", kind.name()))
}

fn description(kind: AgentKind) -> String {
    let name = kind.name();
    let label = kind.tier().model_label();
    match kind {
        AgentKind::Producer(size) => format!(
            "ecotokens model-router helper for {} jobs ({}). Runs on {label}. \
             Use it when the ecotokens router says to delegate a message to {name}.",
            size.as_str(),
            size.scope()
        ),
        AgentKind::Analyst => format!(
            "ecotokens model-router helper for read-only analysis: constraints, options, \
             trade-offs and sourced research. Cannot edit files or run commands. Runs on {label}. \
             Use it when the ecotokens router says to delegate a message to {name}."
        ),
        AgentKind::Reviewer => format!(
            "ecotokens model-router reviewer: judges a result from the goal, the constraints, \
             the diff and the test output, with a fresh context. Cannot edit files or run \
             commands. Runs on {label}. Use it when the ecotokens router asks for a review by \
             {name}."
        ),
    }
}

/// The rules every helper follows when a job goes wrong. They live here, not
/// in the injected guidance, so they are read only when a helper starts.
fn standing_rules(kind: AgentKind, max_attempts: u32) -> String {
    let attempts = if max_attempts == 1 {
        "1 attempt".to_string()
    } else {
        format!("{max_attempts} attempts")
    };
    let stop = match kind {
        AgentKind::Producer(_) => {
            "run `ecotokens router dossier` (a JSON object on its standard input with the keys \
             goal, constraints, commands, errors and discarded) and return its output as your \
             answer instead of trying again"
        }
        AgentKind::Analyst | AgentKind::Reviewer => {
            "you cannot run commands, so end your reply with the dossier content (goal, \
             constraints, commands run, exact errors, discarded hypotheses) and ask the main \
             session to run `ecotokens router dossier` with it"
        }
    };
    format!(
        "Standing rules:\n\
         - Stop after {attempts} without a new, testable hypothesis. Do not repeat an approach \
         that already failed; {stop}.\n\
         - A failure of the environment (network error, rate limit, missing tool, broken \
         environment) is not a reasoning failure: do not count it as an attempt, report it \
         exactly as it is, and do not ask for a bigger model because of it.\n\
         - If the acceptance criteria contradict each other, ask for clarification instead of \
         choosing one silently.\n\
         - Never raise your own tools or permissions after a failure, and never ask for that: \
         work with what you have, or stop.\n"
    )
}

/// What distinguishes the analyst and the reviewer from a producer.
fn role_rules(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::Producer(_) => "",
        AgentKind::Analyst => {
            "How you work:\n\
             - Deliver, in this order: the constraints you found or were given, the options with \
             their trade-offs, a recommendation when the evidence supports one, and acceptance \
             criteria for the option you recommend. For research, use primary sources, give dates \
             and tie each claim to its source.\n\
             - This is analysis only: do not implement, and do not draft the code or the final \
             document. The main session decides what to do with your options.\n\
             \n"
        }
        AgentKind::Reviewer => {
            "How you work:\n\
             - You review from a fresh context, from the goal, the constraints and the diff and \
             test output you are given, rather than the producer's account of what it did. If the \
             diff or the test output is missing, say so; do not rely on a summary.\n\
             - Your reply is a verdict only: approve or reject, with the reasons, the constraints \
             that are not met and the evidence (file and line). You are read-only: do not edit, do \
             not fix, do not propose a rewrite.\n\
             - The first line of your reply is exactly `Verdict: approve` or `Verdict: reject`; \
             the router reads it. Approve only what the diff and the test output show to be \
             correct; when the evidence is missing, reject and say what is missing.\n\
             \n"
        }
    }
}

fn body(kind: AgentKind, max_attempts: u32) -> String {
    let name = kind.name();
    let label = kind.tier().model_label();
    let intro = match kind {
        AgentKind::Producer(size) => format!(
            "You are the `{}` helper of the ecotokens model router, running on {label}. \
             The router judged that this job ({}) does not need a bigger model.",
            size.as_str(),
            size.scope()
        ),
        AgentKind::Analyst => format!(
            "You are the analyst of the ecotokens model router, running on {label}. You are \
             read-only: you cannot edit files or run commands, and you must not try to."
        ),
        AgentKind::Reviewer => format!(
            "You are the reviewer of the ecotokens model router, running on {label}. You are \
             read-only: you judge, you do not fix."
        ),
    };
    format!(
        "{intro}\n\
         \n\
         - Do the job completely and directly. Keep the answer as short as the job allows.\n\
         - You do not see the user's conversation. Rely on the context given in your prompt. \
         If something essential is missing, say exactly what is missing instead of guessing.\n\
         \n\
         {role}{rules}\n\
         End your reply with exactly this final line, and nothing after it:\n\
         \n\
         — done by {label} ({name})\n",
        role = role_rules(kind),
        rules = standing_rules(kind, max_attempts),
    )
}

/// The agent file for `kind`. `max_attempts` is the configured number of
/// attempts a helper makes before it stops (`router_max_attempts`).
pub fn agent_markdown(kind: AgentKind, max_attempts: u32) -> String {
    let tools = kind
        .tools()
        .map(|t| format!("tools: {}\n", t.join(", ")))
        .unwrap_or_default();
    format!(
        "---\n\
         name: {name}\n\
         description: {description}\n\
         model: {alias}\n\
         {tools}\
         ---\n\
         {MARKER}\n\
         \n\
         {body}",
        name = kind.name(),
        description = description(kind),
        alias = kind.tier().model_alias(),
        body = body(kind, max_attempts),
    )
}

fn is_managed(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|s| s.contains(MARKER))
        .unwrap_or(false)
}

/// Whether the helper can be delegated to: its managed file is there and not
/// shadowed by a file of the same name that the user wrote.
pub fn is_available(dir: &Path, kind: AgentKind) -> bool {
    is_managed(&agent_path(dir, kind))
}

/// Writes the six helpers (idempotent). A file with the same name that the
/// user wrote is kept; its path is returned in the second list.
pub fn install_agents(dir: &Path, max_attempts: u32) -> io::Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    std::fs::create_dir_all(dir)?;
    let mut written = Vec::new();
    let mut kept = Vec::new();
    for kind in AgentKind::ALL {
        let path = agent_path(dir, kind);
        if path.exists() && !is_managed(&path) {
            kept.push(path);
            continue;
        }
        crate::config::atomic_write(&path, agent_markdown(kind, max_attempts))?;
        written.push(path);
    }
    Ok((written, kept))
}

/// Removes the helpers this module wrote; returns the removed paths.
pub fn remove_agents(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut removed = Vec::new();
    for kind in AgentKind::ALL {
        let path = agent_path(dir, kind);
        if is_managed(&path) {
            std::fs::remove_file(&path)?;
            removed.push(path);
        }
    }
    Ok(removed)
}

pub fn are_agents_installed(dir: &Path) -> bool {
    AgentKind::ALL
        .into_iter()
        .all(|kind| is_managed(&agent_path(dir, kind)))
}
