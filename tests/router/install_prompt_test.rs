use ecotokens::install::{
    install_hook, install_prompt_hook, is_hook_installed, is_prompt_hook_installed, uninstall_hook,
    uninstall_prompt_hook,
};
use tempfile::TempDir;

fn settings_with_third_party(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"other-tool"}]}]}}"#,
    )
    .unwrap();
    path
}

fn read(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn install_is_idempotent_and_keeps_third_party_entries() {
    let dir = TempDir::new().unwrap();
    let path = settings_with_third_party(&dir);
    install_prompt_hook(&path, 2).unwrap();
    install_prompt_hook(&path, 4).unwrap();
    assert!(is_prompt_hook_installed(&path));
    let v = read(&path);
    let entries = v["hooks"]["UserPromptSubmit"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["hooks"][0]["command"], "other-tool");
    assert_eq!(entries[1]["hooks"][0]["command"], "ecotokens hook-prompt");
    assert_eq!(
        entries[1]["hooks"][0]["timeout"], 4,
        "timeout follows the latest install"
    );
}

#[test]
fn uninstall_removes_only_ours() {
    let dir = TempDir::new().unwrap();
    let path = settings_with_third_party(&dir);
    install_prompt_hook(&path, 2).unwrap();
    uninstall_prompt_hook(&path).unwrap();
    uninstall_prompt_hook(&path).unwrap();
    assert!(!is_prompt_hook_installed(&path));
    let v = read(&path);
    assert_eq!(v["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 1);
}

#[test]
fn uninstall_drops_the_empty_event_key() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    install_prompt_hook(&path, 2).unwrap();
    uninstall_prompt_hook(&path).unwrap();
    assert!(read(&path)["hooks"].get("UserPromptSubmit").is_none());
}

#[test]
fn full_uninstall_also_removes_the_router_hook() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    let claude_json = dir.path().join(".claude.json");
    install_hook(&path, &claude_json).unwrap();
    install_prompt_hook(&path, 2).unwrap();
    uninstall_hook(&path, &claude_json).unwrap();
    assert!(!is_hook_installed(&path));
    assert!(!is_prompt_hook_installed(&path));
}
