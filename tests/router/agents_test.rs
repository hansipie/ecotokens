use ecotokens::router::agents::{
    agent_markdown, agent_path, are_agents_installed, install_agents, remove_agents, MARKER,
};
use ecotokens::router::Size;
use tempfile::TempDir;

#[test]
fn agent_files_carry_model_and_final_line() {
    for size in Size::ALL {
        let md = agent_markdown(size);
        assert!(md.starts_with("---\n"), "frontmatter first");
        assert!(md.contains(&format!("name: {}\n", size.agent_name())));
        assert!(md.contains(&format!("model: {}\n", size.model_alias())));
        assert!(md.contains(MARKER));
        assert!(md.contains(&format!(
            "— done by {} ({})",
            size.model_label(),
            size.agent_name()
        )));
    }
}

#[test]
fn install_and_remove_are_idempotent() {
    let dir = TempDir::new().unwrap();
    let (written, kept) = install_agents(dir.path()).unwrap();
    assert_eq!(written.len(), 4);
    assert!(kept.is_empty());
    assert!(are_agents_installed(dir.path()));
    let (written, _) = install_agents(dir.path()).unwrap();
    assert_eq!(written.len(), 4);

    assert_eq!(remove_agents(dir.path()).unwrap().len(), 4);
    assert!(!are_agents_installed(dir.path()));
    assert!(remove_agents(dir.path()).unwrap().is_empty());
}

#[test]
fn user_agents_are_never_touched() {
    let dir = TempDir::new().unwrap();
    let rodin = dir.path().join("rodin.md");
    std::fs::write(&rodin, "---\nname: rodin\n---\nmine").unwrap();
    let own_tiny = agent_path(dir.path(), Size::Tiny);
    std::fs::write(&own_tiny, "---\nname: router-tiny\n---\nuser-written").unwrap();

    let (written, kept) = install_agents(dir.path()).unwrap();
    assert_eq!(written.len(), 3);
    assert_eq!(kept, vec![own_tiny.clone()]);
    assert!(std::fs::read_to_string(&own_tiny)
        .unwrap()
        .contains("user-written"));

    remove_agents(dir.path()).unwrap();
    assert!(rodin.exists());
    assert!(own_tiny.exists());
}
