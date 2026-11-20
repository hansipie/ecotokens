use crate::{config, install, metrics};
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DoctorStatus {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub name: &'static str,
    pub status: DoctorStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
}

impl serde::Serialize for DoctorReport {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = s.serialize_struct("DoctorReport", 2)?;
        state.serialize_field("ok", &!self.has_errors())?;
        state.serialize_field("checks", &self.checks)?;
        state.end()
    }
}

struct DoctorPaths {
    config_path: Option<PathBuf>,
    metrics_path: Option<PathBuf>,
    claude_settings_path: Option<PathBuf>,
    gemini_settings_path: Option<PathBuf>,
    qwen_settings_path: Option<PathBuf>,
    pi_extension_path: Option<PathBuf>,
    /// Hermes root home; its `profiles/<name>/` homes are scanned too.
    hermes_home: Option<PathBuf>,
    codex_plugin_dir: Option<PathBuf>,
    codex_hooks_path: Option<PathBuf>,
    codex_config_path: Option<PathBuf>,
    opencode_plugin_path: Option<PathBuf>,
    /// Where the router's helper agents live.
    agents_dir: Option<PathBuf>,
    /// The router's optional custom rules file.
    router_rules_path: Option<PathBuf>,
}

impl DoctorReport {
    pub fn has_errors(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.status == DoctorStatus::Error)
    }
}

pub fn run() -> DoctorReport {
    run_with_paths(DoctorPaths {
        config_path: config::Settings::config_path(),
        metrics_path: metrics::store::metrics_path(),
        claude_settings_path: install::default_claude_settings_path(),
        gemini_settings_path: install::default_gemini_settings_path(),
        qwen_settings_path: install::default_qwen_settings_path(),
        pi_extension_path: install::default_pi_extension_path(),
        hermes_home: install::default_hermes_home(),
        codex_plugin_dir: install::default_codex_plugin_dir(),
        codex_hooks_path: install::default_codex_hooks_path(),
        codex_config_path: install::default_codex_config_path(),
        opencode_plugin_path: install::default_opencode_plugin_path(),
        agents_dir: crate::router::agents::default_agents_dir(),
        router_rules_path: crate::router::policy::Policy::user_rules_path(),
    })
}

fn run_with_paths(paths: DoctorPaths) -> DoctorReport {
    let settings = config::Settings::load();
    let checks = vec![
        check_path_binary(),
        check_config(paths.config_path.as_deref()),
        check_agent_install(
            "Claude setup",
            paths.claude_settings_path.as_deref(),
            install::is_hook_installed,
            install::is_post_hook_installed,
            install::is_mcp_registered,
        ),
        check_agent_install(
            "Gemini setup",
            paths.gemini_settings_path.as_deref(),
            install::is_gemini_hook_installed,
            install::is_gemini_post_hook_installed,
            install::is_gemini_mcp_registered,
        ),
        check_agent_install(
            "Qwen setup",
            paths.qwen_settings_path.as_deref(),
            install::is_qwen_hook_installed,
            install::is_qwen_post_hook_installed,
            install::is_qwen_mcp_registered,
        ),
        check_components(
            "Pi setup",
            &[(
                "extension",
                paths.pi_extension_path.as_deref(),
                install::is_pi_extension_installed,
            )],
        ),
        check_hermes(paths.hermes_home.as_deref()),
        check_components(
            "Codex setup",
            &[
                (
                    "plugin",
                    paths.codex_plugin_dir.as_deref(),
                    install::is_codex_plugin_installed,
                ),
                (
                    "pre_hook",
                    paths.codex_hooks_path.as_deref(),
                    install::is_codex_hook_installed,
                ),
                (
                    "post_hook",
                    paths.codex_hooks_path.as_deref(),
                    install::is_codex_post_hook_installed,
                ),
                (
                    "mcp_server",
                    paths.codex_config_path.as_deref(),
                    install::is_codex_mcp_registered,
                ),
            ],
        ),
        check_components(
            "OpenCode setup",
            &[(
                "plugin",
                paths.opencode_plugin_path.as_deref(),
                install::is_opencode_plugin_installed,
            )],
        ),
        check_auto_watch(&settings, paths.claude_settings_path.as_deref()),
        check_metrics(paths.metrics_path.as_deref()),
        check_jev(&settings, env::var(crate::jev::API_KEY_ENV).ok().as_deref()),
        check_router(
            &settings,
            env::var(crate::jev::API_KEY_ENV).ok().as_deref(),
            paths.claude_settings_path.as_deref(),
            paths.agents_dir.as_deref(),
            paths.router_rules_path.as_deref(),
        ),
        check_handoff(
            &settings,
            paths.claude_settings_path.as_deref(),
            crate::handoff::skills::default_skills_dir().as_deref(),
        ),
    ];
    DoctorReport { checks }
}

