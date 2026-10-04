#[path = "common.rs"]
mod common;
use common::*;

use ecotokens::handoff::cli::{list, load, set, write, SetInput};
use ecotokens::handoff::format::{mask_text, render};
use ecotokens::handoff::hook::process;
use ecotokens::handoff::store::{read_handoff, write_session_record};
use ecotokens::handoff::SessionRecord;
use tempfile::TempDir;

const CWD: &str = "/home/u/proj";
const ID: &str = "abc123";

// Fake credentials in the formats the masking module recognises.
const GITHUB: &str = "ghp_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789";
const AWS: &str = "AKIAIOSFODNN7EXAMPLE";
const BEARER: &str = "Bearer abcdef0123456789.token-value";
const ENV_SECRET: &str = "API_KEY=hunter2hunter2";

fn assert_clean(text: &str, what: &str) {
    for secret in [
        GITHUB,
        AWS,
        "abcdef0123456789.token-value",
        "hunter2hunter2",
    ] {
        assert!(!text.contains(secret), "{what} leaks {secret}:\n{text}");
    }
}

fn setup() -> (TempDir, std::path::PathBuf) {
    let root = TempDir::new().unwrap();
    let dir = root.path().join("handoff");
    (root, dir)
}

fn record_for(dir: &std::path::Path, transcript: &std::path::Path) {
    write_session_record(
        dir,
        &SessionRecord {
            session_id: ID.into(),
            transcript_path: transcript.to_string_lossy().into_owned(),
            cwd: CWD.into(),
            source: "startup".into(),
            recorded: fixed_now(),
        },
    )
    .unwrap();
}

#[test]
fn secrets_in_a_failed_command_never_reach_the_file_or_the_output() {
    let (root, dir) = setup();
    let transcript = TranscriptBuilder::new(ID, CWD, Some("main"))
        .bash_fail(
            &format!("curl -H 'Authorization: {BEARER}' https://api.example.com"),
            22,
        )
        .bash_fail(&format!("deploy --token {GITHUB} --key {AWS}"), 1)
        .bash_fail(&format!("export {ENV_SECRET}"), 1)
        .build(root.path());
    record_for(&dir, &transcript);

    let human = write(&dir, &settings(), ID, Some(CWD), false, fixed_now());
    assert_clean(&human.out, "human output");
    assert_clean(&human.err, "stderr");
    let json = write(&dir, &settings(), ID, Some(CWD), true, fixed_now());
    assert_clean(&json.out, "json output");
    let v: serde_json::Value = serde_json::from_str(&json.out).expect("still valid JSON");
    assert_eq!(v["masked"], true);

    let file = std::fs::read_to_string(dir.join("abc123.md")).unwrap();
    assert_clean(&file, "the file");
    assert!(
        file.contains("[GITHUB_TOKEN]") || file.contains("[REDACTED]"),
        "{file}"
    );
}

#[test]
fn secrets_in_the_model_written_fields_are_masked_after_the_second_step() {
    let (root, dir) = setup();
    let transcript = TranscriptBuilder::new(ID, CWD, None)
        .edit("/home/u/proj/a.rs", "x", "y")
        .build(root.path());
    record_for(&dir, &transcript);
    write(&dir, &settings(), ID, Some(CWD), false, fixed_now());

    let input = SetInput {
        objective: Some(format!("Rotate {ENV_SECRET} for the service")),
        problem: Some(format!("The key {AWS} was pasted in the chat")),
        next_steps: Some(format!("Revoke {GITHUB}")),
        abandoned: vec![format!("Send {BEARER} in the header")],
    };
    let human = set(
        &dir,
        &settings(),
        ID,
        input.clone(),
        None,
        false,
        fixed_now(),
    );
    assert_clean(&human.out, "human output");
    let json = set(&dir, &settings(), ID, input, None, true, fixed_now());
    assert_clean(&json.out, "json output");
    serde_json::from_str::<serde_json::Value>(&json.out).expect("still valid JSON");

    let file = std::fs::read_to_string(dir.join("abc123.md")).unwrap();
    assert_clean(&file, "the file");
    let h = read_handoff(&dir, ID).unwrap();
    assert!(h.objective.unwrap().contains("[REDACTED]"));
}

#[test]
fn a_secret_added_by_hand_is_masked_again_at_injection_and_by_list_and_load() {
    let (_root, dir) = setup();
    let mut h = sample_handoff(ID, CWD);
    h.objective = Some(format!("Use {GITHUB} then {ENV_SECRET}"));
    h.next_steps = Some(format!("Rotate {AWS}"));
    std::fs::create_dir_all(&dir).unwrap();
    // Written straight to disk, as a user editing the file would.
    std::fs::write(dir.join("abc123.md"), render(&h)).unwrap();

    let list_json = list(&dir, &settings(), CWD, false, true, fixed_now());
    assert_clean(&list_json.out, "list --json");
    let list_human = list(&dir, &settings(), CWD, false, false, fixed_now());
    assert_clean(&list_human.out, "list");

    let injected = process(
        &common::hook_input("new1", CWD, "clear", "/t"),
        &settings(),
        &dir,
        None,
        fixed_now(),
    )
    .expect("injected");
    assert_clean(&injected, "the injected text");
    serde_json::from_str::<serde_json::Value>(&injected).expect("still valid JSON");

    // The injection consumed it; load still prints it, masked.
    let loaded = load(&dir, &settings(), ID, false, fixed_now());
    assert_eq!(loaded.code, 0, "{}", loaded.err);
    assert_clean(&loaded.out, "load");
    let loaded_json = load(&dir, &settings(), ID, true, fixed_now());
    assert_clean(&loaded_json.out, "load --json");
    serde_json::from_str::<serde_json::Value>(&loaded_json.out).expect("still valid JSON");
}

#[test]
fn masking_leaves_clean_text_byte_for_byte_unchanged() {
    let h = sample_handoff(ID, CWD);
    let text = render(&h);
    assert_eq!(mask_text(&text), text);
}
