#[path = "common.rs"]
mod common;
use common::{answers, answers_card, hook_env, installed_agents, router_settings, CountingJudge};

use std::io::Write;
use std::process::{Command, Stdio};

use ecotokens::config::Settings;
use ecotokens::jev::{Answers, JevError, StubJudge};
use ecotokens::router::agents::{agent_path, AgentKind};
use ecotokens::router::hook::{is_jev_down, mark_jev_down, process};
use ecotokens::router::{Decision, Size};
use tempfile::TempDir;

struct Fixture {
    cwd: TempDir,
    agents: TempDir,
    marker_dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Fixture {
            cwd: TempDir::new().unwrap(),
            agents: installed_agents(),
            marker_dir: TempDir::new().unwrap(),
        }
    }

    fn run(
        &self,
        prompt: &str,
        settings: &Settings,
        judge: &dyn ecotokens::jev::Judge,
    ) -> (Option<ecotokens::router::Routing>, Option<String>) {
        process(
            prompt,
            settings,
            Some(judge),
            &self.marker_dir.path().join("down"),
            &hook_env(self.cwd.path(), self.agents.path()),
        )
    }

    fn marker(&self) -> std::path::PathBuf {
        self.marker_dir.path().join("down")
    }
}

fn context(out: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(out).unwrap();
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "UserPromptSubmit");
    v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn delegated_message_injects_user_prompt_submit_context() {
    let f = Fixture::new();
    let judge = StubJudge::with_answers(answers("tiny", 0.97, 0.02));
    let (routing, out) = f.run("capital of Australia?", &router_settings(), &judge);
    assert_eq!(routing.unwrap().decision, Decision::Delegated);
    let ctx = context(&out.expect("output"));
    assert!(ctx.contains("router-tiny"), "{ctx}");
}

#[test]
fn router_off_does_nothing() {
    let f = Fixture::new();
    let judge = StubJudge::with_answers(answers("tiny", 1.0, 0.0));
    let (routing, out) = f.run("hi", &Settings::default(), &judge);
    assert!(routing.is_none() && out.is_none());
    assert_eq!(judge.call_count(), 0);
}

#[test]
fn no_judge_does_nothing() {
    let f = Fixture::new();
    let (routing, out) = process(
        "hi",
        &router_settings(),
        None,
        &f.marker(),
        &hook_env(f.cwd.path(), f.agents.path()),
    );
    assert!(routing.is_none() && out.is_none());
}

#[test]
fn unsure_or_follow_up_prints_nothing_but_is_recorded() {
    let f = Fixture::new();
    let judge = StubJudge::with_answers(answers("large", 0.4, 0.1));
    let (routing, out) = f.run("hmm", &router_settings(), &judge);
    assert_eq!(routing.unwrap().decision, Decision::SelfUnsure);
    assert!(out.is_none());
}

#[test]
fn service_failure_prints_nothing_and_pauses_jev() {
    let f = Fixture::new();
    let judge = StubJudge::failing(JevError::Timeout);
    let (routing, out) = f.run("write a post", &router_settings(), &judge);
    assert_eq!(routing.unwrap().decision, Decision::Error);
    assert!(out.is_none());
    assert!(is_jev_down(&f.marker()));

    // Next message: Jev is not asked at all.
    let (routing, out) = f.run("write a post", &router_settings(), &judge);
    assert_eq!(routing.unwrap().decision, Decision::JevDown);
    assert!(out.is_none());
    assert_eq!(judge.call_count(), 1);
}

#[test]
fn request_errors_do_not_pause_jev() {
    let f = Fixture::new();
    let judge = StubJudge::failing(JevError::BadResponse("x".into()));
    let _ = f.run("write a post", &router_settings(), &judge);
    assert!(!is_jev_down(&f.marker()));
}

#[test]
fn stale_marker_is_ignored() {
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("down");
    std::fs::write(&marker, "1000").unwrap();
    assert!(!is_jev_down(&marker));
    mark_jev_down(&marker);
    assert!(is_jev_down(&marker));
    std::fs::write(&marker, "garbage").unwrap();
    assert!(!is_jev_down(&marker));
}

// ── User story 1: guidance text ──────────────────────────────────────────