/// Flags an enabled auto-watch whose Claude Code session hooks are missing —
/// in that state SessionStart never runs, so the watcher silently never starts.
fn check_auto_watch(settings: &config::Settings, claude_settings: Option<&Path>) -> DoctorCheck {
    let path = claude_settings.map(|p| p.display().to_string());
    let (status, message) = if !settings.auto_watch {
        (DoctorStatus::Ok, "auto-watch disabled".to_string())
    } else if claude_settings.is_some_and(install::are_session_hooks_installed) {
        (
            DoctorStatus::Ok,
            "auto-watch enabled and session hooks are installed".to_string(),
        )
    } else {
        (
            DoctorStatus::Warning,
            "auto-watch is enabled but the Claude Code session hooks are missing; \
             run `ecotokens auto-watch enable`"
                .to_string(),
        )
    };
    DoctorCheck {
        name: "auto-watch",
        status,
        message,
        path,
    }
}

/// Reports whether Jev judgments are active. Never prints the API key and
/// makes no network call; a disabled or unconfigured Jev is not a problem,
/// the built-in heuristics simply stay in charge.
pub fn check_jev(settings: &config::Settings, api_key: Option<&str>) -> DoctorCheck {
    let has_key = api_key.is_some_and(|k| !k.trim().is_empty());
    let (status, message) = if !cfg!(feature = "jev") {
        (
            DoctorStatus::Ok,
            "Jev not compiled in; using built-in heuristics".to_string(),
        )
    } else if !settings.jev_enabled {
        (
            DoctorStatus::Ok,
            "Jev disabled (jev_enabled = false); using built-in heuristics".to_string(),
        )
    } else if !has_key {
        (
            DoctorStatus::Warning,
            format!(
                "jev_enabled is true but {} is not set; using built-in heuristics",
                crate::jev::API_KEY_ENV
            ),
        )
    } else {
        let url = settings
            .jev_url
            .as_deref()
            .unwrap_or(crate::jev::DEFAULT_URL);
        let line_select = if settings.jev_line_select_enabled {
            "on"
        } else {
            "off"
        };
        (
            DoctorStatus::Ok,
            format!(
                "Jev enabled ({url}); masked excerpts are sent to TypeSafe; \
                 line selection {line_select}"
            ),
        )
    };
    DoctorCheck {
        name: "Jev",
        status,
        message,
        path: None,
    }
}

