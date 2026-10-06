#[path = "common.rs"]
mod common;
use common::*;

use ecotokens::config::Settings;
use ecotokens::handoff::cli::{off, on, status};
use ecotokens::handoff::skills::are_skills_installed;
use ecotokens::handoff::store::write_handoff;
use ecotokens::handoff::Status;
use ecotokens::install::is_handoff_hook_installed;
use tempfile::TempDir;

const CWD: &str = "/home/u/proj";

struct Env {
    root: TempDir,
}

impl Env {
    fn new() -> Env {
        Env {
            root: TempDir::new().unwrap(),
        }
    }
    fn settings_path(&self) -> std::path::PathBuf {
        self.root.path().join("settings.json")
    }
    fn skills(&self) -> std::path::PathBuf {
        self.root.path().join("skills")
    }
    fn handoffs(&self) -> std::path::PathBuf {
        self.root.path().join("handoff")
    }
}

fn json(out: &str) -> serde_json::Value {
    serde_json::from_str(out).unwrap_or_else(|e| panic!("not JSON ({e}): {out}"))
}

#[test]
fn on_installs_the_hook_and_the_skills_and_enables_the_feature() {
    let env = Env::new();
    let mut s = Settings::default();
    let out = on(&mut s, &env.settings_path(), &env.skills(), None, None);
    assert_eq!(out.code, 0, "{}", out.err);
    assert!(s.handoff_enabled);
    assert!(is_handoff_hook_installed(&env.settings_path()));
    assert!(are_skills_installed(&env.skills()));
    assert!(out.out.contains("Restart Claude Code"), "{}", out.out);
}

#[test]
fn on_twice_changes_nothing() {
    let env = Env::new();
    let mut s = Settings::default();
    on(&mut s, &env.settings_path(), &env.skills(), None, None);
    let hooks = std::fs::read_to_string(env.settings_path()).unwrap();
    let skill = std::fs::read_to_string(env.skills().join("handoff").join("SKILL.md")).unwrap();
    let again = on(&mut s, &env.settings_path(), &env.skills(), None, None);
    assert_eq!(again.code, 0);
    assert_eq!(std::fs::read_to_string(env.settings_path()).unwrap(), hooks);
    assert_eq!(
        std::fs::read_to_string(env.skills().join("handoff").join("SKILL.md")).unwrap(),
        skill
    );
}

#[test]
fn on_saves_the_thresholds_it_is_given() {
    let env = Env::new();
    let mut s = Settings::default();
    let out = on(
        &mut s,
        &env.settings_path(),
        &env.skills(),
        Some(6),
        Some(2500),
    );
    assert_eq!(out.code, 0, "{}", out.err);
    assert_eq!(s.handoff_stale_hours, 6);
    assert_eq!(s.handoff_max_chars, 2500);
}

#[test]
fn on_rejects_a_size_limit_above_the_platform_cap_and_changes_nothing() {
    let env = Env::new();
    for bad in [9001, 0, 50_000] {
        let mut s = Settings::default();
        let out = on(&mut s, &env.settings_path(), &env.skills(), None, Some(bad));
        assert_eq!(out.code, 2, "{bad}");
        assert!(out.err.contains("9000"), "{}", out.err);
        assert!(!s.handoff_enabled);
        assert!(!env.settings_path().exists() && !env.skills().exists());
    }
}

#[test]
fn on_leaves_a_skill_the_user_wrote_alone_and_says_so() {
    let env = Env::new();
    let mine = env.skills().join("handoff").join("SKILL.md");
    std::fs::create_dir_all(mine.parent().unwrap()).unwrap();
    std::fs::write(&mine, "mine").unwrap();
    let mut s = Settings::default();
    let out = on(&mut s, &env.settings_path(), &env.skills(), None, None);
    assert_eq!(out.code, 0);
    assert!(out.err.contains("left untouched"), "{}", out.err);
    assert_eq!(std::fs::read_to_string(&mine).unwrap(), "mine");
}

