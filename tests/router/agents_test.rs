use ecotokens::router::agents::{
    agent_markdown, agent_path, are_agents_installed, install_agents, is_available, remove_agents,
    AgentKind, MARKER,
};
use ecotokens::router::Size;
use tempfile::TempDir;

const DEFAULT_ATTEMPTS: u32 = 2;

/// Value of the frontmatter key, if present.
fn frontmatter<'a>(md: &'a str, key: &str) -> Option<&'a str> {
    let body = md.strip_prefix("---\n")?;
    let end = body.find("\n---\n")?;
    body[..end]
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key}: ")))
}

fn tools(md: &str) -> Option<Vec<String>> {
    frontmatter(md, "tools").map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
}

#[test]
fn exactly_six_agents_are_managed() {
    let names: Vec<_> = AgentKind::ALL.iter().map(|k| k.name()).collect();
    assert_eq!(
        names,
        [
            "router-tiny",
            "router-everyday",
            "router-large",
            "router-hardest",
            "router-analyst",
            "router-reviewer"
        ]
    );
}

#[test]
fn producer_files_carry_model_and_final_line() {
    for size in Size::ALL {
        let md = agent_markdown(size.into(), DEFAULT_ATTEMPTS);
        assert!(md.starts_with("---\n"), "frontmatter first");
        assert!(md.contains(&format!("name: {}\n", size.agent_name())));
        assert!(md.contains(&format!("model: {}\n", size.model_alias())));
        assert!(md.contains(MARKER));
        assert!(md.contains(&format!(
            "— done by {} ({})",
            size.model_label(),
            size.agent_name()
        )));
        assert_eq!(tools(&md), None, "producers keep their tools inherited");
    }
}

#[test]
fn every_file_carries_the_marker_and_the_right_model() {
    for kind in AgentKind::ALL {
        let md = agent_markdown(kind, DEFAULT_ATTEMPTS);
        assert!(md.contains(MARKER), "{}", kind.name());
        assert!(md.contains(&format!("name: {}\n", kind.name())));
    }
    for kind in [AgentKind::Analyst, AgentKind::Reviewer] {
        let md = agent_markdown(kind, DEFAULT_ATTEMPTS);
        assert_eq!(frontmatter(&md, "model"), Some(Size::Large.model_alias()));
    }
}

#[test]
fn install_and_remove_are_idempotent() {
    let dir = TempDir::new().unwrap();
    let (written, kept) = install_agents(dir.path(), DEFAULT_ATTEMPTS).unwrap();
    assert_eq!(written.len(), 6);
    assert!(kept.is_empty());
    assert!(are_agents_installed(dir.path()));

    let snapshot = |d: &std::path::Path| -> Vec<(String, String)> {
        let mut v: Vec<_> = std::fs::read_dir(d)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (
                    e.file_name().to_string_lossy().into_owned(),
                    std::fs::read_to_string(e.path()).unwrap(),
                )
            })
            .collect();
        v.sort();
        v
    };
    let first = snapshot(dir.path());
    let (written, _) = install_agents(dir.path(), DEFAULT_ATTEMPTS).unwrap();
    assert_eq!(written.len(), 6);
    assert_eq!(
        snapshot(dir.path()),
        first,
        "second run leaves bytes identical"
    );

    assert_eq!(remove_agents(dir.path()).unwrap().len(), 6);
    assert!(!are_agents_installed(dir.path()));
    assert!(remove_agents(dir.path()).unwrap().is_empty());
}

#[test]
fn user_agents_are_never_touched() {
    let dir = TempDir::new().unwrap();
    let rodin = dir.path().join("rodin.md");
    std::fs::write(&rodin, "---\nname: rodin\n---\nmine").unwrap();
    let own_tiny = agent_path(dir.path(), Size::Tiny.into());
    std::fs::write(&own_tiny, "---\nname: router-tiny\n---\nuser-written").unwrap();
    let own_reviewer = agent_path(dir.path(), AgentKind::Reviewer);
    std::fs::write(&own_reviewer, "---\nname: router-reviewer\n---\nmine too").unwrap();

    let (written, kept) = install_agents(dir.path(), DEFAULT_ATTEMPTS).unwrap();
    assert_eq!(written.len(), 4);
    assert_eq!(kept, vec![own_tiny.clone(), own_reviewer.clone()]);
    assert!(std::fs::read_to_string(&own_tiny)
        .unwrap()
        .contains("user-written"));

    remove_agents(dir.path()).unwrap();
    assert!(rodin.exists());
    assert!(own_tiny.exists());
    assert!(own_reviewer.exists());
}