/// Reports the model router state (no network call, key never printed): the
/// hooks and helper agents, the routing policy and its custom rules, and the
/// private and allowed locations. Only counts are shown, never a path.
pub fn check_router(
    settings: &config::Settings,
    api_key: Option<&str>,
    claude_settings: Option<&Path>,
    agents_dir: Option<&Path>,
    rules_path: Option<&Path>,
) -> DoctorCheck {
    let has_key = api_key.is_some_and(|k| !k.trim().is_empty());
    let hook = claude_settings.is_some_and(install::is_prompt_hook_installed);
    let session_hooks = claude_settings.is_some_and(install::are_router_session_hooks_installed);
    let agents = agents_dir.is_some_and(crate::router::agents::are_agents_installed);
    let policy = match rules_path {
        Some(path) => crate::router::policy::Policy::load_from(path),
        None => crate::router::policy::Policy::default_policy(),
    };
    let custom = policy
        .rules()
        .iter()
        .filter(|r| r.source == crate::router::policy::Source::User)
        .count();
    let invalid = policy.invalid().len() + policy.warnings().len();
    let (status, message) = if !settings.router_enabled {
        (DoctorStatus::Ok, "model router off".to_string())
    } else if !cfg!(feature = "jev") || !has_key {
        (
            DoctorStatus::Warning,
            format!(
                "model router on but {} is not set; messages are not routed",
                crate::jev::API_KEY_ENV
            ),
        )
    } else if !hook {
        (
            DoctorStatus::Warning,
            "model router on but the UserPromptSubmit hook is missing; run `ecotokens router on`"
                .to_string(),
        )
    } else if !agents {
        (
            DoctorStatus::Warning,
            "model router on but the helper agents are missing or incomplete; run \
             `ecotokens router on`"
                .to_string(),
        )
    } else if !session_hooks {
        (
            DoctorStatus::Warning,
            "model router on but the SessionStart / SessionEnd hooks are missing, so outcomes \
             are not measured; run `ecotokens router on`"
                .to_string(),
        )
    } else if invalid > 0 {
        (
            DoctorStatus::Warning,
            format!(
                "model router on; {invalid} problem(s) in the custom rules file are ignored; \
                 run `ecotokens router rules`"
            ),
        )
    } else if settings.router_strict && settings.router_allowed.is_empty() {
        (
            DoctorStatus::Warning,
            "model router on in strict mode but the allow list is empty, so no message is \
             routed; run `ecotokens router allow add`"
                .to_string(),
        )
    } else {
        (
            DoctorStatus::Ok,
            format!(
                "model router on; every message outside a private location is sent (masked) to \
                 TypeSafe to be sized; policy {} ({custom} custom rule(s)); {} private and {} \
                 allowed location(s), strict {}",
                policy.version(),
                settings.router_private.len(),
                settings.router_allowed.len(),
                if settings.router_strict { "on" } else { "off" }
            ),
        )
    };
    DoctorCheck {
        name: "Router",
        status,
        message,
        path: claude_settings.map(|p| p.display().to_string()),
    }
}

/// Reports the session handoff state: the SessionStart hook and the two skills
/// must both be there for `/handoff` and the re-injection to work.
pub fn check_handoff(
    settings: &config::Settings,
    claude_settings: Option<&Path>,
    skills_dir: Option<&Path>,
) -> DoctorCheck {
    let hook = claude_settings.is_some_and(install::is_handoff_hook_installed);
    let skills = skills_dir.is_some_and(crate::handoff::skills::are_skills_installed);
    let (status, message) = if !settings.handoff_enabled {
        (DoctorStatus::Ok, "session handoff off".to_string())
    } else if !hook {
        (
            DoctorStatus::Warning,
            "session handoff on but the SessionStart hook is missing; run `ecotokens handoff on`"
                .to_string(),
        )
    } else if !skills {
        (
            DoctorStatus::Warning,
            "session handoff on but the /handoff skills are missing; run `ecotokens handoff on`"
                .to_string(),
        )
    } else {
        (
            DoctorStatus::Ok,
            "session handoff on; hook and skills are installed".to_string(),
        )
    };
    DoctorCheck {
        name: "Handoff",
        status,
        message,
        path: claude_settings.map(|p| p.display().to_string()),
    }
}

fn check_path_binary() -> DoctorCheck {
    if find_on_path("ecotokens").is_some() {
        DoctorCheck {
            name: "PATH",
            status: DoctorStatus::Ok,
            message: "ecotokens executable is available on PATH".to_string(),
            path: None,
        }
    } else {
        DoctorCheck {
            name: "PATH",
            status: DoctorStatus::Warning,
            message: "ecotokens is not available on PATH; hooks may fail outside this shell"
                .to_string(),
            path: None,
        }
    }
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    let candidates = executable_candidates(binary);
    env::split_paths(&path_var)
        .flat_map(|dir| candidates.iter().map(move |candidate| dir.join(candidate)))
        .find(|candidate| candidate.is_file())
}

fn executable_candidates(binary: &str) -> Vec<String> {
    if cfg!(windows) {
        vec![
            format!("{binary}.exe"),
            format!("{binary}.cmd"),
            binary.to_string(),
        ]
    } else {
        vec![binary.to_string()]
    }
}

