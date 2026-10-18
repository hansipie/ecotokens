//! Private and allowed locations: where the router must not (or may only)
//! send messages to Jev.
//!
//! The gate runs before anything else in the hook, so a private message
//! never reaches the judge, the statistics or the network. It fails closed:
//! a working directory that cannot be resolved, or settings that cannot be
//! read, count as private. "Private" only means "not sent to Jev": it does
//! not stop the main Claude Code session from reading the folder.

use std::path::{Path, PathBuf};

use globset::GlobBuilder;

use crate::config::Settings;

/// Why a folder is (or is not) private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The folder, or one of its subfolders, is listed.
    Exact,
    /// A listed pattern matches the folder or one of its parents.
    Pattern,
    /// The folder is reached through a symbolic link into a listed folder.
    Symlink,
    /// The folder is another worktree of a listed repository.
    Worktree,
    /// The working directory is unknown or cannot be resolved.
    Unresolvable,
    /// The settings file could not be read, so the lists cannot be trusted.
    CorruptList,
    /// Strict mode is on and the folder is not on the allow list.
    NotAllowed,
    None,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Exact => "exact",
            Reason::Pattern => "pattern",
            Reason::Symlink => "symlink",
            Reason::Worktree => "worktree",
            Reason::Unresolvable => "unresolvable",
            Reason::CorruptList => "corrupt-list",
            Reason::NotAllowed => "not-allowed",
            Reason::None => "none",
        }
    }
}

/// What the gate decided for a working directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateResult {
    Open,
    Private(Reason),
    NotAllowed,
}

/// The gate's answer with the reason and the list entry behind it, for
/// `ecotokens router private check`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub private: bool,
    pub not_allowed: bool,
    pub reason: Reason,
    pub entry: Option<String>,
}

/// State of a list entry, computed and never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryState {
    Active,
    /// The folder no longer exists; the entry is kept.
    Missing,
    /// The pattern does not compile; the entry is ignored.
    Invalid,
}

impl EntryState {
    pub fn as_str(self) -> &'static str {
        match self {
            EntryState::Active => "active",
            EntryState::Missing => "missing",
            EntryState::Invalid => "invalid",
        }
    }
}

/// The working directory as the matcher sees it.
struct Resolved {
    /// The path as given, made absolute.
    lexical: PathBuf,
    /// The path with every symbolic link resolved.
    canonical: PathBuf,
    /// Root of the repository's main worktree, when inside a repository.
    worktree_root: Option<PathBuf>,
}

fn resolve(cwd: Option<&Path>) -> Option<Resolved> {
    let cwd = cwd?;
    let lexical = if cwd.is_absolute() {
        cwd.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(cwd)
    };
    let canonical = lexical.canonicalize().ok()?;
    let worktree_root = main_worktree_root(&canonical);
    Some(Resolved {
        lexical,
        canonical,
        worktree_root,
    })
}

/// Root of the main worktree of the repository containing `path`, found by
/// reading `.git` (no `git` subprocess): a `.git` directory is the main
/// worktree itself; a `.git` file points at an admin directory whose
/// `commondir` leads back to the main `.git`.
fn main_worktree_root(path: &Path) -> Option<PathBuf> {
    for dir in path.ancestors() {
        let git = dir.join(".git");
        if git.is_dir() {
            return dir.canonicalize().ok();
        }
        if git.is_file() {
            let text = std::fs::read_to_string(&git).ok()?;
            let admin = text.trim().strip_prefix("gitdir:")?.trim();
            let admin = if Path::new(admin).is_absolute() {
                PathBuf::from(admin)
            } else {
                dir.join(admin)
            };
            return match std::fs::read_to_string(admin.join("commondir")) {
                Ok(common) => {
                    let common = admin.join(common.trim()).canonicalize().ok()?;
                    if common.file_name().is_some_and(|n| n == ".git") {
                        common.parent().map(Path::to_path_buf)
                    } else {
                        Some(common)
                    }
                }
                // A submodule or a bare layout: the directory is its own root.
                Err(_) => dir.canonicalize().ok(),
            };
        }
    }
    None
}

fn is_pattern(entry: &str) -> bool {
    entry.contains(['*', '?', '[', '{'])
}

