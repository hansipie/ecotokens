#[path = "common.rs"]
mod common;
use common::{
    answers, hook_env, installed_agents, router_settings, run_ecotokens, settings_private,
    settings_strict, stdout_of, temp_config_home, write_config, CountingJudge,
};

use std::path::{Path, PathBuf};

use ecotokens::router::hook::{mark_jev_down, process};
use ecotokens::router::stats::{record, summarize};
use ecotokens::router::Decision;
use rusqlite::Connection;
use serde_json::json;
use tempfile::TempDir;

fn canonical_tmp() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    (dir, root)
}

fn mkdir(p: PathBuf) -> PathBuf {
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn a_private_folder_never_reaches_the_judge_and_prints_nothing() {
    let (_d, root) = canonical_tmp();
    let proj = mkdir(root.join("secret-project"));
    let agents = installed_agents();
    let marker = root.join("down");
    let judge = CountingJudge::new(answers("tiny", 0.99, 0.01));
    let settings = settings_private(&[&proj.to_string_lossy()]);

    for prompt in ["write a poem", "capital of Australia?", "/clear", ""] {
        let (routing, out) = process(
            prompt,
            &settings,
            Some(&judge),
            &marker,
            &hook_env(&proj, agents.path()),
        );
        assert_eq!(routing.unwrap().decision, Decision::Private, "{prompt:?}");
        assert!(out.is_none());
    }
    assert_eq!(judge.request_count(), 0);
}

#[test]
fn private_beats_a_down_marker_and_a_missing_judge() {
    let (_d, root) = canonical_tmp();
    let proj = mkdir(root.join("proj"));
    let agents = installed_agents();
    let marker = root.join("down");
    mark_jev_down(&marker);
    let settings = settings_private(&[&proj.to_string_lossy()]);
    // No judge at all (no API key): still recorded as private, not silently dropped.
    let (routing, out) = process(
        "hello",
        &settings,
        None,
        &marker,
        &hook_env(&proj, agents.path()),
    );
    assert_eq!(routing.unwrap().decision, Decision::Private);
    assert!(out.is_none());
}

#[test]
fn strict_mode_records_not_allowed_with_its_own_kind() {
    let (_d, root) = canonical_tmp();
    let allowed = mkdir(root.join("allowed"));
    let other = mkdir(root.join("other"));
    let agents = installed_agents();
    let marker = root.join("down");
    let judge = CountingJudge::new(answers("tiny", 0.99, 0.01));
    let settings = settings_strict(&[&allowed.to_string_lossy()]);

    let (routing, out) = process(
        "hello",
        &settings,
        Some(&judge),
        &marker,
        &hook_env(&other, agents.path()),
    );
    assert_eq!(routing.unwrap().decision, Decision::NotAllowed);
    assert!(out.is_none());
    assert_eq!(judge.request_count(), 0);

    let (routing, _) = process(
        "hello",
        &settings,
        Some(&judge),
        &marker,
        &hook_env(&allowed, agents.path()),
    );
    assert_eq!(routing.unwrap().decision, Decision::Delegated);
    assert_eq!(judge.request_count(), 1);
}

#[test]
fn an_unresolvable_working_directory_is_private() {
    let (_d, root) = canonical_tmp();
    let agents = installed_agents();
    let judge = CountingJudge::new(answers("tiny", 0.99, 0.01));
    let env = ecotokens::router::hook::HookEnv::new(None, agents.path().to_path_buf());
    let (routing, out) = process(
        "hi",
        &router_settings(),
        Some(&judge),
        &root.join("m"),
        &env,
    );
    assert_eq!(routing.unwrap().decision, Decision::Private);
    assert!(out.is_none());
    assert_eq!(judge.request_count(), 0);
}

#[test]
fn gate_rows_hold_only_a_timestamp_and_the_kind() {
    let (_d, root) = canonical_tmp();
    let db = root.join("router.db");
    for decision in [Decision::Private, Decision::NotAllowed] {
        let mut routing = ecotokens::router::Routing::without_jev(decision);
        // Even if a caller left data on the routing, the row must not keep it.
        routing.session_id = Some("session-should-not-be-stored".into());
        routing.policy_version = Some("2026.1".into());
        routing.rule = Some("size-only".into());
        routing.error = Some("some reason".into());
        record(&db, &routing).unwrap();
    }
    let conn = Connection::open(&db).unwrap();
    let leaking: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM router_decisions
             WHERE size IS NOT NULL OR confidence IS NOT NULL OR followup_prob IS NOT NULL
                OR input_tokens != 0 OR output_tokens != 0 OR latency_ms != 0
                OR session_id IS NOT NULL OR nature IS NOT NULL OR risk IS NOT NULL
                OR ambiguity IS NOT NULL OR verifiability IS NOT NULL OR rule IS NOT NULL
                OR route IS NOT NULL OR raised IS NOT NULL OR policy_version IS NOT NULL
                OR models IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(leaking, 0);
    let kinds: Vec<String> = conn
        .prepare("SELECT decision FROM router_decisions ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(kinds, ["private", "not_allowed"]);
    // Gate decisions are not Jev requests and carry no policy version.
    let s = summarize(&db, &ecotokens::config::Settings::default()).unwrap();
    assert_eq!(s.messages, 2);
    assert_eq!(s.jev_requests, 0);
    assert!(s.by_policy.is_empty());
}

// ── Binary level: nothing leaves a trace (SC-003) ───────────────────────

const PROMPT_MARKER: &str = "TOP-SECRET-PROMPT-TEXT-4711";

fn hook_input(prompt: &str, cwd: &Path, session: &str) -> String {
    json!({ "prompt": prompt, "cwd": cwd, "session_id": session }).to_string()
}

fn byte_search(dir: &Path, needles: &[String], skip: &[&str]) -> Vec<String> {
    let mut hits = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() || skip.contains(&name.as_str()) {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap_or_default();
        for needle in needles {
            if bytes.windows(needle.len()).any(|w| w == needle.as_bytes()) {
                hits.push(format!("{name} contains {needle}"));
            }
        }
    }
    hits
}

#[test]
fn fifty_private_messages_leave_no_trace_but_a_count() {
    let home = temp_config_home();
    let (_d, root) = canonical_tmp();

    // Folders the fifty messages come from: exact, subdirectory, symlink,
    // second worktree, pattern match.
    let proj = mkdir(root.join("acme-secret-client"));
    let sub = mkdir(proj.join("src").join("deep"));
    let clients = mkdir(root.join("clients").join("globex-hidden"));
    let main = mkdir(root.join("main-repo-name"));
    let admin = mkdir(main.join(".git").join("worktrees").join("wt"));
    std::fs::write(admin.join("commondir"), "../..\n").unwrap();
    let wt = mkdir(root.join("second-checkout"));
    std::fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
    #[cfg(unix)]
    let link = {
        let link = root.join("shortcut-to-acme");
        std::os::unix::fs::symlink(&sub, &link).unwrap();
        link
    };
    #[cfg(not(unix))]
    let link = sub.clone();
    let vanished = root.join("vanished-directory");

    write_config(
        &home,
        &json!({
            "router_enabled": true,
            "router_private": [
                proj.to_string_lossy(),
                format!("{}/clients/*", root.display()),
                main.to_string_lossy(),
            ],
            // A key and an unreachable URL: any attempt to ask Jev would fail loudly.
            "jev_url": "http://127.0.0.1:9/never"
        }),
    );

    let cwds = [proj.clone(), sub, link, wt, clients, vanished];
    for i in 0..50 {
        let cwd = &cwds[i % cwds.len()];
        let out = run_hook_with_key(&home, &hook_input(PROMPT_MARKER, cwd, "sess-private-9"));
        assert!(out.status.success());
        assert!(
            stdout_of(&out).is_empty(),
            "nothing is printed: {}",
            stdout_of(&out)
        );
    }

    // The database holds fifty bare rows.
    let db = home.router_db();
    let conn = Connection::open(&db).unwrap();
    let (total, private): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), SUM(decision = 'private') FROM router_decisions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((total, private), (50, 50));
    let jev_calls: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'jev_calls'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    if jev_calls > 0 {
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM jev_calls", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0, "no Jev call is recorded for a private folder");
    }
    drop(conn);

    // Byte search of router.db, its WAL and its shared memory file.
    let needles: Vec<String> = [
        "acme-secret-client",
        "globex-hidden",
        "main-repo-name",
        "second-checkout",
        "shortcut-to-acme",
        "vanished-directory",
        PROMPT_MARKER,
        "sess-private-9",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let hits = byte_search(&home.ecotokens_dir(), &needles, &["config.json"]);
    assert!(hits.is_empty(), "traces found: {hits:?}");

    // `router status --json` reports the count.
    let out = run_ecotokens(&home, &["router", "status", "--json"], None, None);
    let status: serde_json::Value = serde_json::from_str(&stdout_of(&out)).unwrap();
    assert_eq!(status["private_skipped"], 50);
    assert_eq!(status["not_allowed_skipped"], 0);
}

fn run_hook_with_key(home: &common::ConfigHome, input: &str) -> std::process::Output {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new(env!("CARGO_BIN_EXE_ecotokens"))
        .arg("hook-prompt")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("TYPESAFE_API_KEY", "dummy-key-for-the-test")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn a_routed_message_never_leaves_its_text_or_its_folder_in_the_database() {
    use ecotokens::router::hook::{run_prompt, PromptRuntime};
    use ecotokens::router::policy::Policy;

    let (_d, root) = canonical_tmp();
    let work = mkdir(root.join("visible-project-name"));
    let agents = installed_agents();
    let db = root.join("router.db");
    let marker = root.join("down");
    let judge = CountingJudge::new(answers("tiny", 0.97, 0.02));
    let settings = router_settings();
    let raw = json!({
        "prompt": "message with sk-ant-api03-SECRETSECRETSECRET and PROMPT-TEXT-1234",
        "cwd": work,
        "session_id": "opaque-session",
    })
    .to_string();
    let out = run_prompt(
        &raw,
        &PromptRuntime {
            settings: &settings,
            judge: Some(&judge),
            db: Some(&db),
            marker: &marker,
            agents_dir: Some(agents.path().to_path_buf()),
            policy: Policy::default_policy(),
            lists_trusted: true,
            bind_project: false,
        },
    );
    let guidance = out.expect("delegated");
    assert!(
        !guidance.contains("PROMPT-TEXT-1234"),
        "the guidance never repeats the message"
    );

    let needles: Vec<String> = ["PROMPT-TEXT-1234", "SECRETSECRET", "visible-project-name"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let hits = byte_search(&root, &needles, &[]);
    assert!(hits.is_empty(), "traces found: {hits:?}");
    // The routed row itself is there, with the opaque session id and no text.
    let s =
        ecotokens::router::stats::summarize(&db, &ecotokens::config::Settings::default()).unwrap();
    assert_eq!(s.by_decision["delegated"], 1);
}
