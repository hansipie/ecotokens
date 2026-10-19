use std::process::Command;
use tempfile::TempDir;

mod helpers;
use helpers::ecotokens;

fn temp_home_with_empty_metrics() -> TempDir {
    let dir = TempDir::new().unwrap();
    let config_dir = dir.path().join(".config").join("ecotokens");
    std::fs::create_dir_all(&config_dir).unwrap();
    // Create empty metrics file so metrics_path() resolves and read_from() returns []
    std::fs::write(config_dir.join("metrics.jsonl"), "").unwrap();
    dir
}

fn temp_claude_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
    dir
}

// ── T040 — end-to-end scenario ────────────────────────────────────────────────

#[test]
fn install_then_uninstall_is_clean() {
    let home = temp_claude_dir();
    let settings = home.path().join(".claude").join("settings.json");

    // Install
    let out = Command::new(ecotokens())
        .args(["install"])
        .env("HOME", home.path())
        .output()
        .expect("failed to run install");
    assert!(
        out.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        settings.exists(),
        "settings.json should be created after install"
    );

    // Verify hook is present
    let content = std::fs::read_to_string(&settings).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(
        v["hooks"]["PreToolUse"].is_array(),
        "PreToolUse hooks should be present"
    );

    // Uninstall
    let out = Command::new(ecotokens())
        .args(["uninstall"])
        .env("HOME", home.path())
        .output()
        .expect("failed to run uninstall");
    assert!(
        out.status.success(),
        "uninstall failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn filter_large_output_reduces_size() {
    // Generic filter triggers at 200 lines — create 300-line fixture
    let tmp = TempDir::new().unwrap();
    let input_file = tmp.path().join("large_output.txt");
    let lines: Vec<String> = (0..300)
        .map(|i| format!("line {i}: some content that takes space"))
        .collect();
    let input = lines.join("\n");
    std::fs::write(&input_file, &input).unwrap();

    let out = Command::new(ecotokens())
        .args(["filter", "--", "cat", input_file.to_str().unwrap()])
        .output()
        .expect("failed to run filter");
    assert!(
        out.status.success(),
        "filter should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let filtered = String::from_utf8_lossy(&out.stdout);
    assert!(
        filtered.len() < input.len(),
        "filtered output ({} bytes) should be shorter than input ({} bytes)",
        filtered.len(),
        input.len()
    );
    assert!(
        filtered.contains("[ecotokens]"),
        "should contain summary marker"
    );
}

#[test]
fn config_subcommand_shows_settings() {
    let out = Command::new(ecotokens())
        .args(["config"])
        .output()
        .expect("failed to run config");
    assert!(out.status.success(), "config should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("debug"),
        "config output should contain settings keys"
    );
}

// ── T050 — gain --history ──────────────────────────────────────────────────────

#[test]
fn gain_history_shows_three_period_labels() {
    let home = temp_home_with_empty_metrics();
    let out = Command::new(ecotokens())
        .args(["gain", "--history"])
        .env("HOME", home.path())
        .output()
        .expect("failed to run gain --history");
    assert!(
        out.status.success(),
        "gain --history should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Last 24h"),
        "output should contain 'Last 24h': {stdout}"
    );
    assert!(
        stdout.contains("Last 7 days"),
        "output should contain 'Last 7 days': {stdout}"
    );
    assert!(
        stdout.contains("Last 30 days"),
        "output should contain 'Last 30 days': {stdout}"
    );
}

#[test]
fn gain_history_json_is_valid_with_period_keys() {
    let home = temp_home_with_empty_metrics();
    let out = Command::new(ecotokens())
        .args(["gain", "--history", "--json"])
        .env("HOME", home.path())
        .output()
        .expect("failed to run gain --history --json");
    assert!(
        out.status.success(),
        "gain --history --json should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON: {stdout}");
    assert!(v["day"].is_object(), "JSON should have 'day' key: {stdout}");
    assert!(
        v["week"].is_object(),
        "JSON should have 'week' key: {stdout}"
    );
    assert!(
        v["month"].is_object(),
        "JSON should have 'month' key: {stdout}"
    );
}

#[test]
fn gain_history_and_period_are_mutually_exclusive() {
    let out = Command::new(ecotokens())
        .args(["gain", "--history", "--period", "week"])
        .output()
        .expect("failed to run");
    assert!(!out.status.success(), "should fail when both flags are set");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "should show conflict error: {stderr}"
    );
}