fn check_config(path: Option<&Path>) -> DoctorCheck {
    let Some(path) = path else {
        return DoctorCheck {
            name: "config",
            status: DoctorStatus::Warning,
            message: "could not resolve config directory".to_string(),
            path: None,
        };
    };
    if !path.exists() {
        return DoctorCheck {
            name: "config",
            status: DoctorStatus::Warning,
            message: "config file not found; defaults will be used".to_string(),
            path: Some(path.display().to_string()),
        };
    }
    match std::fs::read_to_string(path)
        .map_err(|err| err.to_string())
        .and_then(|data| {
            serde_json::from_str::<serde_json::Value>(&data)
                .map(|_| ())
                .map_err(|err| err.to_string())
        }) {
        Ok(()) => DoctorCheck {
            name: "config",
            status: DoctorStatus::Ok,
            message: "config file is readable JSON".to_string(),
            path: Some(path.display().to_string()),
        },
        Err(err) => DoctorCheck {
            name: "config",
            status: DoctorStatus::Error,
            message: format!("config file is not readable JSON: {err}"),
            path: Some(path.display().to_string()),
        },
    }
}

fn check_agent_install(
    name: &'static str,
    path: Option<&Path>,
    has_pre_hook: fn(&Path) -> bool,
    has_post_hook: fn(&Path) -> bool,
    has_mcp: fn(&Path) -> bool,
) -> DoctorCheck {
    let Some(path) = path else {
        return DoctorCheck {
            name,
            status: DoctorStatus::Warning,
            message: "settings path could not be resolved".to_string(),
            path: None,
        };
    };
    if !path.exists() {
        return not_installed(name, path);
    }

    let pre = has_pre_hook(path);
    let post = has_post_hook(path);
    let mcp = has_mcp(path);
    if !pre && !post && !mcp {
        not_installed(name, path)
    } else if pre && post && mcp {
        DoctorCheck {
            name,
            status: DoctorStatus::Ok,
            message: "hooks and MCP server entry are installed".to_string(),
            path: Some(path.display().to_string()),
        }
    } else {
        let status = if !pre && !post {
            DoctorStatus::Error
        } else {
            DoctorStatus::Warning
        };
        DoctorCheck {
            name,
            status,
            message: format!(
                "partial setup detected: pre_hook={pre}, post_hook={post}, mcp_server={mcp}"
            ),
            path: Some(path.display().to_string()),
        }
    }
}

/// A harness with no ecotokens piece at all is a normal state, not a fault.
fn not_installed(name: &'static str, path: &Path) -> DoctorCheck {
    DoctorCheck {
        name,
        status: DoctorStatus::Ok,
        message: "not installed".to_string(),
        path: Some(path.display().to_string()),
    }
}

/// Probe for one ecotokens piece of a harness: label, location, detector.
type Component<'a> = (&'static str, Option<&'a Path>, fn(&Path) -> bool);

/// Report a harness made of several independent pieces: all present is OK,
/// none present is "not installed", anything in between is a partial setup.
fn check_components(name: &'static str, components: &[Component<'_>]) -> DoctorCheck {
    let Some(first) = components.iter().find_map(|(_, p, _)| *p) else {
        return DoctorCheck {
            name,
            status: DoctorStatus::Warning,
            message: "install path could not be resolved".to_string(),
            path: None,
        };
    };
    let states: Vec<(&str, bool)> = components
        .iter()
        .map(|(label, p, detect)| (*label, p.is_some_and(*detect)))
        .collect();
    let path = Some(first.display().to_string());
    if states.iter().all(|(_, on)| !on) {
        return not_installed(name, first);
    }
    if states.iter().all(|(_, on)| *on) {
        let labels: Vec<&str> = states.iter().map(|(l, _)| *l).collect();
        return DoctorCheck {
            name,
            status: DoctorStatus::Ok,
            message: format!("installed ({})", labels.join(", ")),
            path,
        };
    }
    let detail: Vec<String> = states.iter().map(|(l, on)| format!("{l}={on}")).collect();
    DoctorCheck {
        name,
        status: DoctorStatus::Warning,
        message: format!("partial setup detected: {}", detail.join(", ")),
        path,
    }
}

/// Hermes keeps one plugins directory per profile, so the root home and every
/// `profiles/<name>/` home are listed: a plugin left in a profile keeps firing.
fn check_hermes(home: Option<&Path>) -> DoctorCheck {
    let name = "Hermes setup";
    let Some(home) = home else {
        return DoctorCheck {
            name,
            status: DoctorStatus::Warning,
            message: "Hermes home could not be resolved".to_string(),
            path: None,
        };
    };
    let installed: Vec<String> = install::hermes_profile_homes(home)
        .into_iter()
        .filter(|h| install::is_hermes_plugin_installed(&h.join("plugins").join("ecotokens")))
        .map(|h| match h.strip_prefix(home) {
            Ok(rel) if rel.as_os_str().is_empty() => "root".to_string(),
            Ok(rel) => rel.display().to_string(),
            Err(_) => h.display().to_string(),
        })
        .collect();
    if installed.is_empty() {
        return not_installed(name, home);
    }
    DoctorCheck {
        name,
        status: DoctorStatus::Ok,
        message: format!("plugin installed in: {}", installed.join(", ")),
        path: Some(home.display().to_string()),
    }
}

