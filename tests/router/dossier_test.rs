#[path = "common.rs"]
mod common;
use common::{run_ecotokens, stderr_of, stdout_of, temp_config_home, ConfigHome};

use std::path::{Path, PathBuf};
use std::process::Command;

use ecotokens::handoff::store::{clean, read_handoff_file};
use ecotokens::handoff::Status;
use serde_json::{json, Value};
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .expect("git is required for the dossier tests");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A repository with one commit and one uncommitted edit.
fn repo_with_edit() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    git(&root, &["init", "-q"]);
    std::fs::write(root.join("lib.rs"), "fn answer() -> u32 {\n    41\n}\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "init"]);
    std::fs::write(
        root.join("lib.rs"),
        "fn answer() -> u32 {\n    42 // the real edit\n}\n",
    )
    .unwrap();
    (dir, root)
}

fn handoff_dir(home: &ConfigHome) -> PathBuf {
    home.ecotokens_dir().join("handoff")
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    match std::fs::read_dir(dir) {
        Ok(rd) => {
            let mut v: Vec<_> = rd
                .map(|e| e.unwrap().path())
                .filter(|p| p.is_file())
                .collect();
            v.sort();
            v
        }
        Err(_) => Vec::new(),
    }
}

fn dossier(home: &ConfigHome, cwd: &Path, input: &str, extra: &[&str]) -> std::process::Output {
    let mut args = vec!["router", "dossier"];
    args.extend_from_slice(extra);
    run_ecotokens(home, &args, Some(cwd), Some(input))
}

fn full_input() -> Value {
    json!({
        "goal": "Make the parser accept trailing commas",
        "constraints": ["keep the public API", "no new dependencies"],
        "commands": ["cargo test parser", "cargo clippy"],
        "errors": ["error[E0308]: mismatched types in parse_list"],
        "discarded": ["Tried a regex pre-pass: it breaks nested lists", "Tried serde_json: wrong grammar"],
    })
}

