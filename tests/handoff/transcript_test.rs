#[path = "common.rs"]
mod common;
use common::*;

use ecotokens::handoff::transcript::extract;
use ecotokens::handoff::{KeyFile, Kind};
use tempfile::TempDir;

const CWD: &str = "/home/u/proj";

fn builder() -> TranscriptBuilder {
    TranscriptBuilder::new("abc123", CWD, Some("feat/x"))
}

#[test]
fn modified_files_come_first_then_the_most_read_ones() {
    let dir = TempDir::new().unwrap();
    let path = builder()
        .read("/home/u/proj/src/c.rs")
        .read("/home/u/proj/src/c.rs")
        .read("/home/u/proj/src/c.rs")
        .read("/home/u/proj/src/d.rs")
        .read("/home/u/proj/src/e.rs")
        .read("/home/u/proj/src/e.rs")
        .edit("/home/u/proj/src/a.rs", "x", "y")
        .edit("/home/u/proj/src/b.rs", "x", "y")
        .write("/home/u/proj/src/f.rs")
        .build(dir.path());
    let facts = extract(&path, CWD);
    let order: Vec<&str> = facts.key_files.iter().map(|k| k.path.as_str()).collect();
    assert_eq!(
        order,
        ["src/f.rs", "src/b.rs", "src/a.rs", "src/c.rs", "src/e.rs", "src/d.rs"]
    );
    assert_eq!(facts.key_files[0], KeyFile::modified("src/f.rs"));
    assert_eq!(facts.key_files[3], KeyFile::read("src/c.rs", 3));
}

#[test]
fn a_file_both_read_and_edited_is_listed_once_as_modified() {
    let dir = TempDir::new().unwrap();
    let path = builder()
        .read("/home/u/proj/src/a.rs")
        .edit("/home/u/proj/src/a.rs", "x", "y")
        .build(dir.path());
    let facts = extract(&path, CWD);
    assert_eq!(facts.key_files, vec![KeyFile::modified("src/a.rs")]);
}

#[test]
fn key_files_are_capped_at_ten() {
    let dir = TempDir::new().unwrap();
    let mut b = builder();
    for i in 0..15 {
        b = b.read(&format!("/home/u/proj/f{i}.rs"));
    }
    let facts = extract(&b.build(dir.path()), CWD);
    assert_eq!(facts.key_files.len(), 10);
}

#[test]
fn paths_outside_the_working_directory_stay_absolute() {
    let dir = TempDir::new().unwrap();
    let path = builder()
        .edit("/home/u/other/x.rs", "a", "b")
        .edit("/home/u/proj/y.rs", "a", "b")
        .build(dir.path());
    let facts = extract(&path, CWD);
    let paths: Vec<&str> = facts.key_files.iter().map(|k| k.path.as_str()).collect();
    assert_eq!(paths, ["y.rs", "/home/u/other/x.rs"]);
}

#[test]
fn failed_commands_are_recorded_with_their_exit_code() {
    let dir = TempDir::new().unwrap();
    let path = builder()
        .bash_ok("ecotokens router status")
        .bash_fail("ecotokens router status", 1)
        .build(dir.path());
    let facts = extract(&path, CWD);
    assert_eq!(facts.failures.len(), 1);
    assert_eq!(facts.failures[0].kind, Kind::Command);
    assert_eq!(facts.failures[0].text, "ecotokens router status: exit 1");
}

#[test]
fn failed_tests_and_builds_get_their_own_kind() {
    let dir = TempDir::new().unwrap();
    let path = builder()
        .bash_fail("cargo test --lib", 101)
        .bash_fail("cargo clippy -- -D warnings", 101)
        .bash_fail("pytest -x", 1)
        .bash_fail("npm test", 1)
        .bash_fail("go test ./...", 1)
        .build(dir.path());
    let facts = extract(&path, CWD);
    assert_eq!(facts.failures.len(), 5);
    assert!(
        facts.failures.iter().all(|f| f.kind == Kind::TestOrBuild),
        "{:?}",
        facts.failures
    );
}

#[test]
fn failures_of_read_only_exploration_are_ignored() {
    let dir = TempDir::new().unwrap();
    let path = builder()
        .bash_fail("grep -rn foo src", 1)
        .bash_fail("rg missing", 1)
        .bash_fail("find . -name nothing", 1)
        .bash_fail("ls /nope", 2)
        .bash_fail("cat /nope", 1)
        .bash_fail("test -f /nope", 1)
        .bash_fail("command -v nothing", 1)
        .bash_fail("diff a b", 1)
        .bash_fail("cd /tmp && grep x y", 1)
        .build(dir.path());
    assert!(extract(&path, CWD).failures.is_empty());
}

#[test]
fn the_exit_status_of_a_chain_is_the_last_commands_so_that_one_is_judged() {
    let dir = TempDir::new().unwrap();
    let path = builder()
        // Exit 1 comes from the final grep finding nothing: exploration.
        .bash_fail("sed -i s/a/b/ notes.txt && grep -n zzz notes.txt", 1)
        .bash_fail("cat notes.txt | grep zzz", 1)
        .bash_fail("grep -E \"a|b\" notes.txt", 1)
        // Exit 101 comes from cargo test, whatever came before it.
        .bash_fail("grep -q x notes.txt && cargo test", 101)
        .bash_fail("cd /tmp && make deploy", 2)
        .build(dir.path());
    let facts = extract(&path, CWD);
    let found: Vec<(Kind, &str)> = facts
        .failures
        .iter()
        .map(|f| (f.kind, f.text.as_str()))
        .collect();
    assert_eq!(
        found,
        [
            (Kind::TestOrBuild, "make deploy: exit 2"),
            (
                Kind::TestOrBuild,
                "grep -q x notes.txt && cargo test: exit 101"
            ),
        ]
    );
}

