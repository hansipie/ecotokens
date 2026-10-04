#[path = "common.rs"]
mod common;
use common::*;

use ecotokens::handoff::store::{
    handoff_path, read_handoff, read_session_record, session_record_path, write_handoff,
    write_session_record, ReadError,
};
use ecotokens::handoff::{validate_session_id, SessionRecord};
use tempfile::TempDir;

fn record(id: &str) -> SessionRecord {
    SessionRecord {
        session_id: id.into(),
        transcript_path: "/tmp/t.jsonl".into(),
        cwd: "/home/u/proj".into(),
        source: "startup".into(),
        recorded: fixed_now(),
    }
}

#[test]
fn session_ids_are_restricted_to_a_safe_alphabet() {
    for ok in ["abc123", "a-b_C9", "8c7ad6e1-2210-4fd5-9a87-b8927a101748"] {
        assert!(validate_session_id(ok), "{ok}");
    }
    let too_long = "a".repeat(129);
    for bad in [
        "",
        ".",
        "..",
        "../x",
        "a/b",
        "a b",
        "a\0b",
        "é",
        "a.md",
        too_long.as_str(),
    ] {
        assert!(!validate_session_id(bad), "{bad:?}");
    }
}

#[test]
fn paths_stay_inside_the_handoff_directory() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        handoff_path(dir.path(), "abc"),
        Some(dir.path().join("abc.md"))
    );
    assert_eq!(
        session_record_path(dir.path(), "abc"),
        Some(dir.path().join(".sessions").join("abc.json"))
    );
    for hostile in ["../x", "/etc/passwd", "a/../../b", ""] {
        assert_eq!(handoff_path(dir.path(), hostile), None, "{hostile}");
        assert_eq!(session_record_path(dir.path(), hostile), None, "{hostile}");
    }
}

#[test]
fn a_handoff_round_trips_through_the_directory() {
    let dir = TempDir::new().unwrap();
    let h = sample_handoff("abc123", "/home/u/proj");
    let path = write_handoff(dir.path(), &h).unwrap();
    assert_eq!(path, dir.path().join("abc123.md"));
    assert_eq!(read_handoff(dir.path(), "abc123").unwrap(), h);
}

#[test]
fn writing_leaves_no_temporary_file_behind() {
    let dir = TempDir::new().unwrap();
    write_handoff(dir.path(), &sample_handoff("abc123", "/p")).unwrap();
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["abc123.md".to_string()]);
}

#[test]
fn writing_an_invalid_session_id_is_refused() {
    let dir = TempDir::new().unwrap();
    let h = sample_handoff("../escape", "/p");
    let err = write_handoff(dir.path(), &h).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn reading_distinguishes_missing_from_corrupted() {
    let dir = TempDir::new().unwrap();
    assert!(matches!(
        read_handoff(dir.path(), "nope"),
        Err(ReadError::Missing)
    ));
    std::fs::write(dir.path().join("bad.md"), "not a handoff").unwrap();
    assert!(matches!(
        read_handoff(dir.path(), "bad"),
        Err(ReadError::Corrupted(_))
    ));
    assert!(matches!(
        read_handoff(dir.path(), "../x"),
        Err(ReadError::Missing)
    ));
}

#[test]
fn session_records_round_trip() {
    let dir = TempDir::new().unwrap();
    let r = record("abc123");
    write_session_record(dir.path(), &r).unwrap();
    assert_eq!(read_session_record(dir.path(), "abc123"), Some(r));
}

#[test]
fn missing_or_corrupt_session_records_read_as_none() {
    let dir = TempDir::new().unwrap();
    assert_eq!(read_session_record(dir.path(), "nope"), None);
    std::fs::create_dir_all(dir.path().join(".sessions")).unwrap();
    std::fs::write(dir.path().join(".sessions").join("bad.json"), "{oops").unwrap();
    assert_eq!(read_session_record(dir.path(), "bad"), None);
    assert_eq!(read_session_record(dir.path(), "../x"), None);
}

#[test]
fn refreshing_a_session_record_replaces_it() {
    let dir = TempDir::new().unwrap();
    write_session_record(dir.path(), &record("abc123")).unwrap();
    let mut again = record("abc123");
    again.source = "clear".into();
    write_session_record(dir.path(), &again).unwrap();
    assert_eq!(
        read_session_record(dir.path(), "abc123").unwrap().source,
        "clear"
    );
}

#[test]
fn invalid_session_id_in_a_record_is_refused() {
    let dir = TempDir::new().unwrap();
    let err = write_session_record(dir.path(), &record("../x")).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}

#[cfg(unix)]
#[test]
fn directory_and_files_are_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new().unwrap();
    let dir = root.path().join("handoff");
    let path = write_handoff(&dir, &sample_handoff("abc123", "/p")).unwrap();
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&path), 0o600);
}