#[test]
fn guidance_names_policy_rule_agent_and_hints_in_at_most_six_lines() {
    let f = Fixture::new();
    let judge = StubJudge::with_answers(answers_card("tiny", "bug", "reversible", 0.1, 0.9, 0.05));
    let (_, out) = f.run(
        "SECRET-MESSAGE-TEXT fix the crash",
        &router_settings(),
        &judge,
    );
    let ctx = context(&out.expect("output"));
    assert!(ctx.contains("2026.1"), "{ctx}");
    assert!(ctx.contains("bounded-bug"), "{ctx}");
    assert!(ctx.contains("router-everyday"), "{ctx}");
    assert!(ctx.to_lowercase().contains("reproduce"), "{ctx}");
    assert!(ctx.lines().count() <= 6, "{ctx}");
    assert!(
        !ctx.contains("SECRET-MESSAGE-TEXT"),
        "the message never comes back: {ctx}"
    );
}

#[test]
fn the_analyst_route_says_read_only_and_no_implementation() {
    let f = Fixture::new();
    let judge = StubJudge::with_answers(answers_card(
        "large",
        "design",
        "reversible",
        0.9,
        0.2,
        0.05,
    ));
    let (routing, out) = f.run(
        "how should we structure the billing module",
        &router_settings(),
        &judge,
    );
    let routing = routing.unwrap();
    assert_eq!(routing.route.as_deref(), Some("router-analyst"));
    let ctx = context(&out.expect("output"));
    assert!(ctx.contains("router-analyst"), "{ctx}");
    assert!(ctx.contains("Read-only"), "{ctx}");
    assert!(ctx.contains("do not implement"), "{ctx}");
    assert!(ctx.lines().count() <= 6, "{ctx}");
}

#[test]
fn the_decision_is_recorded_with_card_rule_route_and_session() {
    let f = Fixture::new();
    let judge = StubJudge::with_answers(answers_card("large", "bug", "reversible", 0.1, 0.9, 0.05));
    let mut env = hook_env(f.cwd.path(), f.agents.path());
    env.session_id = Some("session-1".into());
    let (routing, _) = process(
        "fix it",
        &router_settings(),
        Some(&judge),
        &f.marker(),
        &env,
    );
    let r = routing.unwrap();
    assert_eq!(r.rule.as_deref(), Some("bounded-bug"));
    assert_eq!(r.route.as_deref(), Some("router-large"));
    assert_eq!(r.policy_version.as_deref(), Some("2026.1"));
    assert_eq!(r.session_id.as_deref(), Some("session-1"));
    assert!(r.card.is_some());
}

#[test]
fn every_failure_path_prints_nothing() {
    let f = Fixture::new();
    // Judge error.
    let judge = CountingJudge::failing(JevError::Http(500));
    let (_, out) = f.run("do the thing", &router_settings(), &judge);
    assert!(out.is_none());
    let _ = std::fs::remove_file(f.marker());

    // Malformed answers: nothing usable at all, then an unknown size.
    for bad in [Answers::default(), answers("gigantic", 0.9, 0.1)] {
        let judge = StubJudge::with_answers(bad);
        let (routing, out) = f.run("do the thing", &router_settings(), &judge);
        assert_eq!(routing.unwrap().decision, Decision::Error);
        assert!(out.is_none());
    }
}

#[test]
fn a_route_without_a_usable_helper_is_no_route_and_prints_nothing() {
    let f = Fixture::new();
    let judge = StubJudge::with_answers(answers_card("large", "bug", "reversible", 0.1, 0.9, 0.05));

    // Helper missing.
    std::fs::remove_file(agent_path(f.agents.path(), Size::Large.into())).unwrap();
    let (routing, out) = f.run("fix it", &router_settings(), &judge);
    let r = routing.unwrap();
    assert_eq!(r.decision, Decision::NoRoute);
    assert!(out.is_none());
    assert!(
        r.error
            .as_deref()
            .is_some_and(|e| e.contains("router-large")),
        "{:?}",
        r.error
    );
    assert_eq!(
        r.rule.as_deref(),
        Some("bounded-bug"),
        "the row keeps the card and rule"
    );

    // Helper shadowed by a user-owned file of the same name.
    std::fs::write(
        agent_path(f.agents.path(), Size::Large.into()),
        "---\nname: router-large\n---\nmine",
    )
    .unwrap();
    let (routing, out) = f.run("fix it", &router_settings(), &judge);
    assert_eq!(routing.unwrap().decision, Decision::NoRoute);
    assert!(out.is_none());

    // The analyst follows the same rule.
    let judge = StubJudge::with_answers(answers_card(
        "large",
        "research",
        "reversible",
        0.1,
        0.5,
        0.05,
    ));
    std::fs::remove_file(agent_path(f.agents.path(), AgentKind::Analyst)).unwrap();
    let (routing, out) = f.run("compare vendors", &router_settings(), &judge);
    assert_eq!(routing.unwrap().decision, Decision::NoRoute);
    assert!(out.is_none());
}

