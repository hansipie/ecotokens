#[path = "common.rs"]
mod common;
use common::*;

use ecotokens::config::Settings;
use ecotokens::handoff::cli::{set, write, SetInput};
use ecotokens::handoff::store::{read_handoff, write_session_record};
use ecotokens::handoff::{Kind, SessionRecord, Status};
use tempfile::TempDir;

const CWD: &str = "/home/u/proj";
const ID: &str = "abc123";

struct Env {
    root: TempDir,
}

impl Env {
    /// A handoff directory with a session record pointing at a transcript that
    /// has one edit and one failed test run.
    fn new() -> Env {
        let root = TempDir::new().unwrap();
        let transcript = TranscriptBuilder::new(ID, CWD, Some("feat/x"))
            .edit("/home/u/proj/src/a.rs", "x", "y")
            .bash_fail("cargo test", 101)
            .build(root.path());
        let env = Env { root };
        write_session_record(
            &env.dir(),
            &SessionRecord {
                session_id: ID.into(),
                transcript_path: transcript.to_string_lossy().into_owned(),
                cwd: CWD.into(),
                source: "startup".into(),
                recorded: fixed_now(),
            },
        )
        .unwrap();
        env
    }

    fn dir(&self) -> std::path::PathBuf {
        self.root.path().join("handoff")
    }
}

fn json(out: &str) -> serde_json::Value {
    serde_json::from_str(out).unwrap_or_else(|e| panic!("not JSON ({e}): {out}"))
}

#[test]
fn write_creates_a_pending_file_with_the_mechanical_fields() {
    let env = Env::new();
    let out = write(&env.dir(), &settings(), ID, Some(CWD), false, fixed_now());
    assert_eq!(out.code, 0, "{}", out.err);
    assert!(out.out.contains("abc123.md"), "{}", out.out);
    let h = read_handoff(&env.dir(), ID).unwrap();
    assert_eq!(h.status, Status::Pending);
    assert_eq!(h.cwd, CWD);
    assert_eq!(h.branch.as_deref(), Some("feat/x"));
    assert_eq!(h.created, fixed_now());
    assert_eq!(h.key_files.len(), 1);
    assert_eq!(h.key_files[0].path, "src/a.rs");
    assert_eq!(h.failed_attempts.len(), 1);
    assert_eq!(h.failed_attempts[0].kind, Kind::TestOrBuild);
    assert_eq!(
        h.objective, None,
        "model-written fields are not provided yet"
    );
    assert_eq!(h.next_steps, None);
}

#[test]
fn write_json_follows_the_contract() {
    let env = Env::new();
    let out = write(&env.dir(), &settings(), ID, Some(CWD), true, fixed_now());
    assert_eq!(out.code, 0);
    let v = json(&out.out);
    assert_eq!(v["session"], ID);
    assert_eq!(v["cwd"], CWD);
    assert_eq!(v["branch"], "feat/x");
    assert_eq!(v["key_files"], serde_json::json!(["src/a.rs"]));
    assert_eq!(v["failed_attempts"][0]["kind"], "test_or_build");
    assert_eq!(v["failed_attempts"][0]["text"], "cargo test: exit 101");
    assert_eq!(v["transcript_found"], true);
    assert_eq!(v["skipped_lines"], 0);
    assert_eq!(v["masked"], false);
    assert!(v["chars"].as_u64().unwrap() > 0);
    assert!(v["path"].as_str().unwrap().ends_with("abc123.md"));
}

#[test]
fn write_without_a_session_record_still_writes_a_valid_file() {
    let root = TempDir::new().unwrap();
    let dir = root.path().join("handoff");
    let out = write(&dir, &settings(), ID, Some(CWD), true, fixed_now());
    assert_eq!(out.code, 0, "{}", out.err);
    assert!(out.err.to_lowercase().contains("transcript"), "{}", out.err);
    assert_eq!(json(&out.out)["transcript_found"], false);
    let h = read_handoff(&dir, ID).unwrap();
    assert!(h.key_files.is_empty() && h.failed_attempts.is_empty());
    assert_eq!(h.cwd, CWD);
}

