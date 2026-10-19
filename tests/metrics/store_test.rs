use ecotokens::metrics::store::{
    agent_to_hook_type_post, agent_to_hook_type_pre, append_to, read_from, CommandFamily,
    FilterMode, HookType, Interception,
};
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

// ── Secrets in the stored command ─────────────────────────────────────────────
//
// The command line (or tool input) is persisted in the `command` column and
// shown back by `ecotokens gain`. A secret typed on that command line must be
// masked before it reaches the database. The keys are built at runtime so no
// secret-shaped literal sits in the source.

fn fake_aws_key() -> String {
    format!("AKIA{}", "IOSFODNN7EXAMPLE")
}

fn fake_github_token() -> String {
    format!("ghp_{}", "a1B2".repeat(9))
}

/// Raw bytes of the database and its write-ahead log: a row that has not been
/// checkpointed yet only lives in `-wal`.
fn raw_db_bytes(path: &std::path::Path) -> Vec<u8> {
    let mut bytes = std::fs::read(path).unwrap_or_default();
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    bytes.extend(std::fs::read(std::path::PathBuf::from(wal)).unwrap_or_default());
    bytes
}

fn contains_bytes(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

fn assert_no_secret_stored(path: &std::path::Path) {
    let (aws, gh) = (fake_aws_key(), fake_github_token());
    let bytes = raw_db_bytes(path);
    assert!(!contains_bytes(&bytes, &aws), "AWS key stored in clear");
    assert!(!contains_bytes(&bytes, &gh), "GitHub token stored in clear");
    for item in read_from(path).unwrap() {
        assert!(!item.command.contains(&aws), "{}", item.command);
        assert!(!item.command.contains(&gh), "{}", item.command);
        assert!(item.command.contains("[AWS_KEY]"), "{}", item.command);
        assert!(item.command.contains("[GITHUB_TOKEN]"), "{}", item.command);
    }
}

fn interception_with_command(command: String, mode: FilterMode) -> Interception {
    Interception::new(
        command,
        CommandFamily::Generic,
        None,
        100,
        40,
        mode,
        false,
        1,
        None,
        None,
    )
}

#[test]
fn append_masks_secrets_in_the_stored_command() {
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);
    let cmd = format!(
        "echo {} && curl -H 'x: {}' example.org",
        fake_aws_key(),
        fake_github_token()
    );
    append_to(&path, &interception_with_command(cmd, FilterMode::Filtered)).unwrap();
    assert_no_secret_stored(&path);
    let items = read_from(&path).unwrap();
    assert_eq!(
        items[0].command, "echo [AWS_KEY] && curl -H 'x: [GITHUB_TOKEN]' example.org",
        "only the secrets are replaced, the rest of the command is kept"
    );
}

#[test]
fn append_masks_secrets_in_a_post_hook_read_command() {
    // Post-hook rows store `Read <file_path>` (see `metrics_command`).
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);
    let cmd = format!(
        "Read /tmp/{}/{}/notes.rs",
        fake_aws_key(),
        fake_github_token()
    );
    let item =
        interception_with_command(cmd, FilterMode::Filtered).with_hook_type(HookType::PostToolUse);
    append_to(&path, &item).unwrap();
    assert_no_secret_stored(&path);
}

#[test]
fn append_masks_secrets_in_the_post_hook_cwd_and_outline() {
    // A post-hook row stores the hook's `cwd` as `git_root`, and the Read outline
    // stored as `content_after` starts with the file path: both are tool input.
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);
    let secret_dir = format!("/tmp/{}/{}", fake_aws_key(), fake_github_token());
    let item = Interception::new(
        format!("Read {secret_dir}/notes.rs"),
        CommandFamily::NativeRead,
        Some(secret_dir.clone()),
        100,
        40,
        FilterMode::Filtered,
        false,
        0,
        Some("fn main() {}".to_string()),
        Some(format!(
            "[ecotokens outline] {secret_dir}/notes.rs\n  fn main (l.1)"
        )),
    )
    .with_hook_type(HookType::PostToolUse);
    append_to(&path, &item).unwrap();
    assert_no_secret_stored(&path);
    let stored = &read_from(&path).unwrap()[0];
    assert_eq!(
        stored.git_root.as_deref(),
        Some("/tmp/[AWS_KEY]/[GITHUB_TOKEN]")
    );
    assert!(stored
        .content_after
        .as_deref()
        .unwrap()
        .contains("/tmp/[AWS_KEY]/[GITHUB_TOKEN]/notes.rs"));
}

#[test]
fn append_masks_secrets_in_an_auto_rewrite_row_command() {
    // The auto-rewrite stage records a second `Rewritten` row for the same command.
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);
    let cmd = format!("cat {} {}", fake_aws_key(), fake_github_token());
    append_to(
        &path,
        &interception_with_command(cmd, FilterMode::Rewritten),
    )
    .unwrap();
    assert_no_secret_stored(&path);
}

#[test]
fn write_to_masks_secrets_in_the_stored_command() {
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);
    let cmd = format!("echo {} {}", fake_aws_key(), fake_github_token());
    ecotokens::metrics::store::write_to(
        &path,
        &[interception_with_command(cmd, FilterMode::Filtered)],
    )
    .unwrap();
    assert_no_secret_stored(&path);
}

#[test]
fn a_command_without_secrets_is_stored_unchanged() {
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);
    let cmd = "git log --oneline -n 5 -- src/metrics/store.rs".to_string();
    append_to(
        &path,
        &interception_with_command(cmd.clone(), FilterMode::Filtered),
    )
    .unwrap();
    assert_eq!(read_from(&path).unwrap()[0].command, cmd);
}

#[test]
fn opencode_agent_maps_to_opencode_hook_type() {
    assert_eq!(agent_to_hook_type_pre("opencode"), HookType::OpenCode);
    assert_eq!(agent_to_hook_type_post("opencode"), HookType::OpenCode);
    assert_eq!(HookType::OpenCode.agent_label(), "opencode");
}
