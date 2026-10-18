#[path = "common.rs"]
mod common;
use common::temp_config_home;

use ecotokens::install::{
    are_router_session_hooks_installed, install_router_session_hooks,
    uninstall_router_session_hooks,
};
use ecotokens::install::{
    install_hook, install_prompt_hook, is_hook_installed, is_prompt_hook_installed, uninstall_hook,
    uninstall_prompt_hook,
};
use ecotokens::router::agents::{agent_path, AgentKind, MARKER};
use ecotokens::router::cli;
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

// ── Session hooks and the six agents (user story 5) ──────────────────────

fn dir_snapshot(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read(e.path()).unwrap(),
            )
        })
        .collect();
    v.sort();
    v
}

fn commands(v: &serde_json::Value, event: &str) -> Vec<String> {
    v["hooks"][event]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|h| h["hooks"][0]["command"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn on_installs_the_three_hooks_and_the_six_agents() {
    let _home = temp_config_home();
    let dir = TempDir::new().unwrap();
    let settings = dir.path().join("settings.json");
    let agents = dir.path().join("agents");
    cli::on(&settings, &agents, None).unwrap();

    let v = read(&settings);
    assert_eq!(commands(&v, "UserPromptSubmit"), ["ecotokens hook-prompt"]);
    assert_eq!(
        commands(&v, "SessionStart"),
        ["ecotokens hook-router-session start"]
    );
    assert_eq!(
        commands(&v, "SessionEnd"),
        ["ecotokens hook-router-session end"]
    );
    assert!(is_prompt_hook_installed(&settings));
    assert!(are_router_session_hooks_installed(&settings));
    for kind in AgentKind::ALL {
        let text = std::fs::read_to_string(agent_path(&agents, kind)).unwrap();
        assert!(text.contains(MARKER), "{}", kind.name());
    }
    assert_eq!(std::fs::read_dir(&agents).unwrap().count(), 6);
}

#[test]
fn a_second_on_leaves_settings_and_agents_byte_identical() {
    let _home = temp_config_home();
    let dir = TempDir::new().unwrap();
    let settings = settings_with_third_party(&dir);
    let agents = dir.path().join("agents");
    cli::on(&settings, &agents, None).unwrap();
    let settings_once = std::fs::read(&settings).unwrap();
    let agents_once = dir_snapshot(&agents);
    cli::on(&settings, &agents, None).unwrap();
    assert_eq!(std::fs::read(&settings).unwrap(), settings_once);
    assert_eq!(dir_snapshot(&agents), agents_once);
}

#[test]
fn third_party_entries_on_the_same_events_survive_on_and_off() {
    let _home = temp_config_home();
    let dir = TempDir::new().unwrap();
    let settings = dir.path().join("settings.json");
    std::fs::write(
        &settings,
        r#"{"hooks":{
            "UserPromptSubmit":[{"hooks":[{"type":"command","command":"other-prompt"}]}],
            "SessionStart":[{"hooks":[{"type":"command","command":"other-start"}]}],
            "SessionEnd":[{"hooks":[{"type":"command","command":"other-end"}]}]}}"#,
    )
    .unwrap();
    let agents = dir.path().join("agents");
    cli::on(&settings, &agents, None).unwrap();
    let v = read(&settings);
    assert_eq!(
        commands(&v, "SessionStart"),
        ["other-start", "ecotokens hook-router-session start"]
    );
    assert_eq!(
        commands(&v, "SessionEnd"),
        ["other-end", "ecotokens hook-router-session end"]
    );

    cli::off(&settings, &agents).unwrap();
    cli::off(&settings, &agents).unwrap();
    let v = read(&settings);
    assert_eq!(commands(&v, "UserPromptSubmit"), ["other-prompt"]);
    assert_eq!(commands(&v, "SessionStart"), ["other-start"]);
    assert_eq!(commands(&v, "SessionEnd"), ["other-end"]);
    assert!(!are_router_session_hooks_installed(&settings));
    assert!(!agents.exists() || std::fs::read_dir(&agents).unwrap().count() == 0);
}

#[test]
fn off_drops_event_keys_that_only_held_our_entries() {
    let _home = temp_config_home();
    let dir = TempDir::new().unwrap();
    let settings = dir.path().join("settings.json");
    let agents = dir.path().join("agents");
    cli::on(&settings, &agents, None).unwrap();
    cli::off(&settings, &agents).unwrap();
    let v = read(&settings);
    for event in ["UserPromptSubmit", "SessionStart", "SessionEnd"] {
        assert!(v["hooks"].get(event).is_none(), "{event} left behind: {v}");
    }
}

#[test]
fn off_keeps_a_user_owned_agent_of_a_managed_name() {
    let _home = temp_config_home();
    let dir = TempDir::new().unwrap();
    let settings = dir.path().join("settings.json");
    let agents = dir.path().join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    let own = agent_path(&agents, AgentKind::Reviewer);
    std::fs::write(&own, "---\nname: router-reviewer\n---\nmine").unwrap();
    cli::on(&settings, &agents, None).unwrap();
    assert!(
        std::fs::read_to_string(&own).unwrap().contains("mine"),
        "on keeps it"
    );
    cli::off(&settings, &agents).unwrap();
    assert!(
        std::fs::read_to_string(&own).unwrap().contains("mine"),
        "off keeps it"
    );
}

#[test]
fn the_session_hooks_install_and_remove_idempotently() {
    let dir = TempDir::new().unwrap();
    let path = settings_with_third_party(&dir);
    install_router_session_hooks(&path).unwrap();
    let once = std::fs::read(&path).unwrap();
    install_router_session_hooks(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), once);
    assert!(are_router_session_hooks_installed(&path));
    uninstall_router_session_hooks(&path).unwrap();
    uninstall_router_session_hooks(&path).unwrap();
    assert!(!are_router_session_hooks_installed(&path));
    let v = read(&path);
    assert_eq!(commands(&v, "UserPromptSubmit"), ["other-tool"]);
}

#[test]
fn full_uninstall_removes_the_session_hooks_too() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    let claude_json = dir.path().join(".claude.json");
    install_hook(&path, &claude_json).unwrap();
    install_router_session_hooks(&path).unwrap();
    uninstall_hook(&path, &claude_json).unwrap();
    assert!(!are_router_session_hooks_installed(&path));
    let v = read(&path);
    assert!(!commands(&v, "SessionStart")
        .iter()
        .any(|c| c.contains("hook-router-session")));
    assert!(!commands(&v, "SessionEnd")
        .iter()
        .any(|c| c.contains("hook-router-session")));
}
