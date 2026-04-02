use ecotokens::metrics::store::{append_to, read_from, CommandFamily, FilterMode, Interception};
use tempfile::TempDir;

fn metrics_file(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join("metrics.db")
}

fn make_interception(tokens_before: u32, tokens_after: u32, mode: FilterMode) -> Interception {
    Interception::new(
        "git status".to_string(),
        CommandFamily::Git,
        Some("/repo".to_string()),
        tokens_before,
        tokens_after,
        mode,
        false,
        10,
        None,
        None,
    )
}

#[test]
fn append_creates_file_and_valid_json_line() {
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);

    let item = make_interception(100, 40, FilterMode::Filtered);
    append_to(&path, &item).unwrap();

    assert!(path.exists(), "metrics.db should be created");
    let items = read_from(&path).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].command, "git status");
}

#[test]
fn read_existing_file_returns_interceptions() {
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);

    let item = make_interception(200, 80, FilterMode::Filtered);
    append_to(&path, &item).unwrap();

    let items = read_from(&path).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].tokens_before, 200);
}

#[test]
fn absent_file_returns_ok_empty() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nonexistent.db");
    let items = read_from(&path).unwrap();
    assert!(items.is_empty(), "absent file should return empty vec");
}

#[test]
fn read_triggers_jsonl_migration_when_db_absent() {
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);
    let jsonl_path = path.with_extension("jsonl");

    let line = r#"{"id":"abc","timestamp":"2026-01-01T00:00:00+00:00","command":"git status","command_family":"git","git_root":null,"tokens_before":100,"tokens_after":40,"savings_pct":60.0,"mode":"filtered","redacted":false,"duration_ms":10}"#;
    std::fs::write(&jsonl_path, format!("{line}\n")).unwrap();

    let items = read_from(&path).unwrap();

    assert_eq!(items.len(), 1);
    assert!(path.exists(), "metrics.db should be created by migration");
    assert!(
        !jsonl_path.exists(),
        "legacy metrics.jsonl should be renamed after migration"
    );
    assert!(
        path.with_extension("jsonl.migrated").exists(),
        "migration should preserve a backup as metrics.jsonl.migrated"
    );
}

#[test]
fn two_successive_appends_produce_two_lines() {
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);

    append_to(&path, &make_interception(100, 40, FilterMode::Filtered)).unwrap();
    append_to(&path, &make_interception(200, 80, FilterMode::Summarized)).unwrap();

    let items = read_from(&path).unwrap();
    assert_eq!(items.len(), 2, "should have two lines");
}

#[test]
fn savings_pct_is_zero_in_passthrough_mode() {
    let item = make_interception(100, 100, FilterMode::Passthrough);
    assert_eq!(
        item.savings_pct, 0.0,
        "savings should be 0 in passthrough mode"
    );
}

#[test]
fn savings_pct_calculated_correctly_in_filtered_mode() {
    let item = make_interception(100, 40, FilterMode::Filtered);
    let expected = 60.0f32;
    assert!(
        (item.savings_pct - expected).abs() < 0.01,
        "savings should be ~60%"
    );
}

#[test]
fn old_jsonl_without_content_fields_deserializes_ok() {
    let line = r#"{"id":"abc","timestamp":"2026-01-01T00:00:00+00:00","command":"git status","command_family":"git","git_root":null,"tokens_before":100,"tokens_after":40,"savings_pct":60.0,"mode":"filtered","redacted":false,"duration_ms":10}"#;
    let item: Interception = serde_json::from_str(line).unwrap();
    assert_eq!(item.content_before, None);
    assert_eq!(item.content_after, None);
}

#[test]
fn migration_resumes_from_migrating_file_after_crash() {
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);
    let migrating_path = path.with_extension("jsonl.migrating");

    let line = r#"{"id":"crash-id","timestamp":"2026-01-01T00:00:00+00:00","command":"cargo test","command_family":"cargo","git_root":null,"tokens_before":500,"tokens_after":200,"savings_pct":60.0,"mode":"filtered","redacted":false,"duration_ms":5}"#;
    std::fs::write(&migrating_path, format!("{line}\n")).unwrap();

    let items = read_from(&path).unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "crash-id");
    assert!(
        !migrating_path.exists(),
        ".migrating doit être renommé en .migrated"
    );
    assert!(path.with_extension("jsonl.migrated").exists());
}

#[test]
fn concurrent_migration_produces_no_duplicates_and_no_error() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);
    let jsonl_path = path.with_extension("jsonl");

    let lines: Vec<String> = (0..3)
        .map(|i| format!(
            r#"{{"id":"concurrent-{i}","timestamp":"2026-01-01T00:00:0{i}+00:00","command":"git log","command_family":"git","git_root":null,"tokens_before":100,"tokens_after":50,"savings_pct":50.0,"mode":"filtered","redacted":false,"duration_ms":1}}"#
        ))
        .collect();
    std::fs::write(&jsonl_path, lines.join("\n") + "\n").unwrap();

    let path = Arc::new(path);
    let barrier = Arc::new(Barrier::new(2));

    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                read_from(&path)
            })
        })
        .collect();

    for h in handles {
        assert!(
            h.join().unwrap().is_ok(),
            "read_from doit réussir en concurrence"
        );
    }

    let final_items = read_from(&path).unwrap();
    assert_eq!(
        final_items.len(),
        3,
        "exactement 3 enregistrements, pas de doublon"
    );
    assert!(!jsonl_path.exists(), "metrics.jsonl doit avoir été renommé");
}

#[test]
fn content_is_stored_and_truncated() {
    let long = "x".repeat(5000);
    let item = Interception::new(
        "cmd".to_string(),
        CommandFamily::Git,
        None,
        100,
        40,
        FilterMode::Filtered,
        false,
        10,
        Some(long.clone()),
        Some(long),
    );
    let before = item.content_before.unwrap();
    assert!(before.len() < 5000, "content should be truncated");
    assert!(before.contains("[truncated]"));
}