#[test]
fn config_json_flag_outputs_valid_json() {
    let out = Command::new(ecotokens())
        .args(["config", "--json"])
        .output()
        .expect("failed to run config --json");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(
        v.is_ok(),
        "config --json should produce valid JSON: {stdout}"
    );
}

#[test]
fn abbreviations_list_reads_separate_json_file() {
    let home = temp_home_with_empty_metrics();
    let config_dir = home.path().join(".config").join("ecotokens");
    std::fs::write(
        config_dir.join("config.json"),
        r#"{"abbreviations_enabled":true}"#,
    )
    .unwrap();
    std::fs::write(
        config_dir.join("abbreviations.json"),
        r#"{"function":"func","repository":"repo"}"#,
    )
    .unwrap();

    let out = Command::new(ecotokens())
        .args(["abbreviations", "list"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .output()
        .expect("failed to run abbreviations list");
    assert!(
        out.status.success(),
        "abbreviations list should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("abbreviations_enabled : true"));
    assert!(stdout.contains("function → func"));
    assert!(stdout.contains("repository → repo"));
}

#[test]
fn abbreviations_enable_migrates_custom_pairs_out_of_config_json() {
    let home = temp_home_with_empty_metrics();
    let config_dir = home.path().join(".config").join("ecotokens");
    std::fs::write(
        config_dir.join("config.json"),
        r#"{"abbreviations_custom":{"function":"func"}}"#,
    )
    .unwrap();

    let out = Command::new(ecotokens())
        .args(["abbreviations", "enable"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .output()
        .expect("failed to run abbreviations enable");
    assert!(
        out.status.success(),
        "abbreviations enable should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(config_dir.join("config.json")).unwrap())
            .unwrap();
    assert_eq!(config["abbreviations_enabled"], true);
    assert!(
        config.get("abbreviations_custom").is_none(),
        "custom abbreviations should no longer live in config.json: {config}"
    );

    let abbreviations: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(config_dir.join("abbreviations.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(abbreviations["function"], "func");
}

// ── Secrets on the command line never reach metrics.db ───────────────────────
//
// Built at runtime so no secret-shaped literal sits in the source.

fn fake_aws_key() -> String {
    format!("AKIA{}", "IOSFODNN7EXAMPLE")
}

fn fake_github_token() -> String {
    format!("ghp_{}", "a1B2".repeat(9))
}

/// Raw bytes of `metrics.db` plus its write-ahead log, so a row that was not
/// checkpointed yet is still searched.
fn raw_metrics_bytes(config_home: &std::path::Path) -> Vec<u8> {
    let db = config_home.join("ecotokens").join("metrics.db");
    assert!(db.exists(), "metrics.db should have been written");
    let mut bytes = std::fs::read(&db).unwrap();
    bytes.extend(std::fs::read(db.with_extension("db-wal")).unwrap_or_default());
    bytes
}

fn contains_bytes(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

fn assert_metrics_db_has_no_secret(config_home: &std::path::Path) {
    let bytes = raw_metrics_bytes(config_home);
    assert!(
        !contains_bytes(&bytes, &fake_aws_key()),
        "AWS key stored in clear in metrics.db"
    );
    assert!(
        !contains_bytes(&bytes, &fake_github_token()),
        "GitHub token stored in clear in metrics.db"
    );
    assert!(
        contains_bytes(&bytes, "[AWS_KEY]") && contains_bytes(&bytes, "[GITHUB_TOKEN]"),
        "the masked command should still be recorded"
    );
}

#[test]
fn filter_never_stores_secrets_from_the_command_line() {
    let home = temp_home_with_empty_metrics();
    let config_home = home.path().join(".config");
    let out = Command::new(ecotokens())
        .args([
            "filter",
            "--debug",
            "--",
            "echo",
            &fake_aws_key(),
            &fake_github_token(),
        ])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", &config_home)
        .output()
        .expect("failed to run filter");
    assert!(out.status.success());
    assert_metrics_db_has_no_secret(&config_home);

    // The `--debug` line echoes the command on stderr, which reaches the model.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("command=echo"), "debug line: {stderr}");
    assert!(!stderr.contains(&fake_aws_key()), "debug line: {stderr}");
    assert!(
        !stderr.contains(&fake_github_token()),
        "debug line: {stderr}"
    );
}

#[test]
fn filter_output_never_stores_secrets_from_the_command() {
    use std::io::Write;
    let home = temp_home_with_empty_metrics();
    let config_home = home.path().join(".config");
    let command = format!("echo {} {}", fake_aws_key(), fake_github_token());
    let mut child = Command::new(ecotokens())
        .args(["filter-output", "--debug", "--command", &command])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", &config_home)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to run filter-output");
    child.stdin.take().unwrap().write_all(b"hello\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_metrics_db_has_no_secret(&config_home);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains(&fake_aws_key()), "debug line: {stderr}");
    assert!(
        !stderr.contains(&fake_github_token()),
        "debug line: {stderr}"
    );
}

#[test]
fn post_hook_read_never_stores_secrets_from_the_file_path() {
    use std::io::Write;
    let home = temp_home_with_empty_metrics();
    let config_home = home.path().join(".config");
    // A file whose path carries the secrets: post-hook rows store `Read <path>`.
    let dir = home
        .path()
        .join("work")
        .join(fake_aws_key())
        .join(fake_github_token());
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("lib.rs");
    let content: String = (0..80)
        .map(|i| {
            format!(
                "pub fn function_{i}(x: u32) -> u32 {{\n    let y = x + {i};\n    y * 2\n}}\n\n"
            )
        })
        .collect();
    std::fs::write(&file, &content).unwrap();
    let payload = serde_json::json!({
        "tool_name": "Read",
        "tool_input": { "file_path": file.to_str().unwrap() },
        "tool_response": { "file": { "content": content } },
        "cwd": dir.to_str().unwrap(),
    });

    let mut child = Command::new(ecotokens())
        .args(["hook-post"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", &config_home)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to run hook-post");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_metrics_db_has_no_secret(&config_home);
}

#[test]
fn codex_post_hook_never_stores_secrets_from_the_command() {
    use std::io::Write;
    let home = temp_home_with_empty_metrics();
    let config_home = home.path().join(".config");
    let output: String = (0..300)
        .map(|i| format!("line {i}: some content that takes space\n"))
        .collect();
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": {
            "command": format!("cat {} {}", fake_aws_key(), fake_github_token())
        },
        "tool_response": output,
        "cwd": home.path().to_str().unwrap(),
    });

    let mut child = Command::new(ecotokens())
        .args(["hook-post-codex"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", &config_home)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to run hook-post-codex");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_metrics_db_has_no_secret(&config_home);
}

#[test]
fn filter_masks_secrets_on_stderr() {
    // The command's stderr is forwarded to the caller (and so to the model) as
    // well as its stdout: a secret printed there must be masked too (#127).
    // Built at runtime so no key sits in the source: AWS's documented example.
    let key = format!("AKIA{}", "IOSFODNN7EXAMPLE");
    let home = temp_home_with_empty_metrics();
    let out = Command::new(ecotokens())
        .args([
            "filter",
            "--",
            "sh",
            "-c",
            &format!("echo out; echo key={key} >&2"),
        ])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .output()
        .expect("failed to run filter");
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains(&key), "secret leaked on stderr: {stderr}");
    assert!(
        stderr.contains("key="),
        "stderr should still be forwarded: {stderr}"
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("out"));
}