#[test]
fn a_reverted_edit_is_an_abandoned_approach() {
    let dir = TempDir::new().unwrap();
    let path = builder()
        .edit("/home/u/proj/src/a.rs", "old", "new")
        .edit("/home/u/proj/src/a.rs", "new", "old")
        .build(dir.path());
    let facts = extract(&path, CWD);
    assert_eq!(facts.failures.len(), 1);
    assert_eq!(facts.failures[0].kind, Kind::EditReverted);
    assert_eq!(facts.failures[0].text, "src/a.rs");
}

#[test]
fn an_edit_rewritten_in_the_same_place_is_noted() {
    let dir = TempDir::new().unwrap();
    let path = builder()
        .edit("/home/u/proj/src/a.rs", "old", "first")
        .edit("/home/u/proj/src/a.rs", "first", "second")
        .build(dir.path());
    let facts = extract(&path, CWD);
    assert_eq!(facts.failures.len(), 1);
    assert_eq!(facts.failures[0].kind, Kind::EditRewritten);
}

#[test]
fn edits_on_different_files_are_not_related() {
    let dir = TempDir::new().unwrap();
    let path = builder()
        .edit("/home/u/proj/a.rs", "old", "new")
        .edit("/home/u/proj/b.rs", "new", "old")
        .build(dir.path());
    assert!(extract(&path, CWD).failures.is_empty());
}

#[test]
fn rejected_edits_are_tool_misuse_not_a_failed_approach() {
    let dir = TempDir::new().unwrap();
    let path = builder()
        .edit_rejected("/home/u/proj/a.rs", "missing", "x")
        .build(dir.path());
    let facts = extract(&path, CWD);
    assert!(facts.failures.is_empty());
}

#[test]
fn subagent_work_is_not_the_main_sessions_history() {
    let dir = TempDir::new().unwrap();
    let path = builder()
        .bash_fail("cargo test", 101)
        .sidechain(true)
        .bash_fail("cargo build", 101)
        .edit("/home/u/proj/side.rs", "a", "b")
        .sidechain(false)
        .build(dir.path());
    let facts = extract(&path, CWD);
    assert_eq!(facts.failures.len(), 1);
    assert_eq!(facts.failures[0].text, "cargo test: exit 101");
    assert!(facts.key_files.is_empty());
}

#[test]
fn failures_are_deduplicated_newest_first_capped_and_shortened() {
    let dir = TempDir::new().unwrap();
    let mut b = builder().bash_fail("repeat me", 1);
    for i in 1..=10 {
        b = b.bash_fail(&format!("tool{i} --run"), 1);
    }
    b = b.bash_fail("repeat me", 1);
    b = b.bash_fail(&format!("long {}", "z".repeat(400)), 1);
    let facts = extract(&b.build(dir.path()), CWD);
    assert_eq!(facts.failures.len(), 8, "{:?}", facts.failures);
    assert!(facts.failures[0].text.starts_with("long z"));
    assert!(facts.failures.iter().all(|f| f.text.chars().count() <= 160));
    assert_eq!(
        facts
            .failures
            .iter()
            .filter(|f| f.text.starts_with("repeat me"))
            .count(),
        1,
        "the duplicate was merged"
    );
    assert_eq!(
        facts.failures[1].text, "repeat me: exit 1",
        "kept at its latest position"
    );
}

#[test]
fn the_branch_comes_from_the_transcript() {
    let dir = TempDir::new().unwrap();
    let with = builder().bash_ok("true").build(dir.path());
    assert_eq!(extract(&with, CWD).branch.as_deref(), Some("feat/x"));
    let without = TranscriptBuilder::new("nobranch", CWD, None)
        .bash_ok("true")
        .build(dir.path());
    assert_eq!(extract(&without, CWD).branch, None);
}

#[test]
fn malformed_lines_are_counted_and_skipped() {
    let dir = TempDir::new().unwrap();
    let path = builder()
        .garbage("{not json")
        .garbage("")
        .edit("/home/u/proj/a.rs", "x", "y")
        .garbage("[1,2,3]")
        .build(dir.path());
    let facts = extract(&path, CWD);
    assert!(facts.found);
    assert_eq!(facts.skipped_lines, 2, "blank lines are not counted");
    assert_eq!(facts.key_files, vec![KeyFile::modified("a.rs")]);
}

#[test]
fn a_missing_transcript_yields_empty_facts() {
    let dir = TempDir::new().unwrap();
    let facts = extract(&dir.path().join("nope.jsonl"), CWD);
    assert!(!facts.found);
    assert!(facts.key_files.is_empty() && facts.failures.is_empty());
    assert_eq!(facts.branch, None);
}

#[test]
fn non_utf8_bytes_do_not_stop_the_extraction() {
    let dir = TempDir::new().unwrap();
    let good = builder()
        .edit("/home/u/proj/a.rs", "x", "y")
        .build(dir.path());
    let mut bytes = std::fs::read(&good).unwrap();
    bytes.extend_from_slice(&[0xff, 0xfe, b'\n']);
    std::fs::write(&good, bytes).unwrap();
    let facts = extract(&good, CWD);
    assert_eq!(facts.key_files, vec![KeyFile::modified("a.rs")]);
}