#[test]
fn the_dossier_holds_the_six_items() {
    let home = temp_config_home();
    let (_d, repo) = repo_with_edit();
    let out = dossier(&home, &repo, &full_input().to_string(), &[]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let md = stdout_of(&out);
    assert!(
        md.contains("Make the parser accept trailing commas"),
        "goal: {md}"
    );
    assert!(
        md.contains("keep the public API") && md.contains("no new dependencies"),
        "constraints"
    );
    assert!(md.contains("42 // the real edit"), "the real diff: {md}");
    assert!(md.contains("cargo test parser"), "commands");
    assert!(
        md.contains("mismatched types in parse_list"),
        "exact errors"
    );
    assert!(md.contains("regex pre-pass"), "discarded hypotheses");
    assert!(md.contains("router-dossier"), "kind marker");

    // The file is on disk, owned by the handoff directory.
    let files = files_in(&handoff_dir(&home));
    assert_eq!(files.len(), 1, "{files:?}");
    let on_disk = std::fs::read_to_string(&files[0]).unwrap();
    assert!(on_disk.contains("42 // the real edit"));
}

#[test]
fn the_diff_comes_from_the_repository_not_from_the_input() {
    let home = temp_config_home();
    let (_d, repo) = repo_with_edit();
    let mut input = full_input();
    input["diff"] = json!("FAKE-DIFF-FROM-THE-PREVIOUS-AGENT");
    input["current_diff"] = json!("FAKE-DIFF-TWO");
    let out = dossier(&home, &repo, &input.to_string(), &[]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let md = stdout_of(&out);
    assert!(!md.contains("FAKE-DIFF"), "{md}");
    assert!(md.contains("42 // the real edit"));
}

#[test]
fn the_diff_is_bounded() {
    let home = temp_config_home();
    let (_d, repo) = repo_with_edit();
    let big: String = (0..4000)
        .map(|i| format!("line number {i} of a very large edit\n"))
        .collect();
    std::fs::write(repo.join("lib.rs"), big).unwrap();
    let out = dossier(&home, &repo, &full_input().to_string(), &[]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let md = stdout_of(&out);
    assert!(md.chars().count() < 12_000, "{} chars", md.chars().count());
    assert!(md.contains("truncated"), "the cut is announced");
    assert!(
        md.contains("Make the parser accept trailing commas"),
        "the rest survives"
    );
}

/// Samples of recognised secrets: each must be changed by the masker, or the
/// test below proves nothing.
fn secret_samples() -> Vec<(&'static str, String)> {
    vec![
        ("github", format!("ghp_{}", "a1B2c3D4e5".repeat(4))),
        ("aws", "AKIAIOSFODNN7EXAMPLE".to_string()),
        (
            "bearer",
            "Authorization: Bearer abcdefghijklmnopqrstuvwxyz0123456789".to_string(),
        ),
        ("anthropic", format!("sk-ant-api03-{}AA", "x".repeat(93))),
        (
            "openai",
            format!("sk-{}T3BlbkFJ{}", "a".repeat(20), "b".repeat(20)),
        ),
        ("env", "API_SECRET=hunter2hunter2hunter2".to_string()),
    ]
}

#[test]
fn secrets_in_any_input_field_are_masked() {
    let samples = secret_samples();
    for (name, secret) in &samples {
        let (masked, changed) = ecotokens::masking::mask(secret);
        assert!(
            changed && masked != *secret,
            "sample `{name}` is not recognised by the masker"
        );
    }
    let home = temp_config_home();
    let (_d, repo) = repo_with_edit();
    // Also a secret inside the working-tree diff.
    let (_, token) = &samples[0];
    std::fs::write(
        repo.join("lib.rs"),
        format!("fn answer() -> u32 {{\n    42\n}}\nconst TOKEN: &str = \"{token}\";\n"),
    )
    .unwrap();
    for field in ["goal", "constraints", "commands", "errors", "discarded"] {
        for (name, secret) in &samples {
            let text = format!("seen {secret} here");
            let mut input = full_input();
            input[field] = if field == "goal" {
                json!(text)
            } else {
                json!([text])
            };
            let out = dossier(&home, &repo, &input.to_string(), &[]);
            assert!(out.status.success(), "{field}/{name}: {}", stderr_of(&out));
            let md = stdout_of(&out);
            assert!(
                !md.contains(secret.as_str()),
                "{field}/{name} leaked on stdout"
            );
            for file in files_in(&handoff_dir(&home)) {
                let on_disk = std::fs::read_to_string(file).unwrap();
                assert!(
                    !on_disk.contains(secret.as_str()),
                    "{field}/{name} leaked on disk"
                );
            }
        }
    }
}

#[test]
fn invalid_input_exits_non_zero_and_writes_nothing() {
    let home = temp_config_home();
    let (_d, repo) = repo_with_edit();
    for input in [
        "",
        "not json",
        "[1, 2, 3]",
        "\"just a string\"",
        "42",
        "null",
    ] {
        let out = dossier(&home, &repo, input, &[]);
        assert!(!out.status.success(), "{input:?} should fail");
        assert!(
            !stderr_of(&out).is_empty(),
            "{input:?}: an actionable message"
        );
        assert!(stdout_of(&out).is_empty(), "{input:?}");
    }
    // Wrong field types are invalid too.
    for input in [
        r#"{"goal": 5}"#,
        r#"{"constraints": "one string"}"#,
        r#"{"errors": [1, 2]}"#,
    ] {
        let out = dossier(&home, &repo, input, &[]);
        assert!(!out.status.success(), "{input} should fail");
    }
    assert!(
        files_in(&handoff_dir(&home)).is_empty(),
        "nothing was written"
    );
}

#[test]
fn missing_optional_keys_render_as_not_provided() {
    let home = temp_config_home();
    let (_d, repo) = repo_with_edit();
    let out = dossier(&home, &repo, r#"{"goal": "only a goal"}"#, &[]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let md = stdout_of(&out);
    assert!(md.contains("only a goal"));
    assert!(md.contains("not provided"), "{md}");
    let out = dossier(&home, &repo, "{}", &[]);
    assert!(
        out.status.success(),
        "an empty object is a valid, empty dossier"
    );
    assert!(stdout_of(&out).contains("not provided"));
}

#[test]
fn without_a_repository_the_diff_section_says_so() {
    let home = temp_config_home();
    let plain = TempDir::new().unwrap();
    let out = dossier(&home, plain.path(), &full_input().to_string(), &[]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let md = stdout_of(&out);
    assert!(md.contains("not a git repository"), "{md}");
}

#[test]
fn a_clean_repository_says_there_is_no_diff() {
    let home = temp_config_home();
    let (_d, repo) = repo_with_edit();
    git(&repo, &["checkout", "--", "."]);
    let out = dossier(&home, &repo, &full_input().to_string(), &[]);
    assert!(out.status.success());
    assert!(
        stdout_of(&out).contains("no uncommitted changes"),
        "{}",
        stdout_of(&out)
    );
}

#[test]
fn the_file_loads_with_the_handoff_reader_and_follows_its_retention() {
    let home = temp_config_home();
    let (_d, repo) = repo_with_edit();
    let out = dossier(&home, &repo, &full_input().to_string(), &[]);
    assert!(out.status.success());
    let files = files_in(&handoff_dir(&home));
    assert_eq!(files.len(), 1);

    let h = read_handoff_file(&files[0]).expect("the handoff reader parses a dossier");
    assert_eq!(
        h.objective.as_deref(),
        Some("Make the parser accept trailing commas")
    );
    assert!(h
        .header_extra
        .iter()
        .any(|(k, v)| k == "kind" && v == "router-dossier"));
    assert!(h
        .extra_sections
        .iter()
        .any(|(name, _)| name == "Current diff"));
    assert_eq!(h.failed_attempts.len(), 2);
    assert_eq!(
        h.status,
        Status::Consumed,
        "a dossier is read on purpose; it is never injected by the next SessionStart"
    );

    let dir = handoff_dir(&home);
    let now = chrono::Utc::now();
    let kept = clean(&dir, 30, 48, now, true).unwrap();
    assert!(kept.removed.is_empty(), "still within the retention");
    let later = now + chrono::Duration::days(31);
    let removed = clean(&dir, 30, 48, later, false).unwrap();
    assert_eq!(
        removed.removed.len(),
        1,
        "the existing retention rules clean it"
    );
}

#[test]
fn json_output_has_the_path_and_the_size() {
    let home = temp_config_home();
    let (_d, repo) = repo_with_edit();
    let out = dossier(&home, &repo, &full_input().to_string(), &["--json"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let v: Value = serde_json::from_str(&stdout_of(&out)).unwrap();
    let path = PathBuf::from(v["path"].as_str().unwrap());
    assert!(path.exists());
    assert!(path.starts_with(handoff_dir(&home)));
    let chars = v["chars"].as_u64().unwrap() as usize;
    assert_eq!(
        chars,
        std::fs::read_to_string(&path).unwrap().chars().count()
    );
    assert_eq!(v.as_object().unwrap().len(), 2, "exactly path and chars");
}

#[test]
fn two_dossiers_written_in_a_row_never_overwrite_each_other() {
    let home = temp_config_home();
    let (_d, repo) = repo_with_edit();
    for _ in 0..5 {
        let out = dossier(&home, &repo, &full_input().to_string(), &[]);
        assert!(out.status.success(), "{}", stderr_of(&out));
    }
    assert_eq!(files_in(&handoff_dir(&home)).len(), 5);
}