fn compile(entry: &str) -> Option<globset::GlobMatcher> {
    GlobBuilder::new(entry)
        .literal_separator(true)
        .build()
        .ok()
        .map(|g| g.compile_matcher())
}

/// Whether `matcher` matches `path` or one of its ancestors.
fn pattern_covers(matcher: &globset::GlobMatcher, path: &Path) -> bool {
    path.ancestors().any(|a| matcher.is_match(a))
}

/// The first entry of `list` that covers the folder, with how it does.
fn match_list(list: &[String], resolved: &Resolved) -> Option<(Reason, String)> {
    for entry in list {
        if is_pattern(entry) {
            let Some(matcher) = compile(entry) else {
                continue; // invalid: ignored
            };
            if pattern_covers(&matcher, &resolved.canonical) {
                return Some((Reason::Pattern, entry.clone()));
            }
            if resolved
                .worktree_root
                .as_deref()
                .is_some_and(|root| pattern_covers(&matcher, root))
            {
                return Some((Reason::Worktree, entry.clone()));
            }
            continue;
        }
        let listed = Path::new(entry)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(entry));
        if resolved.lexical.starts_with(&listed) {
            return Some((Reason::Exact, entry.clone()));
        }
        if resolved.canonical.starts_with(&listed) {
            return Some((Reason::Symlink, entry.clone()));
        }
        if resolved
            .worktree_root
            .as_deref()
            .is_some_and(|root| root.starts_with(&listed))
        {
            return Some((Reason::Worktree, entry.clone()));
        }
    }
    None
}

/// The gate's full answer. `trusted` is false when the settings file could
/// not be read (see [`Settings::config_file_trusted`]).
pub fn check(cwd: Option<&Path>, settings: &Settings, trusted: bool) -> Check {
    let private = |reason, entry| Check {
        private: true,
        not_allowed: false,
        reason,
        entry,
    };
    if !trusted {
        return private(Reason::CorruptList, None);
    }
    let Some(resolved) = resolve(cwd) else {
        return private(Reason::Unresolvable, None);
    };
    if let Some((reason, entry)) = match_list(&settings.router_private, &resolved) {
        return private(reason, Some(entry));
    }
    if settings.router_strict && match_list(&settings.router_allowed, &resolved).is_none() {
        return Check {
            private: false,
            not_allowed: true,
            reason: Reason::NotAllowed,
            entry: None,
        };
    }
    Check {
        private: false,
        not_allowed: false,
        reason: Reason::None,
        entry: None,
    }
}

/// The gate: private wins over allowed; strict mode needs an allowed entry.
pub fn gate(cwd: Option<&Path>, settings: &Settings, trusted: bool) -> GateResult {
    let c = check(cwd, settings, trusted);
    if c.private {
        GateResult::Private(c.reason)
    } else if c.not_allowed {
        GateResult::NotAllowed
    } else {
        GateResult::Open
    }
}

pub fn entry_state(entry: &str) -> EntryState {
    if is_pattern(entry) {
        if compile(entry).is_some() {
            EntryState::Active
        } else {
            EntryState::Invalid
        }
    } else if Path::new(entry).exists() {
        EntryState::Active
    } else {
        EntryState::Missing
    }
}

/// How an entry is stored: an existing folder in its canonical form, a
/// pattern as typed, a missing folder as an absolute path.
pub fn normalize_entry(raw: &str) -> String {
    if is_pattern(raw) {
        return raw.to_string();
    }
    let path = Path::new(raw);
    if let Ok(canonical) = path.canonicalize() {
        return canonical.to_string_lossy().into_owned();
    }
    if path.is_absolute() {
        return raw.to_string();
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(path).to_string_lossy().into_owned(),
        Err(_) => raw.to_string(),
    }
}

/// Adds an entry; `false` when it was already there.
pub fn add_entry(list: &mut Vec<String>, raw: &str) -> bool {
    let entry = normalize_entry(raw);
    if list.contains(&entry) {
        return false;
    }
    list.push(entry);
    true
}

/// Removes an entry, however it is spelled; `false` when it was absent.
pub fn remove_entry(list: &mut Vec<String>, raw: &str) -> bool {
    let normalized = normalize_entry(raw);
    let before = list.len();
    list.retain(|e| e != raw && *e != normalized);
    list.len() != before
}
