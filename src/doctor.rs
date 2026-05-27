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

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub ok: bool,
    pub checks: Vec<DoctorCheck>,
}

struct DoctorPaths {
    config_path: Option<PathBuf>,
    metrics_path: Option<PathBuf>,
    claude_settings_path: PathBuf,
    gemini_settings_path: Option<PathBuf>,
    qwen_settings_path: Option<PathBuf>,
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
        claude_settings_path: default_claude_settings_path(),
        gemini_settings_path: install::default_gemini_settings_path(),
        qwen_settings_path: install::default_qwen_settings_path(),
    })
}

fn run_with_paths(paths: DoctorPaths) -> DoctorReport {
    let checks = vec![
        check_path_binary(),
        check_config(paths.config_path.as_deref()),
        check_claude_install(&paths.claude_settings_path),
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
        check_metrics(paths.metrics_path.as_deref()),
    ];
    DoctorReport {
        ok: !checks
            .iter()
            .any(|check| check.status == DoctorStatus::Error),
        checks,
    }
}

fn default_claude_settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("settings.json")
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

fn check_claude_install(path: &Path) -> DoctorCheck {
    check_agent_install(
        "Claude setup",
        Some(path),
        install::is_hook_installed,
        install::is_post_hook_installed,
        install::is_mcp_registered,
    )
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
        return DoctorCheck {
            name,
            status: DoctorStatus::Warning,
            message: "settings file not found; run ecotokens install when this agent is used"
                .to_string(),
            path: Some(path.display().to_string()),
        };
    }

    let pre = has_pre_hook(path);
    let post = has_post_hook(path);
    let mcp = has_mcp(path);
    if pre && post && mcp {
        DoctorCheck {
            name,
            status: DoctorStatus::Ok,
            message: "hooks and MCP server entry are installed".to_string(),
            path: Some(path.display().to_string()),
        }
    } else {
        DoctorCheck {
            name,
            status: DoctorStatus::Warning,
            message: format!(
                "partial setup detected: pre_hook={pre}, post_hook={post}, mcp_server={mcp}"
            ),
            path: Some(path.display().to_string()),
        }
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
            claude_settings_path: dir.path().join(".claude").join("settings.json"),
            gemini_settings_path: Some(dir.path().join(".gemini").join("settings.json")),
            qwen_settings_path: Some(dir.path().join(".qwen").join("settings.json")),
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
}
