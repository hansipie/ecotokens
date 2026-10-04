#[path = "common.rs"]
mod common;
use common::*;

use ecotokens::handoff::cli::{clean, list, load, set, write, SetInput};
use ecotokens::handoff::hook::process;
use ecotokens::handoff::store::write_handoff;
use tempfile::TempDir;

const CWD: &str = "/home/u/proj";

/// Ids that would escape the handoff directory or name something else.
const HOSTILE: [&str; 7] = [
    "../../escape",
    "/etc/passwd",
    "..",
    "a/../../b",
    "a\0b",
    "..\\..\\x",
    "~root",
];

fn tree(root: &std::path::Path) -> Vec<String> {
    fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            out.push(path.to_string_lossy().into_owned());
            if path.is_dir() {
                walk(&path, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

/// A sandbox with a handoff directory nested inside it, so that any file that
/// escapes the handoff directory still lands somewhere the test can see.
fn sandbox() -> (TempDir, std::path::PathBuf) {
    let root = TempDir::new().unwrap();
    let dir = root.path().join("config").join("handoff");
    (root, dir)
}

#[test]
fn a_hostile_session_id_never_creates_a_file_through_write() {
    let (root, dir) = sandbox();
    for id in HOSTILE {
        let out = write(&dir, &settings(), id, Some(CWD), false, fixed_now());
        assert_eq!(out.code, 2, "{id:?}");
    }
    assert!(tree(root.path()).is_empty(), "{:?}", tree(root.path()));
}

#[test]
fn a_hostile_session_id_never_creates_a_file_through_set() {
    let (root, dir) = sandbox();
    for id in HOSTILE {
        let input = SetInput {
            objective: Some("x".into()),
            ..SetInput::default()
        };
        let out = set(&dir, &settings(), id, input, None, false, fixed_now());
        assert_eq!(out.code, 2, "{id:?}");
    }
    assert!(tree(root.path()).is_empty(), "{:?}", tree(root.path()));
}

#[test]
fn a_hostile_id_is_never_read_or_consumed_by_load() {
    let (root, dir) = sandbox();
    write_handoff(&dir, &sample_handoff("abc123", CWD)).unwrap();
    let before = tree(root.path());
    for id in HOSTILE {
        let out = load(&dir, &settings(), id, false, fixed_now());
        assert_eq!(out.code, 1, "{id:?}");
        assert!(out.out.is_empty(), "{id:?} printed something");
    }
    assert_eq!(tree(root.path()), before, "nothing changed on disk");
}

#[test]
fn a_hostile_session_id_in_the_hook_input_is_ignored() {
    let (root, dir) = sandbox();
    for id in HOSTILE {
        let input = serde_json::json!({
            "session_id": id, "cwd": CWD, "source": "clear", "transcript_path": "/t"
        })
        .to_string();
        assert_eq!(
            process(&input, &settings(), &dir, None, fixed_now()),
            None,
            "{id:?}"
        );
    }
    assert!(tree(root.path()).is_empty(), "{:?}", tree(root.path()));
}

#[test]
fn a_hostile_transcript_path_only_reads_never_writes() {
    // The record may point anywhere; extraction reads it and nothing else.
    let (root, dir) = sandbox();
    let input = hook_input("abc123", CWD, "startup", "../../../../etc/shadow");
    assert_eq!(process(&input, &settings(), &dir, None, fixed_now()), None);
    let out = write(&dir, &settings(), "abc123", Some(CWD), true, fixed_now());
    assert_eq!(out.code, 0, "{}", out.err);
    // Only the handoff directory and what it holds exist.
    for path in tree(root.path()) {
        let inside = std::path::Path::new(&path).starts_with(&dir);
        let parent_of_it = dir.starts_with(&path);
        assert!(
            inside || parent_of_it,
            "{path} is outside the handoff directory"
        );
    }
}

#[test]
fn list_and_clean_only_touch_regular_files_inside_the_directory() {
    let (root, dir) = sandbox();
    std::fs::create_dir_all(&dir).unwrap();
    let outside = root.path().join("outside.md");
    std::fs::write(&outside, "keep me").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, dir.join("link.md")).unwrap();
    let mut old = sample_handoff("old", CWD);
    old.created = fixed_now() - chrono::Duration::days(90);
    write_handoff(&dir, &old).unwrap();

    list(&dir, &settings(), CWD, true, true, fixed_now());
    let out = clean(&dir, &settings(), false, true, fixed_now());
    assert_eq!(out.code, 0, "{}", out.err);
    assert!(
        outside.exists(),
        "a symlink never leads clean outside the directory"
    );
    assert!(!dir.join("old.md").exists());
}

#[cfg(unix)]
#[test]
fn everything_written_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;
    let (root, dir) = sandbox();
    let input = hook_input("abc123", CWD, "startup", "/t");
    process(&input, &settings(), &dir, None, fixed_now());
    write(&dir, &settings(), "abc123", Some(CWD), false, fixed_now());
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&dir.join(".sessions")), 0o700);
    assert_eq!(mode(&dir.join(".sessions").join("abc123.json")), 0o600);
    assert_eq!(mode(&dir.join("abc123.md")), 0o600);
    drop(root);
}
