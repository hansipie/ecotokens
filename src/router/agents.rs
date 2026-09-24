//! Helper agents, one per size, written to `~/.claude/agents/`. Only files
//! carrying [`MARKER`] are ever overwritten or removed, so agents the user
//! wrote are left alone.

use std::io;
use std::path::{Path, PathBuf};

use super::Size;

pub const MARKER: &str =
    "<!-- managed by ecotokens router: `ecotokens router off` removes this file -->";

pub fn default_agents_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|d| d.join(".claude").join("agents"))
}

pub fn agent_path(dir: &Path, size: Size) -> PathBuf {
    dir.join(format!("{}.md", size.agent_name()))
}

pub fn agent_markdown(size: Size) -> String {
    format!(
        "---\n\
         name: {name}\n\
         description: ecotokens model-router helper for {size} jobs ({scope}). Runs on {label}. \
         Use it when the ecotokens router says to delegate a message to {name}.\n\
         model: {alias}\n\
         ---\n\
         {MARKER}\n\
         \n\
         You are the `{size}` helper of the ecotokens model router, running on {label}. \
         The router judged that this job ({scope}) does not need a bigger model.\n\
         \n\
         - Do the job completely and directly. Keep the answer as short as the job allows.\n\
         - You do not see the user's conversation. Rely on the context given in your prompt. \
         If something essential is missing, say exactly what is missing instead of guessing.\n\
         - End your reply with exactly this final line, and nothing after it:\n\
         \n\
         — done by {label} ({name})\n",
        name = size.agent_name(),
        size = size.as_str(),
        scope = size.scope(),
        label = size.model_label(),
        alias = size.model_alias(),
    )
}

fn is_managed(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|s| s.contains(MARKER))
        .unwrap_or(false)
}

/// Writes the four helpers (idempotent). A file with the same name that the
/// user wrote is kept; its path is returned in the second list.
pub fn install_agents(dir: &Path) -> io::Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    std::fs::create_dir_all(dir)?;
    let mut written = Vec::new();
    let mut kept = Vec::new();
    for size in Size::ALL {
        let path = agent_path(dir, size);
        if path.exists() && !is_managed(&path) {
            kept.push(path);
            continue;
        }
        crate::config::atomic_write(&path, agent_markdown(size))?;
        written.push(path);
    }
    Ok((written, kept))
}

/// Removes the helpers this module wrote; returns the removed paths.
pub fn remove_agents(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut removed = Vec::new();
    for size in Size::ALL {
        let path = agent_path(dir, size);
        if is_managed(&path) {
            std::fs::remove_file(&path)?;
            removed.push(path);
        }
    }
    Ok(removed)
}

pub fn are_agents_installed(dir: &Path) -> bool {
    Size::ALL
        .into_iter()
        .all(|size| is_managed(&agent_path(dir, size)))
}