#[test]
fn removal_deletes_only_marked_files() {
    let dir = TempDir::new().unwrap();
    install_agents(dir.path(), DEFAULT_ATTEMPTS).unwrap();
    let analyst = agent_path(dir.path(), AgentKind::Analyst);
    std::fs::write(&analyst, "---\nname: router-analyst\n---\nuser took over").unwrap();
    let removed = remove_agents(dir.path()).unwrap();
    assert_eq!(removed.len(), 5);
    assert!(analyst.exists());
}

#[test]
fn availability_needs_a_managed_file_not_shadowed_by_a_user_file() {
    let dir = TempDir::new().unwrap();
    for kind in AgentKind::ALL {
        assert!(!is_available(dir.path(), kind), "nothing installed yet");
    }
    install_agents(dir.path(), DEFAULT_ATTEMPTS).unwrap();
    for kind in AgentKind::ALL {
        assert!(is_available(dir.path(), kind), "{}", kind.name());
    }
    std::fs::write(
        agent_path(dir.path(), AgentKind::Reviewer),
        "---\nname: router-reviewer\n---\nuser",
    )
    .unwrap();
    assert!(!is_available(dir.path(), AgentKind::Reviewer));
    assert!(is_available(dir.path(), AgentKind::Analyst));
}

const WRITE_CAPABLE: [&str; 4] = ["Bash", "Write", "Edit", "NotebookEdit"];

#[test]
fn analyst_and_reviewer_are_read_only_by_their_tool_allowlist() {
    for kind in [AgentKind::Analyst, AgentKind::Reviewer] {
        let md = agent_markdown(kind, DEFAULT_ATTEMPTS);
        let list = tools(&md).unwrap_or_else(|| panic!("{} must declare tools", kind.name()));
        assert!(!list.is_empty(), "{}", kind.name());
        for forbidden in WRITE_CAPABLE {
            assert!(
                !list.iter().any(|t| t == forbidden),
                "{} lists {forbidden}",
                kind.name()
            );
        }
        for required in ["Read", "Grep", "Glob"] {
            assert!(
                list.iter().any(|t| t == required),
                "{} lacks {required}",
                kind.name()
            );
        }
        assert!(
            list.iter().any(|t| t == "mcp__ecotokens__ecotokens_search"),
            "ecotokens read tools are allowed"
        );
        assert!(
            !md.contains("disallowedTools"),
            "an allowlist is used, never a denylist"
        );
    }
}

#[test]
fn only_the_analyst_may_use_the_web_and_the_reviewer_never_does() {
    let analyst = tools(&agent_markdown(AgentKind::Analyst, DEFAULT_ATTEMPTS)).unwrap();
    assert!(analyst.iter().any(|t| t == "WebSearch"));
    assert!(analyst.iter().any(|t| t == "WebFetch"));
    let reviewer = tools(&agent_markdown(AgentKind::Reviewer, DEFAULT_ATTEMPTS)).unwrap();
    assert!(
        !reviewer.iter().any(|t| t.starts_with("Web")),
        "{reviewer:?}"
    );
}

#[test]
fn no_tool_name_of_the_write_family_appears_anywhere_in_the_frontmatter() {
    for kind in [AgentKind::Analyst, AgentKind::Reviewer] {
        let md = agent_markdown(kind, DEFAULT_ATTEMPTS);
        let front = md.split("\n---\n").next().unwrap();
        for forbidden in WRITE_CAPABLE {
            assert!(
                !front.contains(forbidden),
                "{} frontmatter has {forbidden}",
                kind.name()
            );
        }
    }
}

// ── Standing rules (user story 4) ────────────────────────────────────────

