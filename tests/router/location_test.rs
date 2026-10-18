#[path = "common.rs"]
mod common;
use common::{settings_private, settings_strict};

use std::path::{Path, PathBuf};

use ecotokens::config::Settings;
use ecotokens::router::location::{
    add_entry, check, entry_state, gate, remove_entry, EntryState, GateResult, Reason,
};
use tempfile::TempDir;

/// A temp dir whose path is already canonical (no symlinked prefix).
fn tmp() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    (dir, root)
}

fn mkdir(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path).unwrap();
    path.to_path_buf()
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn gate_for(cwd: &Path, settings: &Settings) -> GateResult {
    gate(Some(cwd), settings, true)
}

#[test]
fn an_unlisted_folder_is_open() {
    let (_d, root) = tmp();
    let work = mkdir(&root.join("work"));
    assert_eq!(gate_for(&work, &Settings::default()), GateResult::Open);
    assert_eq!(
        gate_for(&work, &settings_private(&[&s(&root.join("other"))])),
        GateResult::Open
    );
}

#[test]
fn an_exact_folder_is_private() {
    let (_d, root) = tmp();
    let proj = mkdir(&root.join("proj"));
    let settings = settings_private(&[&s(&proj)]);
    assert_eq!(
        gate_for(&proj, &settings),
        GateResult::Private(Reason::Exact)
    );
}