#[test]
fn off_removes_the_hook_and_the_skills_but_not_the_saved_handoffs() {
    let env = Env::new();
    let mut s = Settings::default();
    on(&mut s, &env.settings_path(), &env.skills(), None, None);
    write_handoff(&env.handoffs(), &sample_handoff("abc123", CWD)).unwrap();
    let out = off(&mut s, &env.settings_path(), &env.skills());
    assert_eq!(out.code, 0, "{}", out.err);
    assert!(!s.handoff_enabled);
    assert!(!is_handoff_hook_installed(&env.settings_path()));
    assert!(!are_skills_installed(&env.skills()));
    assert!(env.handoffs().join("abc123.md").exists());
    assert_eq!(
        off(&mut s, &env.settings_path(), &env.skills()).code,
        0,
        "idempotent"
    );
}

#[test]
fn status_json_has_exactly_the_documented_keys() {
    let env = Env::new();
    let mut s = Settings::default();
    on(&mut s, &env.settings_path(), &env.skills(), None, None);
    write_handoff(&env.handoffs(), &sample_handoff("pending1", CWD)).unwrap();
    let mut used = sample_handoff("used1", CWD);
    used.status = Status::Consumed;
    used.consumed = Some(fixed_now());
    write_handoff(&env.handoffs(), &used).unwrap();

    let out = status(
        &s,
        &env.settings_path(),
        &env.skills(),
        &env.handoffs(),
        CWD,
        None,
        true,
        fixed_now(),
    );
    assert_eq!(out.code, 0, "{}", out.err);
    let v = json(&out.out);
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "consumed",
            "consumed_retention_hours",
            "enabled",
            "hook_installed",
            "inject_startup",
            "injections",
            "max_chars",
            "pending",
            "retention_days",
            "skills_installed",
            "stale_hours",
        ]
    );
    assert_eq!(v["enabled"], true);
    assert_eq!(v["hook_installed"], true);
    assert_eq!(v["skills_installed"], true);
    assert_eq!(v["max_chars"], 4000);
    assert_eq!(v["stale_hours"], 24);
    assert_eq!(v["retention_days"], 30);
    assert_eq!(v["inject_startup"], false);
    assert_eq!(v["pending"], 1);
    assert_eq!(v["consumed"], 1);
    for key in ["count", "chars_total", "chars_avg", "stale"] {
        assert!(v["injections"][key].is_u64(), "{key}: {v}");
    }
}

#[test]
fn status_on_a_fresh_machine_is_actionable_and_succeeds() {
    let env = Env::new();
    let out = status(
        &Settings::default(),
        &env.settings_path(),
        &env.skills(),
        &env.handoffs(),
        CWD,
        None,
        false,
        fixed_now(),
    );
    assert_eq!(out.code, 0);
    assert!(out.out.contains("handoff    : OFF"), "{}", out.out);
    assert!(out.out.contains("hook no"), "{}", out.out);
    assert!(
        out.out
            .contains("No handoff yet. Run /handoff in a Claude Code session."),
        "{}",
        out.out
    );
}

#[test]
fn status_human_output_shows_thresholds_and_counts() {
    let env = Env::new();
    let mut s = Settings::default();
    on(&mut s, &env.settings_path(), &env.skills(), None, None);
    write_handoff(&env.handoffs(), &sample_handoff("pending1", CWD)).unwrap();
    let out = status(
        &s,
        &env.settings_path(),
        &env.skills(),
        &env.handoffs(),
        CWD,
        None,
        false,
        fixed_now(),
    );
    assert!(out.out.contains("handoff    : ON"), "{}", out.out);
    assert!(out.out.contains("hook yes"), "{}", out.out);
    assert!(out.out.contains("skills yes"), "{}", out.out);
    assert!(out.out.contains("24 h"), "{}", out.out);
    assert!(out.out.contains("1 pending"), "{}", out.out);
}