#[test]
fn write_with_a_deleted_transcript_degrades_the_same_way() {
    let env = Env::new();
    std::fs::remove_file(env.root.path().join(format!("{ID}.jsonl"))).unwrap();
    let out = write(&env.dir(), &settings(), ID, Some(CWD), true, fixed_now());
    assert_eq!(out.code, 0);
    assert_eq!(json(&out.out)["transcript_found"], false);
}

#[test]
fn write_refuses_an_invalid_session_id() {
    let root = TempDir::new().unwrap();
    let dir = root.path().join("handoff");
    for bad in ["", "../escape", "a/b"] {
        let out = write(&dir, &settings(), bad, Some(CWD), false, fixed_now());
        assert_eq!(out.code, 2, "{bad:?}");
        assert!(out.err.contains("invalid"), "{}", out.err);
    }
    assert!(!dir.exists(), "nothing was written");
}

#[test]
fn writing_twice_replaces_the_file_and_refreshes_the_timestamp() {
    let env = Env::new();
    write(&env.dir(), &settings(), ID, Some(CWD), false, fixed_now());
    let input = SetInput {
        objective: Some("first".into()),
        ..SetInput::default()
    };
    set(&env.dir(), &settings(), ID, input, None, false, fixed_now());
    let later = fixed_now() + chrono::Duration::minutes(30);
    write(&env.dir(), &settings(), ID, Some(CWD), false, later);
    let h = read_handoff(&env.dir(), ID).unwrap();
    assert_eq!(h.created, later);
    assert_eq!(h.objective, None, "the second run starts a fresh file");
}

#[test]
fn set_fills_the_model_written_fields_from_flags() {
    let env = Env::new();
    write(&env.dir(), &settings(), ID, Some(CWD), false, fixed_now());
    let input = SetInput {
        objective: Some("O".into()),
        problem: Some("P".into()),
        next_steps: Some("N".into()),
        abandoned: vec!["h1".into(), "h2".into()],
    };
    let out = set(&env.dir(), &settings(), ID, input, None, false, fixed_now());
    assert_eq!(out.code, 0, "{}", out.err);
    let h = read_handoff(&env.dir(), ID).unwrap();
    assert_eq!(h.objective.as_deref(), Some("O"));
    assert_eq!(h.problem.as_deref(), Some("P"));
    assert_eq!(h.next_steps.as_deref(), Some("N"));
    let kinds: Vec<(Kind, &str)> = h
        .failed_attempts
        .iter()
        .map(|a| (a.kind, a.text.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            (Kind::TestOrBuild, "cargo test: exit 101"),
            (Kind::Hypothesis, "h1"),
            (Kind::Hypothesis, "h2"),
        ],
        "hypotheses follow the mechanical entries"
    );
    assert_eq!(h.key_files.len(), 1, "extracted fields are untouched");
}

#[test]
fn set_reads_a_json_object_from_stdin_and_flags_win() {
    let env = Env::new();
    write(&env.dir(), &settings(), ID, Some(CWD), false, fixed_now());
    let stdin = r#"{"objective":"O2","problem":"P2","next_steps":"N2","abandoned":["x"]}"#;
    let input = SetInput {
        objective: Some("FLAG".into()),
        ..SetInput::default()
    };
    let out = set(
        &env.dir(),
        &settings(),
        ID,
        input,
        Some(stdin),
        false,
        fixed_now(),
    );
    assert_eq!(out.code, 0, "{}", out.err);
    let h = read_handoff(&env.dir(), ID).unwrap();
    assert_eq!(h.objective.as_deref(), Some("FLAG"));
    assert_eq!(h.problem.as_deref(), Some("P2"));
    assert_eq!(h.next_steps.as_deref(), Some("N2"));
    assert!(h
        .failed_attempts
        .iter()
        .any(|a| a.kind == Kind::Hypothesis && a.text == "x"));
}