fn check_metrics(path: Option<&Path>) -> DoctorCheck {
    let Some(path) = path else {
        return DoctorCheck {
            name: "metrics database",
            status: DoctorStatus::Warning,
            message: "could not resolve metrics path".to_string(),
            path: None,
        };
    };
    match metrics::store::read_from(path) {
        Ok(_) if path.exists() => DoctorCheck {
            name: "metrics database",
            status: DoctorStatus::Ok,
            message: "metrics database is reachable".to_string(),
            path: Some(path.display().to_string()),
        },
        Ok(_) => DoctorCheck {
            name: "metrics database",
            status: DoctorStatus::Warning,
            message: "metrics database not found; it will be created after the first interception"
                .to_string(),
            path: Some(path.display().to_string()),
        },
        Err(err) => DoctorCheck {
            name: "metrics database",
            status: DoctorStatus::Error,
            message: format!("metrics database is not reachable: {err}"),
            path: Some(path.display().to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn missing_files_are_warnings_not_errors() {
        let dir = tempdir().unwrap();
        let report = run_with_paths(DoctorPaths {
            config_path: Some(dir.path().join("config.json")),
            metrics_path: Some(dir.path().join("metrics.db")),
            claude_settings_path: Some(dir.path().join(".claude").join("settings.json")),
            gemini_settings_path: Some(dir.path().join(".gemini").join("settings.json")),
            qwen_settings_path: Some(dir.path().join(".qwen").join("settings.json")),
            pi_extension_path: Some(dir.path().join(".pi").join("ecotokens.ts")),
            hermes_home: Some(dir.path().join(".hermes")),
            codex_plugin_dir: Some(dir.path().join(".codex").join("plugins").join("ecotokens")),
            codex_hooks_path: Some(dir.path().join(".codex").join("hooks.json")),
            codex_config_path: Some(dir.path().join(".codex").join("config.toml")),
            opencode_plugin_path: Some(dir.path().join("opencode").join("ecotokens.ts")),
            agents_dir: Some(dir.path().join(".claude").join("agents")),
            router_rules_path: Some(dir.path().join("router_rules.toml")),
        });

        assert!(!report.has_errors());
        assert!(report
            .checks
            .iter()
            .any(|check| { check.name == "config" && check.status == DoctorStatus::Warning }));
    }

    #[test]
    fn invalid_config_is_an_error() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::write(&config_path, "{not json").unwrap();

        let check = check_config(Some(&config_path));

        assert_eq!(check.status, DoctorStatus::Error);
    }

    #[test]
    fn no_hooks_at_all_is_an_error() {
        let dir = tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        std::fs::write(&settings, "{}").unwrap();

        let check = check_agent_install(
            "Claude setup",
            Some(&settings),
            |_| false,
            |_| false,
            |_| true,
        );

        assert_eq!(check.status, DoctorStatus::Error);
    }

    #[test]
    fn partial_hooks_is_a_warning() {
        let dir = tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        std::fs::write(&settings, "{}").unwrap();

        let check = check_agent_install(
            "Claude setup",
            Some(&settings),
            |_| true,
            |_| false,
            |_| true,
        );

        assert_eq!(check.status, DoctorStatus::Warning);
    }

    #[test]
    fn auto_watch_without_session_hooks_is_a_warning() {
        let dir = tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        std::fs::write(&settings_path, "{}").unwrap();
        let settings = config::Settings {
            auto_watch: true,
            ..Default::default()
        };

        let check = check_auto_watch(&settings, Some(&settings_path));
        assert_eq!(check.status, DoctorStatus::Warning);

        install::install_session_hooks(&settings_path).unwrap();
        let check = check_auto_watch(&settings, Some(&settings_path));
        assert_eq!(check.status, DoctorStatus::Ok);
    }

    #[test]
    fn auto_watch_disabled_is_ok_without_session_hooks() {
        let dir = tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        std::fs::write(&settings_path, "{}").unwrap();

        let check = check_auto_watch(&config::Settings::default(), Some(&settings_path));

        assert_eq!(check.status, DoctorStatus::Ok);
    }

    #[test]
    fn handoff_is_ok_when_off_and_warns_when_its_pieces_are_missing() {
        let dir = tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let skills = dir.path().join("skills");
        std::fs::write(&settings_path, "{}").unwrap();

        let off = config::Settings::default();
        assert_eq!(
            check_handoff(&off, Some(&settings_path), Some(&skills)).status,
            DoctorStatus::Ok
        );

        let on = config::Settings {
            handoff_enabled: true,
            ..Default::default()
        };
        let check = check_handoff(&on, Some(&settings_path), Some(&skills));
        assert_eq!(check.status, DoctorStatus::Warning);
        assert!(
            check.message.contains("ecotokens handoff on"),
            "{}",
            check.message
        );

        install::install_handoff_hook(&settings_path).unwrap();
        let check = check_handoff(&on, Some(&settings_path), Some(&skills));
        assert_eq!(check.status, DoctorStatus::Warning, "skills still missing");

        crate::handoff::skills::install_skills(&skills).unwrap();
        let check = check_handoff(&on, Some(&settings_path), Some(&skills));
        assert_eq!(check.status, DoctorStatus::Ok);
        assert_eq!(check.name, "Handoff");
    }

    #[test]
    fn agent_with_nothing_installed_is_ok_not_installed() {
        let dir = tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        std::fs::write(&settings, "{}").unwrap();

        let check = check_agent_install(
            "Gemini setup",
            Some(&settings),
            |_| false,
            |_| false,
            |_| false,
        );

        assert_eq!(check.status, DoctorStatus::Ok);
        assert_eq!(check.message, "not installed");
    }

    #[test]
    fn components_report_installed_partial_and_absent() {
        let dir = tempdir().unwrap();
        let p = dir.path();
        let none = check_components("X", &[("a", Some(p), |_| false), ("b", Some(p), |_| false)]);
        assert_eq!(
            (none.status, none.message.as_str()),
            (DoctorStatus::Ok, "not installed")
        );

        let all = check_components("X", &[("a", Some(p), |_| true), ("b", Some(p), |_| true)]);
        assert_eq!(
            (all.status, all.message.as_str()),
            (DoctorStatus::Ok, "installed (a, b)")
        );

        let part = check_components("X", &[("a", Some(p), |_| true), ("b", Some(p), |_| false)]);
        assert_eq!(part.status, DoctorStatus::Warning);
        assert!(part.message.contains("a=true, b=false"), "{}", part.message);
    }

    #[test]
    fn hermes_check_lists_every_profile_holding_the_plugin() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        assert_eq!(check_hermes(Some(home)).message, "not installed");

        install::install_hermes_plugin(
            &home
                .join("profiles")
                .join("iris")
                .join("plugins")
                .join("ecotokens"),
        )
        .unwrap();
        std::fs::create_dir_all(home.join("profiles").join("mike")).unwrap();
        let check = check_hermes(Some(home));
        assert_eq!(check.status, DoctorStatus::Ok);
        assert_eq!(
            check.message,
            format!(
                "plugin installed in: {}",
                Path::new("profiles").join("iris").display()
            )
        );

        install::install_hermes_plugin(&home.join("plugins").join("ecotokens")).unwrap();
        assert!(check_hermes(Some(home))
            .message
            .starts_with("plugin installed in: root, "));
    }

    #[test]
    fn ok_field_is_false_when_errors_present() {
        let report = DoctorReport {
            checks: vec![DoctorCheck {
                name: "test",
                status: DoctorStatus::Error,
                message: "oops".to_string(),
                path: None,
            }],
        };
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["ok"], false);
        assert!(report.has_errors());
    }

    #[test]
    fn ok_field_is_true_when_no_errors() {
        let report = DoctorReport {
            checks: vec![DoctorCheck {
                name: "test",
                status: DoctorStatus::Warning,
                message: "meh".to_string(),
                path: None,
            }],
        };
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["ok"], true);
        assert!(!report.has_errors());
    }
}
