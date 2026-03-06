use ecotokens::metrics::store::{append_to, read_from, CommandFamily, FilterMode, Interception};
use tempfile::TempDir;

fn metrics_file(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join("metrics.jsonl")
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
    )
}

#[test]
fn append_creates_file_and_valid_json_line() {
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);

    let item = make_interception(100, 40, FilterMode::Filtered);
    append_to(&path, &item).unwrap();

    assert!(path.exists(), "metrics.jsonl should be created");
    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
    assert_eq!(parsed["command"].as_str().unwrap(), "git status");
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
    let path = dir.path().join("nonexistent.jsonl");
    let items = read_from(&path).unwrap();
    assert!(items.is_empty(), "absent file should return empty vec");
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
    assert_eq!(item.savings_pct, 0.0, "savings should be 0 in passthrough mode");
}

#[test]
fn savings_pct_calculated_correctly_in_filtered_mode() {
    let item = make_interception(100, 40, FilterMode::Filtered);
    let expected = 60.0f32;
    assert!((item.savings_pct - expected).abs() < 0.01, "savings should be ~60%");
}