#[test]
fn set_rejects_invalid_stdin_and_leaves_the_file_alone() {
    let env = Env::new();
    write(&env.dir(), &settings(), ID, Some(CWD), false, fixed_now());
    let before = std::fs::read_to_string(env.dir().join("abc123.md")).unwrap();
    let out = set(
        &env.dir(),
        &settings(),
        ID,
        SetInput::default(),
        Some("not json"),
        false,
        fixed_now(),
    );
    assert_eq!(out.code, 2);
    assert_eq!(
        std::fs::read_to_string(env.dir().join("abc123.md")).unwrap(),
        before
    );
}

#[test]
fn set_without_a_file_says_to_run_write_first() {
    let root = TempDir::new().unwrap();
    let out = set(
        &root.path().join("handoff"),
        &settings(),
        ID,
        SetInput::default(),
        None,
        false,
        fixed_now(),
    );
    assert_eq!(out.code, 1);
    assert!(out.err.contains("handoff write"), "{}", out.err);
}

#[test]
fn a_second_set_with_hypotheses_replaces_the_first_ones() {
    let env = Env::new();
    write(&env.dir(), &settings(), ID, Some(CWD), false, fixed_now());
    let first = SetInput {
        abandoned: vec!["old guess".into()],
        ..SetInput::default()
    };
    set(&env.dir(), &settings(), ID, first, None, false, fixed_now());
    let second = SetInput {
        abandoned: vec!["new guess".into()],
        ..SetInput::default()
    };
    set(
        &env.dir(),
        &settings(),
        ID,
        second,
        None,
        false,
        fixed_now(),
    );
    let h = read_handoff(&env.dir(), ID).unwrap();
    let hyps: Vec<&str> = h
        .failed_attempts
        .iter()
        .filter(|a| a.kind == Kind::Hypothesis)
        .map(|a| a.text.as_str())
        .collect();
    assert_eq!(hyps, ["new guess"]);
}

#[test]
fn set_without_hypotheses_keeps_the_existing_ones() {
    let env = Env::new();
    write(&env.dir(), &settings(), ID, Some(CWD), false, fixed_now());
    let first = SetInput {
        abandoned: vec!["keep me".into()],
        ..SetInput::default()
    };
    set(&env.dir(), &settings(), ID, first, None, false, fixed_now());
    let again = SetInput {
        objective: Some("O".into()),
        ..SetInput::default()
    };
    set(&env.dir(), &settings(), ID, again, None, false, fixed_now());
    let h = read_handoff(&env.dir(), ID).unwrap();
    assert!(h.failed_attempts.iter().any(|a| a.text == "keep me"));
}

#[test]
fn set_trims_to_the_configured_limit_and_reports_what_it_removed() {
    let env = Env::new();
    write(&env.dir(), &settings(), ID, Some(CWD), false, fixed_now());
    let small = Settings {
        handoff_max_chars: 700,
        ..settings()
    };
    let input = SetInput {
        objective: Some("o".repeat(100)),
        problem: Some(format!("{}\n{}", "p".repeat(900), "q".repeat(900))),
        next_steps: Some("n".repeat(100)),
        abandoned: vec![],
    };
    let out = set(&env.dir(), &small, ID, input, None, true, fixed_now());
    assert_eq!(out.code, 0, "{}", out.err);
    let text = std::fs::read_to_string(env.dir().join("abc123.md")).unwrap();
    assert!(text.chars().count() <= 700, "{}", text.chars().count());
    let v = json(&out.out);
    assert!(!v["trimmed"].as_array().unwrap().is_empty(), "{v}");
    assert_eq!(v["session"], ID);
    assert!(v["chars"].as_u64().unwrap() <= 700);
}

#[test]
fn set_json_lists_nothing_trimmed_when_everything_fits() {
    let env = Env::new();
    write(&env.dir(), &settings(), ID, Some(CWD), false, fixed_now());
    let input = SetInput {
        objective: Some("O".into()),
        ..SetInput::default()
    };
    let out = set(&env.dir(), &settings(), ID, input, None, true, fixed_now());
    assert_eq!(json(&out.out)["trimmed"], serde_json::json!([]));
}
