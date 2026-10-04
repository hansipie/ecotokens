use ecotokens::install::{
    install_handoff_hook, install_hook, install_session_hooks, is_handoff_hook_installed,
    is_hook_installed, uninstall_handoff_hook, uninstall_hook,
};
use tempfile::TempDir;

const COMMAND: &str = "ecotokens hook-handoff";

fn settings_with_third_party(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"other-tool"}]}]}}"#,
    )
    .unwrap();
    path
}

fn read(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn commands(v: &serde_json::Value) -> Vec<String> {
    v["hooks"]["SessionStart"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| e["hooks"][0]["command"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn install_is_idempotent_and_writes_one_matcherless_entry() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    install_handoff_hook(&path).unwrap();
    install_handoff_hook(&path).unwrap();
    assert!(is_handoff_hook_installed(&path));
    let v = read(&path);
    let entries = v["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].get("matcher").is_none(), "{}", entries[0]);
    assert_eq!(entries[0]["hooks"][0]["type"], "command");
    assert_eq!(entries[0]["hooks"][0]["command"], COMMAND);
    assert_eq!(
        entries[0]["hooks"][0]["timeout"], 5,
        "a stuck hook must not hold the session back"
    );
}

#[test]
fn install_keeps_third_party_entries_and_the_session_start_hook() {
    let dir = TempDir::new().unwrap();
    let path = settings_with_third_party(&dir);
    install_session_hooks(&path).unwrap();
    install_handoff_hook(&path).unwrap();
    install_handoff_hook(&path).unwrap();
    assert_eq!(
        commands(&read(&path)),
        ["other-tool", "ecotokens session-start", COMMAND]
    );
}

#[test]
fn uninstall_removes_only_ours_and_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let path = settings_with_third_party(&dir);
    install_session_hooks(&path).unwrap();
    install_handoff_hook(&path).unwrap();
    uninstall_handoff_hook(&path).unwrap();
    uninstall_handoff_hook(&path).unwrap();
    assert!(!is_handoff_hook_installed(&path));
    assert_eq!(
        commands(&read(&path)),
        ["other-tool", "ecotokens session-start"]
    );
}

#[test]
fn uninstall_drops_the_empty_event_key() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    install_handoff_hook(&path).unwrap();
    uninstall_handoff_hook(&path).unwrap();
    assert!(read(&path)["hooks"].get("SessionStart").is_none());
}

#[test]
fn uninstall_without_a_settings_file_is_a_no_op() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing.json");
    uninstall_handoff_hook(&path).unwrap();
    assert!(!path.exists(), "nothing was created");
}

#[test]
fn invalid_json_is_refused_rather_than_overwritten() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, "{not json").unwrap();
    assert!(install_handoff_hook(&path).is_err());
    assert!(uninstall_handoff_hook(&path).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{not json");
}

#[test]
fn the_other_settings_keys_survive() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"theme":"dark","hooks":{"PreToolUse":[]}}"#).unwrap();
    install_handoff_hook(&path).unwrap();
    uninstall_handoff_hook(&path).unwrap();
    let v = read(&path);
    assert_eq!(v["theme"], "dark");
    assert!(v["hooks"].get("PreToolUse").is_some());
}

#[test]
fn full_uninstall_also_removes_the_handoff_hook() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    let claude_json = dir.path().join(".claude.json");
    install_hook(&path, &claude_json).unwrap();
    install_handoff_hook(&path).unwrap();
    uninstall_hook(&path, &claude_json).unwrap();
    assert!(!is_hook_installed(&path));
    assert!(!is_handoff_hook_installed(&path));
}