// ── Binary level: hostile stdin ─────────────────────────────────────────

fn run_hook(stdin: &[u8]) -> std::process::Output {
    let home = common::temp_config_home();
    let config = home.ecotokens_dir().join("config.json");
    std::fs::write(&config, r#"{"router_enabled": true}"#).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ecotokens"))
        .arg("hook-prompt")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env_remove("TYPESAFE_API_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let data = stdin.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = input.write_all(&data);
    });
    let out = child.wait_with_output().unwrap();
    let _ = writer.join();
    out
}

#[test]
fn hostile_stdin_prints_nothing_and_exits_zero() {
    for input in [
        b"".to_vec(),
        b"not json at all".to_vec(),
        b"{\"prompt\": 42}".to_vec(),
        vec![b'x'; 11 * 1024 * 1024],
    ] {
        let out = run_hook(&input);
        assert!(out.status.success(), "exit {:?}", out.status);
        assert!(
            out.stdout.is_empty(),
            "stdout: {:?}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
}

// ── User story 6: review and approval lines ──────────────────────────────

use ecotokens::router::policy::Policy;

fn guidance_with(policy: Policy, card: Answers) -> String {
    let f = Fixture::new();
    let judge = StubJudge::with_answers(card);
    let mut env = hook_env(f.cwd.path(), f.agents.path());
    env.policy = policy;
    let (routing, out) = process(
        "do the risky thing",
        &router_settings(),
        Some(&judge),
        &f.marker(),
        &env,
    );
    assert_eq!(routing.unwrap().decision, Decision::Delegated);
    context(&out.expect("output"))
}

#[test]
fn a_required_review_adds_the_reviewer_step_and_the_approval_line() {
    let ctx = guidance_with(
        Policy::default_policy(),
        answers_card("everyday", "implementation", "sensitive", 0.1, 0.9, 0.05),
    );
    assert!(ctx.contains("`router-reviewer`"), "{ctx}");
    assert!(
        ctx.contains("goal, constraints, `git diff` and test output"),
        "{ctx}"
    );
    assert!(
        ctx.contains("not its summary"),
        "the producer's own account is not the input: {ctx}"
    );
    assert!(ctx.contains("verdict unchanged"), "{ctx}");
    assert!(
        ctx.contains("does not trigger an automatic second production round"),
        "{ctx}"
    );
    assert!(
        ctx.contains("explicit approval before any irreversible action"),
        "{ctx}"
    );
    assert!(
        ctx.lines().count() <= 7,
        "at most 6 lines plus the reviewer step: {ctx}"
    );
}

#[test]
fn a_suggested_review_is_optional_and_carries_no_approval_line() {
    let policy = Policy::from_user_toml(
        "[[rule]]\nname = \"maybe-review\"\nroute = \"large\"\nreview = \"suggested\"\n[rule.when]\nnature = [\"ui\"]\n",
    );
    assert!(policy.invalid().is_empty());
    let ctx = guidance_with(
        policy,
        answers_card("everyday", "ui", "reversible", 0.1, 0.9, 0.05),
    );
    assert!(ctx.contains("`router-reviewer`"), "{ctx}");
    assert!(ctx.contains("optional"), "{ctx}");
    assert!(!ctx.contains("explicit approval"), "{ctx}");
    assert!(ctx.lines().count() <= 7, "{ctx}");
}

#[test]
fn no_review_means_neither_a_reviewer_nor_an_approval_line() {
    for card in [
        answers_card("tiny", "mechanical", "reversible", 0.1, 0.9, 0.05),
        answers_card("large", "implementation", "reversible", 0.1, 0.9, 0.05),
        answers_card("large", "bug", "reversible", 0.1, 0.9, 0.05),
    ] {
        let ctx = guidance_with(Policy::default_policy(), card);
        assert!(!ctx.contains("router-reviewer"), "{ctx}");
        assert!(!ctx.contains("approval"), "{ctx}");
        assert!(ctx.lines().count() <= 6, "{ctx}");
    }
}

#[test]
fn an_approval_without_a_review_adds_only_the_approval_line() {
    let policy = Policy::from_user_toml(
        "[[rule]]\nname = \"ask-first\"\nroute = \"large\"\napproval = \"irreversible\"\n[rule.when]\nnature = [\"ui\"]\n",
    );
    let ctx = guidance_with(
        policy,
        answers_card("everyday", "ui", "reversible", 0.1, 0.9, 0.05),
    );
    assert!(ctx.contains("explicit approval"), "{ctx}");
    assert!(!ctx.contains("router-reviewer"), "{ctx}");
}

// ── Fail-open, end to end (T054) ─────────────────────────────────────────

use ecotokens::router::hook::{run_prompt, PromptRuntime};
use ecotokens::router::stats::summarize;
use std::path::{Path, PathBuf};

struct World {
    cwd: TempDir,
    agents: TempDir,
    scratch: TempDir,
}

impl World {
    fn new() -> Self {
        World {
            cwd: TempDir::new().unwrap(),
            agents: installed_agents(),
            scratch: TempDir::new().unwrap(),
        }
    }

    fn input(&self, prompt: &str) -> String {
        serde_json::json!({
            "prompt": prompt,
            "cwd": self.cwd.path(),
            "session_id": "s-1",
        })
        .to_string()
    }

    fn marker(&self) -> PathBuf {
        self.scratch.path().join("down")
    }

    fn db(&self) -> PathBuf {
        self.scratch.path().join("router.db")
    }

    fn run(
        &self,
        raw: &str,
        settings: &Settings,
        judge: &dyn ecotokens::jev::Judge,
        db: Option<&Path>,
        policy: Policy,
        trusted: bool,
    ) -> Option<String> {
        let marker = self.marker();
        run_prompt(
            raw,
            &PromptRuntime {
                settings,
                judge: Some(judge),
                db,
                marker: &marker,
                agents_dir: Some(self.agents.path().to_path_buf()),
                policy,
                lists_trusted: trusted,
                bind_project: false,
            },
        )
    }
}

fn plain_judge() -> StubJudge {
    StubJudge::with_answers(answers("tiny", 0.97, 0.02))
}

#[test]
fn a_bad_rules_file_leaves_routing_by_size() {
    let w = World::new();
    let rules = w.scratch.path().join("router_rules.toml");
    std::fs::write(&rules, "this = = not [[ toml").unwrap();
    let policy = Policy::load_from(&rules);
    let judge = plain_judge();
    let out = w.run(
        &w.input("hello"),
        &router_settings(),
        &judge,
        Some(&w.db()),
        policy,
        true,
    );
    let ctx = context(&out.expect("still routed by the defaults"));
    assert!(ctx.contains("router-tiny"), "{ctx}");
}

#[test]
fn an_unreadable_settings_file_means_private_and_nothing_is_asked() {
    let w = World::new();
    let judge = CountingJudge::new(answers("tiny", 0.97, 0.02));
    let out = w.run(
        &w.input("hello"),
        &router_settings(),
        &judge,
        Some(&w.db()),
        Policy::default_policy(),
        false,
    );
    assert_eq!(out, None);
    assert_eq!(judge.request_count(), 0);
    let s = summarize(&w.db(), &Settings::default()).unwrap();
    assert_eq!(s.by_decision["private"], 1);
}

#[test]
fn a_statistics_database_that_cannot_be_written_never_blocks_the_injection() {
    let w = World::new();
    // The path is a directory: opening it as a database fails.
    let broken = w.scratch.path().join("broken.db");
    std::fs::create_dir(&broken).unwrap();
    let judge = plain_judge();
    let out = w.run(
        &w.input("hello"),
        &router_settings(),
        &judge,
        Some(&broken),
        Policy::default_policy(),
        true,
    );
    assert!(out.is_some(), "the guidance is still injected");
    // No database path at all.
    let judge = plain_judge();
    let out = w.run(
        &w.input("hello"),
        &router_settings(),
        &judge,
        None,
        Policy::default_policy(),
        true,
    );
    assert!(out.is_some());
}

#[cfg(unix)]
#[test]
fn a_read_only_database_never_blocks_the_injection() {
    use std::os::unix::fs::PermissionsExt;
    let w = World::new();
    let judge = plain_judge();
    w.run(
        &w.input("first"),
        &router_settings(),
        &judge,
        Some(&w.db()),
        Policy::default_policy(),
        true,
    );
    let mut perms = std::fs::metadata(w.db()).unwrap().permissions();
    perms.set_mode(0o444);
    std::fs::set_permissions(w.db(), perms).unwrap();
    let out = w.run(
        &w.input("second"),
        &router_settings(),
        &judge,
        Some(&w.db()),
        Policy::default_policy(),
        true,
    );
    assert!(out.is_some());
}

#[test]
fn a_locked_database_never_blocks_the_injection() {
    let w = World::new();
    let judge = plain_judge();
    w.run(
        &w.input("first"),
        &router_settings(),
        &judge,
        Some(&w.db()),
        Policy::default_policy(),
        true,
    );
    // Another process holds the write lock.
    let holder = rusqlite::Connection::open(w.db()).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = std::time::Instant::now();
    let out = w.run(
        &w.input("second"),
        &router_settings(),
        &judge,
        Some(&w.db()),
        Policy::default_policy(),
        true,
    );
    assert!(
        out.is_some(),
        "injected although the statistics could not be written"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "bounded wait"
    );
    holder.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn oversized_or_malformed_input_is_ignored() {
    let w = World::new();
    let judge = CountingJudge::new(answers("tiny", 0.97, 0.02));
    let huge = format!(
        r#"{{"prompt":"{}","cwd":"/"}}"#,
        "x".repeat(11 * 1024 * 1024)
    );
    for raw in [huge.as_str(), "", "garbage", r#"{"prompt": 5}"#, "[]"] {
        let out = w.run(
            raw,
            &router_settings(),
            &judge,
            Some(&w.db()),
            Policy::default_policy(),
            true,
        );
        assert_eq!(out, None, "{:?}", &raw[..raw.len().min(20)]);
    }
    assert_eq!(judge.request_count(), 0);
}

#[test]
fn jev_timeouts_and_server_errors_leave_the_message_alone() {
    for error in [
        JevError::Timeout,
        JevError::Http(500),
        JevError::Http(502),
        JevError::Transport("down".into()),
    ] {
        let w = World::new();
        let judge = StubJudge::failing(error.clone());
        let out = w.run(
            &w.input("hello"),
            &router_settings(),
            &judge,
            Some(&w.db()),
            Policy::default_policy(),
            true,
        );
        assert_eq!(out, None, "{error:?}");
        let s = summarize(&w.db(), &Settings::default()).unwrap();
        assert_eq!(s.by_decision["error"], 1, "{error:?}");
        assert!(
            ecotokens::router::hook::is_jev_down(&w.marker()),
            "{error:?}: Jev is paused"
        );
    }
}

#[test]
fn a_corrupt_settings_file_makes_the_real_hook_print_nothing_and_exit_zero() {
    let home = common::temp_config_home();
    std::fs::write(home.ecotokens_dir().join("config.json"), "{ corrupt").unwrap();
    std::fs::write(home.ecotokens_dir().join("router_rules.toml"), "= = =").unwrap();
    let cwd = TempDir::new().unwrap();
    let out = common::run_ecotokens(
        &home,
        &["hook-prompt"],
        None,
        Some(&serde_json::json!({"prompt": "hi", "cwd": cwd.path()}).to_string()),
    );
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn a_secret_in_a_custom_hint_is_masked_in_the_guidance() {
    let secret = "AKIAIOSFODNN7EXAMPLE";
    assert!(
        ecotokens::masking::mask(secret).1,
        "the sample must be a recognised secret"
    );
    let policy = Policy::from_user_toml(&format!(
        "[[rule]]\nname = \"hinted\"\nroute = \"large\"\nhints = [\"use key {secret} for staging\"]\n[rule.when]\nnature = [\"ui\"]\n"
    ));
    let ctx = guidance_with(
        policy,
        answers_card("everyday", "ui", "reversible", 0.1, 0.9, 0.05),
    );
    assert!(ctx.contains("use key"), "{ctx}");
    assert!(!ctx.contains(secret), "{ctx}");
}
