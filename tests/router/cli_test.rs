#[path = "common.rs"]
mod common;
use common::{
    read_config, run_ecotokens, stderr_of, stdout_of, temp_config_home, write_config, ConfigHome,
};

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
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

fn run(home: &ConfigHome, args: &[&str]) -> std::process::Output {
    run_ecotokens(home, args, None, None)
}

fn run_in(home: &ConfigHome, cwd: &Path, args: &[&str]) -> std::process::Output {
    run_ecotokens(home, args, Some(cwd), None)
}

fn json_of(out: &std::process::Output) -> Value {
    serde_json::from_str(&stdout_of(out))
        .unwrap_or_else(|e| panic!("not JSON ({e}): {}", stdout_of(out)))
}

/// Every file and directory below `dir`, as sorted relative paths.
fn listing(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            out.push(
                path.strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
            if path.is_dir() {
                walk(base, &path, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn private_add_list_remove_with_json_shapes() {
    let home = temp_config_home();
    let (_d, root) = canonical_tmp();
    let proj = mkdir(root.join("proj"));
    let p = proj.to_string_lossy().into_owned();

    let out = run(&home, &["router", "private", "add", &p, "--json"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(json_of(&out), json!({ "entry": p, "changed": true }));

    // Idempotent: a second add is a no-op that still exits 0.
    let out = run(&home, &["router", "private", "add", &p]);
    assert!(out.status.success());
    assert!(stdout_of(&out).contains("no change"), "{}", stdout_of(&out));
    assert_eq!(read_config(&home)["router_private"], json!([p]));

    let out = run(&home, &["router", "private", "list", "--json"]);
    assert_eq!(json_of(&out), json!([{ "entry": p, "state": "active" }]));

    let out = run(&home, &["router", "private", "remove", &p, "--json"]);
    assert_eq!(json_of(&out), json!({ "entry": p, "changed": true }));
    let out = run(&home, &["router", "private", "remove", &p]);
    assert!(out.status.success(), "removing an absent entry is a no-op");
    assert!(stdout_of(&out).contains("no change"));
    let out = run(&home, &["router", "private", "list", "--json"]);
    assert_eq!(json_of(&out), json!([]));
}

#[test]
fn private_add_defaults_to_the_current_folder() {
    let home = temp_config_home();
    let (_d, root) = canonical_tmp();
    let proj = mkdir(root.join("here"));
    let out = run_in(&home, &proj, &["router", "private", "add"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(
        read_config(&home)["router_private"],
        json!([proj.to_string_lossy()])
    );
}

#[test]
fn the_empty_list_renders_an_actionable_message() {
    let home = temp_config_home();
    let out = run(&home, &["router", "private", "list"]);
    assert!(out.status.success());
    assert!(
        stdout_of(&out).contains("router private add"),
        "{}",
        stdout_of(&out)
    );
}

#[test]
fn list_flags_missing_entries_and_keeps_them() {
    let home = temp_config_home();
    let (_d, root) = canonical_tmp();
    let gone = root.join("deleted");
    let g = gone.to_string_lossy().into_owned();
    run(&home, &["router", "private", "add", &g]);
    let out = run(&home, &["router", "private", "list", "--json"]);
    assert_eq!(json_of(&out), json!([{ "entry": g, "state": "missing" }]));
    let out = run(&home, &["router", "private", "add", "[unclosed"]);
    assert!(out.status.success());
    let out = run(&home, &["router", "private", "list", "--json"]);
    let list = json_of(&out);
    assert!(list
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["entry"] == "[unclosed" && e["state"] == "invalid"));
}

#[test]
fn check_answers_with_reason_and_entry_and_exits_zero_either_way() {
    let home = temp_config_home();
    let (_d, root) = canonical_tmp();
    let proj = mkdir(root.join("proj"));
    let sub = mkdir(proj.join("sub"));
    let other = mkdir(root.join("other"));
    run(
        &home,
        &["router", "private", "add", &proj.to_string_lossy()],
    );

    let out = run(
        &home,
        &[
            "router",
            "private",
            "check",
            &sub.to_string_lossy(),
            "--json",
        ],
    );
    assert!(out.status.success());
    let v = json_of(&out);
    assert_eq!(v["path"], sub.to_string_lossy().as_ref());
    assert_eq!(v["private"], true);
    assert_eq!(v["reason"], "exact");
    assert_eq!(v["entry"], proj.to_string_lossy().as_ref());

    let out = run(
        &home,
        &[
            "router",
            "private",
            "check",
            &other.to_string_lossy(),
            "--json",
        ],
    );
    assert!(out.status.success(), "exit 0 for the negative answer too");
    let v = json_of(&out);
    assert_eq!(v["private"], false);
    assert_eq!(v["reason"], "none");
    assert_eq!(v["entry"], Value::Null);

    // Human output.
    let out = run(
        &home,
        &["router", "private", "check", &sub.to_string_lossy()],
    );
    assert!(stdout_of(&out).contains("private"), "{}", stdout_of(&out));
    let out = run(
        &home,
        &["router", "private", "check", &other.to_string_lossy()],
    );
    assert!(
        stdout_of(&out).contains("not private"),
        "{}",
        stdout_of(&out)
    );
}

#[test]
fn allow_and_strict_commands() {
    let home = temp_config_home();
    let (_d, root) = canonical_tmp();
    let ok = mkdir(root.join("ok"));
    let other = mkdir(root.join("other"));
    let o = ok.to_string_lossy().into_owned();

    let out = run(&home, &["router", "allow", "add", &o, "--json"]);
    assert_eq!(json_of(&out), json!({ "entry": o, "changed": true }));
    let out = run(&home, &["router", "allow", "add", &o]);
    assert!(stdout_of(&out).contains("no change"));
    let out = run(&home, &["router", "allow", "list", "--json"]);
    assert_eq!(json_of(&out), json!([{ "entry": o, "state": "active" }]));

    let out = run(&home, &["router", "strict", "status", "--json"]);
    assert_eq!(json_of(&out), json!({ "strict": false, "allowed": 1 }));
    let out = run(&home, &["router", "strict", "on"]);
    assert!(out.status.success());
    assert_eq!(read_config(&home)["router_strict"], true);
    let out = run(&home, &["router", "strict", "status", "--json"]);
    assert_eq!(json_of(&out)["strict"], true);

    // `check` reports not-allowed under strict mode.
    let out = run(
        &home,
        &[
            "router",
            "private",
            "check",
            &other.to_string_lossy(),
            "--json",
        ],
    );
    let v = json_of(&out);
    assert_eq!(v["reason"], "not-allowed");
    assert_eq!(v["private"], false);
    assert_eq!(v["not_allowed"], true);
    let out = run(
        &home,
        &[
            "router",
            "private",
            "check",
            &ok.to_string_lossy(),
            "--json",
        ],
    );
    assert_eq!(json_of(&out)["reason"], "none");

    let out = run(&home, &["router", "strict", "off"]);
    assert!(out.status.success());
    assert_eq!(read_config(&home)["router_strict"], false);
    assert_eq!(
        read_config(&home)["router_allowed"],
        json!([o]),
        "turning strict off keeps the allow list"
    );

    let out = run(&home, &["router", "allow", "remove", &o, "--json"]);
    assert_eq!(json_of(&out), json!({ "entry": o, "changed": true }));
}

#[test]
fn list_commands_preserve_unknown_settings_keys() {
    let home = temp_config_home();
    write_config(
        &home,
        &json!({ "some_future_key": { "nested": [1, 2, 3] }, "router_enabled": false }),
    );
    let (_d, root) = canonical_tmp();
    let proj = mkdir(root.join("proj"));
    let out = run(
        &home,
        &["router", "private", "add", &proj.to_string_lossy()],
    );
    assert!(out.status.success(), "{}", stderr_of(&out));
    let config = read_config(&home);
    assert_eq!(config["some_future_key"], json!({ "nested": [1, 2, 3] }));
    assert_eq!(config["router_private"], json!([proj.to_string_lossy()]));
}

#[test]
fn nothing_is_written_inside_the_project_directory() {
    let home = temp_config_home();
    let (_d, root) = canonical_tmp();
    let proj = mkdir(root.join("proj"));
    mkdir(proj.join("src"));
    std::fs::write(proj.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    let before = listing(&proj);
    let bytes_before = std::fs::read(proj.join("src").join("main.rs")).unwrap();

    for args in [
        vec!["router", "private", "add"],
        vec!["router", "private", "list"],
        vec!["router", "private", "check"],
        vec!["router", "allow", "add"],
        vec!["router", "strict", "on"],
        vec!["router", "strict", "off"],
        vec!["router", "private", "remove"],
        vec!["router", "allow", "remove"],
    ] {
        let out = run_in(&home, &proj, &args);
        assert!(out.status.success(), "{args:?}: {}", stderr_of(&out));
    }
    assert_eq!(
        listing(&proj),
        before,
        "no file created, renamed or removed"
    );
    assert_eq!(
        std::fs::read(proj.join("src").join("main.rs")).unwrap(),
        bytes_before
    );
}

#[test]
fn try_in_a_private_folder_reports_the_gate_and_does_not_call_jev() {
    let home = temp_config_home();
    let (_d, root) = canonical_tmp();
    let proj = mkdir(root.join("proj"));
    run(
        &home,
        &["router", "private", "add", &proj.to_string_lossy()],
    );

    // No API key at all: the gate answers before the key is even needed.
    let out = run_in(&home, &proj, &["router", "try", "hello there", "--json"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let v = json_of(&out);
    assert_eq!(v[0]["decision"], "private");
    assert_eq!(v[0]["reason"], "exact");
    assert_eq!(v[0]["message_index"], 0);

    let out = run_in(&home, &proj, &["router", "try", "hello there"]);
    assert!(out.status.success());
    let text = stdout_of(&out);
    assert!(text.contains("private"), "{text}");
    assert!(text.contains("not sent"), "{text}");
}

// ── router rules (user story 3) ──────────────────────────────────────────

fn write_rules(home: &ConfigHome, text: &str) {
    std::fs::write(home.ecotokens_dir().join("router_rules.toml"), text).unwrap();
}

const USER_RULES: &str = r#"
[[rule]]
name = "payments-need-review"
route = "large"
review = "required"
hints = ["Check idempotency"]
[rule.when]
nature = ["implementation"]
risk = ["sensitive"]

[[rule]]
name = "broken"
route = "gigantic"
"#;

#[test]
fn rules_lists_the_defaults_when_there_is_no_user_file() {
    let home = temp_config_home();
    let out = run(&home, &["router", "rules", "--json"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let v = json_of(&out);
    assert_eq!(v["policy_version"], "2026.1");
    assert_eq!(v["user_file"], false);
    let rules = v["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 8);
    assert!(rules
        .iter()
        .all(|r| r["source"] == "default" && r["valid"] == true));
    assert_eq!(rules[0]["name"], "sensitive-work");
    assert_eq!(rules[7]["name"], "size-only");
    assert_eq!(rules[0]["route"], "large");
    assert_eq!(rules[0]["bound"], "at-least");
    assert_eq!(rules[0]["review"], "required");
    assert_eq!(rules[0]["approval"], "irreversible");
    assert_eq!(rules[1]["role"], "analyst");
    assert!(!rules[5]["hints"].as_array().unwrap().is_empty());
    assert_eq!(rules[1]["when"]["nature"], json!(["design"]));
    assert_eq!(rules[1]["when"]["ambiguity"], "high");

    let out = run(&home, &["router", "rules"]);
    let text = stdout_of(&out);
    assert!(text.contains("2026.1"), "{text}");
    assert!(text.contains("sensitive-work"), "{text}");
    assert!(
        text.contains("router_rules.toml"),
        "the empty state says where to put custom rules: {text}"
    );
}

#[test]
fn rules_lists_user_rules_first_and_marks_invalid_ones() {
    let home = temp_config_home();
    write_rules(&home, USER_RULES);
    let out = run(&home, &["router", "rules", "--json"]);
    assert!(out.status.success());
    let v = json_of(&out);
    assert_eq!(v["user_file"], true);
    assert!(v["policy_version"]
        .as_str()
        .unwrap()
        .starts_with("2026.1+u"));
    let rules = v["rules"].as_array().unwrap();
    assert_eq!(rules[0]["name"], "payments-need-review");
    assert_eq!(rules[0]["source"], "user");
    assert_eq!(rules[0]["valid"], true);
    assert_eq!(rules[1]["name"], "broken");
    assert_eq!(rules[1]["source"], "user");
    assert_eq!(rules[1]["valid"], false);
    assert!(rules[1]["reason"].as_str().unwrap().contains("gigantic"));
    assert_eq!(rules[2]["source"], "default");
    assert_eq!(rules.len(), 10);

    // Warnings go to stderr only, never to the JSON on stdout.
    assert!(stderr_of(&out).contains("broken"), "{}", stderr_of(&out));
    serde_json::from_str::<Value>(&stdout_of(&out)).unwrap();
}

#[test]
fn rules_human_output_names_policy_version_and_sources() {
    let home = temp_config_home();
    write_rules(&home, USER_RULES);
    let out = run(&home, &["router", "rules"]);
    assert!(out.status.success());
    let text = stdout_of(&out);
    assert!(text.contains("2026.1+u"), "{text}");
    assert!(text.contains("user"), "{text}");
    assert!(text.contains("default"), "{text}");
    assert!(text.contains("payments-need-review"), "{text}");
    assert!(text.contains("invalid"), "{text}");
    let err = stderr_of(&out);
    assert!(err.contains("broken"), "warning on stderr: {err}");
    assert!(!text.contains("warning"), "no warning on stdout: {text}");
}

#[test]
fn a_file_that_is_not_toml_is_one_warning_and_the_defaults_stay() {
    let home = temp_config_home();
    write_rules(&home, "this = = nope [[");
    let out = run(&home, &["router", "rules", "--json"]);
    assert!(out.status.success());
    let v = json_of(&out);
    assert_eq!(v["policy_version"], "2026.1");
    assert_eq!(v["rules"].as_array().unwrap().len(), 8);
    assert_eq!(stderr_of(&out).lines().count(), 1, "{}", stderr_of(&out));
}

// ── doctor ───────────────────────────────────────────────────────────────

mod doctor {
    use ecotokens::config::Settings;
    use ecotokens::doctor::{check_router, DoctorStatus};
    use ecotokens::install::{install_prompt_hook, install_router_session_hooks};
    use ecotokens::router::agents::install_agents;
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct Setup {
        _dir: TempDir,
        settings: PathBuf,
        agents: PathBuf,
        rules: PathBuf,
    }

    fn setup(prompt: bool, session: bool, agents: bool) -> Setup {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        let s = Setup {
            settings: root.join("settings.json"),
            agents: root.join("agents"),
            rules: root.join("router_rules.toml"),
            _dir: dir,
        };
        if prompt {
            install_prompt_hook(&s.settings, 2).unwrap();
        }
        if session {
            install_router_session_hooks(&s.settings).unwrap();
        }
        if agents {
            install_agents(&s.agents, 2).unwrap();
        }
        s
    }

    fn on() -> Settings {
        Settings {
            router_enabled: true,
            ..Settings::default()
        }
    }

    fn check(settings: &Settings, s: &Setup) -> ecotokens::doctor::DoctorCheck {
        check_router(
            settings,
            Some("key"),
            Some(&s.settings),
            Some(&s.agents),
            Some(&s.rules),
        )
    }

    #[test]
    fn off_is_fine() {
        let s = setup(false, false, false);
        let c = check(&Settings::default(), &s);
        assert_eq!(c.status, DoctorStatus::Ok);
        assert!(c.message.contains("off"));
    }

    #[test]
    fn a_complete_setup_reports_policy_and_location_counts_only() {
        let s = setup(true, true, true);
        let mut settings = on();
        settings.router_private = vec!["/very/secret/client".into()];
        settings.router_allowed = vec!["/other/place".into(), "/third".into()];
        let c = check(&settings, &s);
        if cfg!(feature = "jev") {
            assert_eq!(c.status, DoctorStatus::Ok, "{}", c.message);
            assert!(c.message.contains("policy 2026.1"), "{}", c.message);
            assert!(c.message.contains("0 custom"), "{}", c.message);
            assert!(
                c.message.contains("1 private") && c.message.contains("2 allowed"),
                "{}",
                c.message
            );
            assert!(
                !c.message.contains("secret"),
                "no path in the report: {}",
                c.message
            );
            assert!(!c.message.contains("/other"), "{}", c.message);
        }
    }

    #[test]
    fn missing_pieces_are_warnings_with_the_fix() {
        if !cfg!(feature = "jev") {
            return;
        }
        for (prompt, session, agents, needle) in [
            (false, true, true, "UserPromptSubmit"),
            (true, false, true, "SessionStart"),
            (true, true, false, "helper agents"),
        ] {
            let s = setup(prompt, session, agents);
            let c = check(&on(), &s);
            assert_eq!(c.status, DoctorStatus::Warning, "{needle}: {}", c.message);
            assert!(c.message.contains(needle), "{}", c.message);
            assert!(c.message.contains("ecotokens router on"), "{}", c.message);
        }
    }

    #[test]
    fn invalid_custom_rules_are_a_warning_naming_the_command() {
        if !cfg!(feature = "jev") {
            return;
        }
        let s = setup(true, true, true);
        std::fs::write(
            &s.rules,
            "[[rule]]\nname = \"broken\"\nroute = \"gigantic\"\n",
        )
        .unwrap();
        let c = check(&on(), &s);
        assert_eq!(c.status, DoctorStatus::Warning);
        assert!(
            c.message.contains("ecotokens router rules"),
            "{}",
            c.message
        );

        std::fs::write(&s.rules, "[[rule]]\nname = \"fine\"\nroute = \"large\"\n").unwrap();
        let c = check(&on(), &s);
        assert_eq!(c.status, DoctorStatus::Ok, "{}", c.message);
        assert!(c.message.contains("1 custom"), "{}", c.message);
    }

    #[test]
    fn strict_mode_with_an_empty_allow_list_is_flagged() {
        if !cfg!(feature = "jev") {
            return;
        }
        let s = setup(true, true, true);
        let mut settings = on();
        settings.router_strict = true;
        let c = check(&settings, &s);
        assert_eq!(c.status, DoctorStatus::Warning);
        assert!(c.message.contains("allow list is empty"), "{}", c.message);
    }
}

#[test]
fn strict_on_and_off_have_json_output_like_status() {
    let home = temp_config_home();
    let out = run(&home, &["router", "strict", "on", "--json"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(json_of(&out), json!({ "strict": true, "allowed": 0 }));
    let out = run(&home, &["router", "strict", "off", "--json"]);
    assert_eq!(json_of(&out), json!({ "strict": false, "allowed": 0 }));
}

#[test]
fn every_new_command_has_json_output() {
    let home = temp_config_home();
    let (_d, root) = canonical_tmp();
    let proj = mkdir(root.join("p"));
    let p = proj.to_string_lossy().into_owned();
    for args in [
        vec!["router", "rules", "--json"],
        vec!["router", "private", "add", &p, "--json"],
        vec!["router", "private", "list", "--json"],
        vec!["router", "private", "check", &p, "--json"],
        vec!["router", "private", "remove", &p, "--json"],
        vec!["router", "allow", "add", &p, "--json"],
        vec!["router", "allow", "list", "--json"],
        vec!["router", "allow", "remove", &p, "--json"],
        vec!["router", "strict", "status", "--json"],
        vec!["router", "outcome", "--json"],
        vec!["router", "outcome", "--pending", "--json"],
        vec!["router", "status", "--json"],
    ] {
        let out = run(&home, &args);
        assert!(out.status.success(), "{args:?}: {}", stderr_of(&out));
        serde_json::from_str::<Value>(&stdout_of(&out))
            .unwrap_or_else(|e| panic!("{args:?} is not JSON: {e}\n{}", stdout_of(&out)));
    }
}

#[test]
fn empty_states_say_what_to_do_next() {
    let home = temp_config_home();
    for (args, needle) in [
        (vec!["router", "private", "list"], "router private add"),
        (vec!["router", "allow", "list"], "router allow add"),
        (vec!["router", "outcome", "--pending"], "No delegated job"),
        (vec!["router", "status"], "No messages routed yet"),
        (vec!["router", "rules"], "router_rules.toml"),
    ] {
        let out = run(&home, &args);
        assert!(out.status.success(), "{args:?}");
        assert!(
            stdout_of(&out).contains(needle),
            "{args:?}: {}",
            stdout_of(&out)
        );
    }
}

#[test]
fn the_concepts_keep_one_name_across_commands() {
    let home = temp_config_home();
    let (_d, root) = canonical_tmp();
    let proj = mkdir(root.join("p"));
    let p = proj.to_string_lossy().into_owned();
    let added = stdout_of(&run(&home, &["router", "private", "add", &p]));
    assert!(added.contains("private location"), "{added}");
    let allowed = stdout_of(&run(&home, &["router", "allow", "add", &p]));
    assert!(allowed.contains("allowed location"), "{allowed}");
    let rules = stdout_of(&run(&home, &["router", "rules"]));
    assert!(rules.contains("Policy 2026.1"), "{rules}");
}