#[test]
fn every_body_carries_the_standing_rules() {
    for kind in AgentKind::ALL {
        let md = agent_markdown(kind, DEFAULT_ATTEMPTS);
        let name = kind.name();
        assert!(md.contains("after 2 attempts"), "{name}: attempt limit");
        assert!(
            md.contains("ecotokens router dossier"),
            "{name}: dossier command"
        );
        assert!(md.contains("environment"), "{name}: environment failures");
        assert!(
            md.contains("network") && md.contains("rate limit"),
            "{name}"
        );
        assert!(md.contains("missing tool"), "{name}");
        assert!(md.contains("not a reasoning failure"), "{name}");
        assert!(md.contains("contradict"), "{name}: contradictory criteria");
        assert!(md.contains("ask for clarification"), "{name}");
        assert!(
            md.contains("Never raise your own tools or permissions after a failure"),
            "{name}: permissions"
        );
    }
}

#[test]
fn the_attempt_limit_follows_the_setting() {
    for attempts in [1, 2, 5] {
        let md = agent_markdown(AgentKind::Producer(Size::Large), attempts);
        assert!(
            md.contains(&format!("after {attempts} attempt")),
            "{attempts}"
        );
    }
    let md = agent_markdown(AgentKind::Producer(Size::Large), 3);
    assert!(!md.contains("after 2 attempts"));
}

#[test]
fn reinstalling_after_the_setting_changes_rewrites_only_managed_files() {
    let dir = TempDir::new().unwrap();
    install_agents(dir.path(), 2).unwrap();
    let own = agent_path(dir.path(), AgentKind::Reviewer);
    std::fs::write(&own, "---\nname: router-reviewer\n---\nuser file").unwrap();

    let (written, kept) = install_agents(dir.path(), 4).unwrap();
    assert_eq!(written.len(), 5);
    assert_eq!(kept, vec![own.clone()]);
    let producer = std::fs::read_to_string(agent_path(dir.path(), Size::Tiny.into())).unwrap();
    assert!(producer.contains("after 4 attempts"));
    assert!(std::fs::read_to_string(&own).unwrap().contains("user file"));
}

#[test]
fn read_only_helpers_hand_the_dossier_to_the_main_session() {
    // They cannot run commands, so they must not be told to.
    for kind in [AgentKind::Analyst, AgentKind::Reviewer] {
        let md = agent_markdown(kind, DEFAULT_ATTEMPTS);
        assert!(md.contains("you cannot run commands"), "{}", kind.name());
        assert!(
            md.contains("ask the main session to run `ecotokens router dossier`"),
            "{}",
            kind.name()
        );
    }
    let md = agent_markdown(AgentKind::Producer(Size::Tiny), DEFAULT_ATTEMPTS);
    assert!(md.contains("run `ecotokens router dossier`"));
}

// ── Analyst and reviewer bodies (user story 6) ───────────────────────────

#[test]
fn the_reviewer_is_read_only_and_judges_from_the_evidence_it_is_given() {
    let md = agent_markdown(AgentKind::Reviewer, DEFAULT_ATTEMPTS);
    let body = md.split(MARKER).nth(1).unwrap();
    assert!(body.contains("read-only"), "{body}");
    assert!(body.contains("diff and test output"), "{body}");
    assert!(
        body.contains("rather than the producer's account"),
        "{body}"
    );
    assert!(body.contains("fresh context"), "{body}");
    assert!(body.contains("verdict"), "{body}");
    assert!(
        body.contains("do not edit"),
        "a verdict only, no fixes: {body}"
    );
    assert!(
        body.contains("approve") && body.contains("reject"),
        "{body}"
    );
}

#[test]
fn the_analyst_delivers_options_and_does_not_implement() {
    let md = agent_markdown(AgentKind::Analyst, DEFAULT_ATTEMPTS);
    let body = md.split(MARKER).nth(1).unwrap();
    assert!(body.contains("constraints"), "{body}");
    assert!(body.contains("options"), "{body}");
    assert!(body.contains("trade-offs"), "{body}");
    assert!(body.contains("acceptance criteria"), "{body}");
    assert!(body.contains("do not implement"), "{body}");
    assert!(
        body.contains("primary sources"),
        "research is sourced: {body}"
    );
}