#[test]
fn a_subdirectory_of_a_private_folder_is_private() {
    let (_d, root) = tmp();
    let proj = mkdir(&root.join("proj"));
    let deep = mkdir(&proj.join("src").join("deep"));
    let settings = settings_private(&[&s(&proj)]);
    assert!(matches!(gate_for(&deep, &settings), GateResult::Private(_)));
    // A sibling with a shared prefix is not a subdirectory.
    let sibling = mkdir(&root.join("proj-old"));
    assert_eq!(gate_for(&sibling, &settings), GateResult::Open);
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_into_a_private_folder_is_private() {
    let (_d, root) = tmp();
    let proj = mkdir(&root.join("proj"));
    mkdir(&proj.join("sub"));
    let link = root.join("shortcut");
    std::os::unix::fs::symlink(proj.join("sub"), &link).unwrap();
    let settings = settings_private(&[&s(&proj)]);
    assert_eq!(
        gate_for(&link, &settings),
        GateResult::Private(Reason::Symlink)
    );
}

/// Lays out a main repository and a second worktree of it, the way git does.
fn repo_with_worktree(root: &Path) -> (PathBuf, PathBuf) {
    let main = mkdir(&root.join("main"));
    let git = mkdir(&main.join(".git"));
    let admin = mkdir(&git.join("worktrees").join("wt"));
    std::fs::write(admin.join("commondir"), "../..\n").unwrap();
    let wt = mkdir(&root.join("wt-checkout"));
    std::fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
    std::fs::write(
        admin.join("gitdir"),
        format!("{}\n", wt.join(".git").display()),
    )
    .unwrap();
    (main, wt)
}

#[test]
fn a_second_worktree_of_a_private_repository_is_private() {
    let (_d, root) = tmp();
    let (main, wt) = repo_with_worktree(&root);
    let sub = mkdir(&wt.join("src"));
    let settings = settings_private(&[&s(&main)]);
    assert_eq!(
        gate_for(&wt, &settings),
        GateResult::Private(Reason::Worktree)
    );
    assert_eq!(
        gate_for(&sub, &settings),
        GateResult::Private(Reason::Worktree)
    );
    // The main worktree itself matches directly.
    assert_eq!(
        gate_for(&main, &settings),
        GateResult::Private(Reason::Exact)
    );
    // An unrelated repository is not affected.
    let (_d2, root2) = tmp();
    let (_m2, wt2) = repo_with_worktree(&root2);
    assert_eq!(gate_for(&wt2, &settings), GateResult::Open);
}

#[test]
fn patterns_match_whole_path_segments_only() {
    let (_d, root) = tmp();
    let acme = mkdir(&root.join("clients").join("acme"));
    let deep = mkdir(&acme.join("repo"));
    let old = mkdir(&root.join("clients-old").join("acme"));
    let settings = settings_private(&[&format!("{}/clients/*", root.display())]);
    assert_eq!(
        gate_for(&acme, &settings),
        GateResult::Private(Reason::Pattern)
    );
    assert!(matches!(gate_for(&deep, &settings), GateResult::Private(_)));
    assert_eq!(
        gate_for(&old, &settings),
        GateResult::Open,
        "clients-old is not clients"
    );
    // The pattern does not match across separators.
    let settings = settings_private(&[&format!("{}/*", root.display())]);
    assert!(matches!(
        gate_for(&root.join("clients"), &settings),
        GateResult::Private(_)
    ));
}

#[test]
fn an_invalid_pattern_is_ignored_and_flagged() {
    let (_d, root) = tmp();
    let work = mkdir(&root.join("work"));
    assert_eq!(entry_state("[unclosed"), EntryState::Invalid);
    assert_eq!(
        gate_for(&work, &settings_private(&["[unclosed"])),
        GateResult::Open
    );
    // The other entries still apply.
    let settings = settings_private(&["[unclosed", &s(&work)]);
    assert!(matches!(gate_for(&work, &settings), GateResult::Private(_)));
}

#[test]
fn an_unresolvable_or_missing_working_directory_is_private() {
    let settings = Settings::default();
    assert_eq!(
        gate(None, &settings, true),
        GateResult::Private(Reason::Unresolvable)
    );
    let (_d, root) = tmp();
    let gone = root.join("does-not-exist");
    assert_eq!(
        gate_for(&gone, &settings),
        GateResult::Private(Reason::Unresolvable)
    );
}

#[test]
fn an_untrusted_settings_file_means_private() {
    let (_d, root) = tmp();
    let work = mkdir(&root.join("work"));
    assert_eq!(
        gate(Some(&work), &Settings::default(), false),
        GateResult::Private(Reason::CorruptList)
    );
}

#[test]
fn strict_mode_only_routes_allowed_locations() {
    let (_d, root) = tmp();
    let allowed = mkdir(&root.join("allowed"));
    let allowed_sub = mkdir(&allowed.join("sub"));
    let other = mkdir(&root.join("other"));
    let settings = settings_strict(&[&s(&allowed)]);
    assert_eq!(gate_for(&allowed, &settings), GateResult::Open);
    assert_eq!(gate_for(&allowed_sub, &settings), GateResult::Open);
    assert_eq!(gate_for(&other, &settings), GateResult::NotAllowed);
    // Without strict mode the allow list does nothing.
    let relaxed = Settings {
        router_strict: false,
        ..settings.clone()
    };
    assert_eq!(gate_for(&other, &relaxed), GateResult::Open);
}

#[test]
fn private_wins_over_allowed() {
    let (_d, root) = tmp();
    let proj = mkdir(&root.join("proj"));
    let settings = Settings {
        router_private: vec![s(&proj)],
        ..settings_strict(&[&s(&root)])
    };
    assert_eq!(
        gate_for(&proj, &settings),
        GateResult::Private(Reason::Exact)
    );
}

#[test]
fn missing_entries_are_kept_and_flagged() {
    let (_d, root) = tmp();
    let gone = root.join("deleted-project");
    assert_eq!(entry_state(&s(&gone)), EntryState::Missing);
    assert_eq!(entry_state(&s(&root)), EntryState::Active);
    assert_eq!(
        entry_state(&format!("{}/*", root.display())),
        EntryState::Active
    );

    let mut list = Vec::new();
    assert!(add_entry(&mut list, &s(&gone)));
    assert_eq!(list, [s(&gone)], "a missing folder is still stored");
}

#[test]
fn add_and_remove_are_idempotent_and_canonicalise_existing_folders() {
    let (_d, root) = tmp();
    let proj = mkdir(&root.join("proj"));
    let mut list = Vec::new();
    assert!(add_entry(&mut list, &s(&proj)));
    assert!(
        !add_entry(&mut list, &s(&proj)),
        "second add changes nothing"
    );
    assert_eq!(list.len(), 1);

    // A path spelled with `..` lands on the same canonical entry.
    let roundabout = format!("{}/proj/../proj", root.display());
    assert!(!add_entry(&mut list, &roundabout));
    assert_eq!(list.len(), 1);

    assert!(remove_entry(&mut list, &roundabout));
    assert!(list.is_empty());
    assert!(
        !remove_entry(&mut list, &s(&proj)),
        "second remove changes nothing"
    );
}

#[test]
fn check_reports_the_reason_and_the_entry() {
    let (_d, root) = tmp();
    let proj = mkdir(&root.join("proj"));
    let work = mkdir(&root.join("work"));

    let settings = settings_private(&[&s(&proj)]);
    let c = check(Some(&proj), &settings, true);
    assert!(c.private && !c.not_allowed);
    assert_eq!(c.reason, Reason::Exact);
    assert_eq!(c.entry.as_deref(), Some(s(&proj).as_str()));

    let c = check(Some(&work), &settings, true);
    assert!(!c.private && !c.not_allowed);
    assert_eq!(c.reason, Reason::None);
    assert_eq!(c.entry, None);

    let c = check(None, &settings, true);
    assert!(c.private);
    assert_eq!(c.reason, Reason::Unresolvable);

    let c = check(Some(&work), &settings, false);
    assert!(c.private);
    assert_eq!(c.reason, Reason::CorruptList);

    let strict = settings_strict(&[&s(&proj)]);
    let c = check(Some(&work), &strict, true);
    assert!(!c.private && c.not_allowed);
    assert_eq!(c.reason, Reason::NotAllowed);

    let names: Vec<_> = [
        Reason::Exact,
        Reason::Pattern,
        Reason::Symlink,
        Reason::Worktree,
        Reason::Unresolvable,
        Reason::CorruptList,
        Reason::NotAllowed,
        Reason::None,
    ]
    .iter()
    .map(|r| r.as_str())
    .collect();
    assert_eq!(
        names,
        [
            "exact",
            "pattern",
            "symlink",
            "worktree",
            "unresolvable",
            "corrupt-list",
            "not-allowed",
            "none"
        ]
    );
}
