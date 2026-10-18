#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use ecotokens::config::Settings;
use ecotokens::jev::{Answer, Answers, ChoiceAnswer, JevError, Judge, Questions, StubJudge, Usage};
use ecotokens::router::agents;
use ecotokens::router::hook::HookEnv;
use serde_json::Value;
use tempfile::TempDir;

fn choice(value: &str, confidence: f64) -> Answer {
    Answer::Choice(ChoiceAnswer {
        choice: value.to_string(),
        probabilities: HashMap::from([(value.to_string(), confidence)]),
        confidence,
    })
}

/// Answers for a plain job of `size` (size confidence `confidence`, follow-up
/// probability `followup`): a reversible implementation, clearly described,
/// easy to verify, every card answer sure. Routes by size under the default
/// policy, like the router before the task card.
pub fn answers(size: &str, confidence: f64, followup: f64) -> Answers {
    answers_card_with(
        size,
        "implementation",
        "reversible",
        0.1,
        0.9,
        followup,
        Confidences {
            size: confidence,
            ..Confidences::default()
        },
    )
}

/// Per-answer confidences of a six-question card.
#[derive(Debug, Clone, Copy)]
pub struct Confidences {
    pub size: f64,
    pub nature: f64,
    pub risk: f64,
}

impl Default for Confidences {
    fn default() -> Self {
        Confidences {
            size: 0.9,
            nature: 0.9,
            risk: 0.9,
        }
    }
}

/// Answers for the six questions (size, needs_conversation, nature, risk,
/// ambiguity, verifiability), every choice at 0.9 confidence.
pub fn answers_card(
    size: &str,
    nature: &str,
    risk: &str,
    ambiguity: f64,
    verifiability: f64,
    followup: f64,
) -> Answers {
    answers_card_with(
        size,
        nature,
        risk,
        ambiguity,
        verifiability,
        followup,
        Confidences::default(),
    )
}

pub fn answers_card_with(
    size: &str,
    nature: &str,
    risk: &str,
    ambiguity: f64,
    verifiability: f64,
    followup: f64,
    conf: Confidences,
) -> Answers {
    let mut a = Answers::default();
    a.insert("size", choice(size, conf.size));
    a.insert("nature", choice(nature, conf.nature));
    a.insert("risk", choice(risk, conf.risk));
    a.insert("needs_conversation", Answer::Noul(followup));
    a.insert("ambiguity", Answer::Noul(ambiguity));
    a.insert("verifiability", Answer::Noul(verifiability));
    a
}

/// A plain, reversible, verifiable implementation job of the given size.
pub fn plain_card(size: &str) -> Answers {
    answers_card(size, "implementation", "reversible", 0.1, 0.9, 0.05)
}

/// A judge that always returns the same answers and records how many
/// requests it got and which question ids each one carried.
pub struct CountingJudge {
    inner: StubJudge,
    requests: Mutex<Vec<Vec<String>>>,
}

impl CountingJudge {
    pub fn new(answers: Answers) -> Self {
        CountingJudge {
            inner: StubJudge::with_answers(answers),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn failing(err: JevError) -> Self {
        CountingJudge {
            inner: StubJudge::failing(err),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    pub fn question_ids(&self) -> Vec<Vec<String>> {
        self.requests.lock().unwrap().clone()
    }
}

impl Judge for CountingJudge {
    fn ask(
        &self,
        state: Value,
        questions: Questions,
        timeout: Duration,
    ) -> Result<Answers, JevError> {
        self.requests
            .lock()
            .unwrap()
            .push(questions.keys().cloned().collect());
        self.inner.ask(state, questions, timeout)
    }

    fn ask_with_usage(
        &self,
        state: Value,
        questions: Questions,
        timeout: Duration,
    ) -> Result<(Answers, Option<Usage>), JevError> {
        self.ask(state, questions, timeout).map(|a| (a, None))
    }
}

pub fn router_settings() -> Settings {
    Settings {
        router_enabled: true,
        ..Settings::default()
    }
}

/// Router settings with private locations.
pub fn settings_private(entries: &[&str]) -> Settings {
    Settings {
        router_private: entries.iter().map(|s| s.to_string()).collect(),
        ..router_settings()
    }
}

/// Router settings in strict mode with an allow list.
pub fn settings_strict(allowed: &[&str]) -> Settings {
    Settings {
        router_strict: true,
        router_allowed: allowed.iter().map(|s| s.to_string()).collect(),
        ..router_settings()
    }
}

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// An isolated config home: `XDG_CONFIG_HOME` and `HOME` point into a temp
/// dir for as long as the value lives. Environment variables are process
/// wide, so holders are serialised.
pub struct ConfigHome {
    pub dir: TempDir,
    _guard: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl ConfigHome {
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// `<home>/ecotokens`, where the settings, `router.db` and rules live.
    pub fn ecotokens_dir(&self) -> PathBuf {
        self.dir.path().join(".config").join("ecotokens")
    }

    pub fn router_db(&self) -> PathBuf {
        self.ecotokens_dir().join("router.db")
    }
}

impl Drop for ConfigHome {
    fn drop(&mut self) {
        for (key, value) in self.saved.drain(..) {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}

pub fn temp_config_home() -> ConfigHome {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let saved = ["XDG_CONFIG_HOME", "HOME", "TYPESAFE_API_KEY"]
        .into_iter()
        .map(|k| (k, std::env::var_os(k)))
        .collect();
    std::env::set_var("HOME", dir.path());
    std::env::set_var("XDG_CONFIG_HOME", dir.path().join(".config"));
    std::fs::create_dir_all(dir.path().join(".config").join("ecotokens")).unwrap();
    ConfigHome {
        dir,
        _guard: guard,
        saved,
    }
}

/// A temp agents directory with the six managed helpers installed.
pub fn installed_agents() -> TempDir {
    let dir = TempDir::new().unwrap();
    agents::install_agents(dir.path(), 2).unwrap();
    dir
}

/// Hook environment: a working directory plus an agents directory.
pub fn hook_env(cwd: &Path, agents_dir: &Path) -> HookEnv {
    HookEnv::new(Some(cwd.to_path_buf()), agents_dir.to_path_buf())
}

/// Runs the real binary in the isolated config home (never touches the
/// user's own). `stdin` is written to its standard input.
pub fn run_ecotokens(
    home: &ConfigHome,
    args: &[&str],
    cwd: Option<&Path>,
    stdin: Option<&str>,
) -> std::process::Output {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ecotokens"));
    cmd.args(args)
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env_remove("TYPESAFE_API_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let mut child = cmd.spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    let data = stdin.unwrap_or("").to_string();
    let writer = std::thread::spawn(move || {
        let _ = input.write_all(data.as_bytes());
    });
    let out = child.wait_with_output().unwrap();
    let _ = writer.join();
    out
}

pub fn stdout_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn stderr_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Writes the settings file of the config home.
pub fn write_config(home: &ConfigHome, json: &serde_json::Value) {
    std::fs::write(
        home.ecotokens_dir().join("config.json"),
        serde_json::to_string_pretty(json).unwrap(),
    )
    .unwrap();
}

pub fn read_config(home: &ConfigHome) -> serde_json::Value {
    let text = std::fs::read_to_string(home.ecotokens_dir().join("config.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}
