use clap::{Parser, Subcommand};
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{poll, read, Event, KeyCode, KeyModifiers};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::ExecutableCommand;
use ratatui::Terminal;
use std::path::PathBuf;

mod config;
mod daemon;
mod duplicates;
mod filter;
mod hook;
mod install;
mod masking;
mod metrics;
mod search;
mod tokens;
mod trace;
mod tui;

const DEFAULT_MODEL: &str = "sonnet";

#[derive(Parser)]
#[command(
    name = "ecotokens",
    version,
    about = "Token-saving companion for Claude Code, Gemini CLI, and GitHub Copilot"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Intercept a bash command via PreToolUse hook (reads JSON from stdin)
    Hook,
    /// Intercept a Gemini CLI tool call via BeforeTool hook (reads JSON from stdin)
    HookGemini,
    /// Execute a command, filter its output, record metrics
    Filter {
        #[arg(last = true)]
        args: Vec<String>,
        #[arg(long)]
        debug: bool,
    },
    /// Show token savings report
    Gain {
        #[arg(long, default_value = "all")]
        period: String,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        model: Option<String>,
    },
    /// Install ecotokens hook in ~/.claude/settings.json or ~/.gemini/settings.json
    Install {
        /// Target AI tool to install for: claude, gemini, or all (default: claude)
        #[arg(long, default_value = "claude")]
        target: String,
        /// Enable AI-powered output summarization via Ollama
        #[arg(long)]
        ai_summary: bool,
        /// Ollama model to use for AI summary (implies --ai-summary)
        #[arg(long)]
        ai_summary_model: Option<String>,
    },
    /// Remove ecotokens hook from ~/.claude/settings.json or ~/.gemini/settings.json
    Uninstall {
        /// Target to uninstall from: claude, vscode, gemini, or all (default: claude)
        #[arg(long, default_value = "claude")]
        target: String,
    },
    /// Show or update configuration
    Config {
        #[arg(long)]
        json: bool,
        /// Set embed provider: ollama, lmstudio, none
        #[arg(long)]
        embed_provider: Option<String>,
        /// URL of the embeddings provider (e.g. http://localhost:11434)
        #[arg(long)]
        embed_url: Option<String>,
    },
    /// Index a directory for BM25 + symbolic search
    Index {
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        index_dir: Option<PathBuf>,
        #[arg(long)]
        reset: bool,
    },
    /// List symbols in a file or directory
    Outline {
        path: PathBuf,
        #[arg(long, value_delimiter = ',')]
        kinds: Option<Vec<String>>,
        #[arg(long)]
        depth: Option<u32>,
        #[arg(long)]
        json: bool,
    },
    /// Look up a symbol by its stable ID
    Symbol {
        id: String,
        #[arg(long)]
        index_dir: Option<PathBuf>,
    },
    /// Search the indexed codebase
    Search {
        query: String,
        #[arg(long, default_value = "5")]
        top_k: usize,
        #[arg(long)]
        index_dir: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Trace callers or callees of a symbol
    Trace {
        #[command(subcommand)]
        action: TraceAction,
    },
    /// Watch a directory and keep the index up to date automatically
    Watch {
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        index_dir: Option<PathBuf>,
        /// Run in background (no TUI, log events to stdout)
        #[arg(long)]
        background: bool,
        /// Show status of background watch process
        #[arg(long)]
        status: bool,
        /// Stop the background watch process
        #[arg(long)]
        stop: bool,
        /// Output status as JSON
        #[arg(long)]
        json: bool,
    },
    /// Detect code duplications in the indexed codebase and propose refactoring
    Duplicates {
        #[arg(long, default_value = "70.0", help = "Minimum similarity %")]
        threshold: f32,
        #[arg(long, default_value = "5", help = "Minimum block size in lines")]
        min_lines: usize,
        #[arg(long)]
        index_dir: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum TraceAction {
    /// Find callers of a symbol
    Callers {
        symbol: String,
        #[arg(long)]
        index_dir: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Find callees of a symbol
    Callees {
        symbol: String,
        #[arg(long, default_value = "1")]
        depth: u32,
        #[arg(long)]
        index_dir: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
}

/// RAII guard that restores terminal state when dropped, even on panic.
struct TerminalGuard {
    use_stderr: bool,
}

impl TerminalGuard {
    fn stdout() -> Self {
        Self { use_stderr: false }
    }
    fn stderr() -> Self {
        Self { use_stderr: true }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        if self.use_stderr {
            let _ = std::io::stderr().execute(LeaveAlternateScreen);
        } else {
            let _ = std::io::stdout().execute(LeaveAlternateScreen);
        }
    }
}

fn is_quit_key(key: &ratatui::crossterm::event::KeyEvent) -> bool {
    matches!(
        key.code,
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
    ) || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

fn default_index_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ecotokens")
        .join("index")
}

fn default_settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("settings.json")
}

fn default_claude_json_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude.json")
}

fn cmd_filter(args: Vec<String>, debug: bool) {
    if args.is_empty() {
        eprintln!("ecotokens filter: no command given");
        std::process::exit(1);
    }
    let command = args.join(" ");
    let start = std::time::Instant::now();

    let output = std::process::Command::new(&args[0])
        .args(&args[1..])
        .output();

    let (raw, exit_code) = match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }
            let code = o.status.code().unwrap_or(1);
            (stdout, code)
        }
        Err(e) => {
            eprintln!("ecotokens filter: failed to run command: {e}");
            std::process::exit(1);
        }
    };

    let duration_ms = start.elapsed().as_millis() as u32;
    let (filtered, tokens_before, tokens_after) =
        filter::run_filter_pipeline(&command, &raw, duration_ms);

    if debug {
        eprintln!("[ecotokens debug] command={command} tokens_before={tokens_before} tokens_after={tokens_after}");
    }

    print!("{filtered}");
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

fn cmd_gain(period: String, json: bool, model: Option<String>) {
    use metrics::report::{aggregate, Period};
    use metrics::store::read_from;

    let path = match metrics::store::metrics_path() {
        Some(p) => p,
        None => {
            eprintln!("Cannot locate metrics file");
            std::process::exit(1);
        }
    };
    let model_str = model.as_deref().unwrap_or(DEFAULT_MODEL);
    let mut items = read_from(&path).unwrap_or_default();
    let p = Period::parse(&period);
    let mut report = aggregate(&items, p.clone(), model_str);

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        if let Err(e) = enable_raw_mode() {
            eprintln!("failed to enable raw mode: {e}");
        }
        if let Err(e) = std::io::stdout().execute(EnterAlternateScreen) {
            eprintln!("failed to enter alternate screen: {e}");
        }
        let _guard = TerminalGuard::stdout();
        let backend = CrosstermBackend::new(std::io::stdout());
        if let Ok(mut terminal) = Terminal::new(backend) {
            let mut gain_mode = tui::gain::GainMode::default();
            let mut sparkline_mode = tui::gain::SparklineMode::default();
            let mut detail_mode = tui::gain::DetailMode::default();
            let mut selected_family: Option<usize> = None;
            let mut selected_project: Option<usize> = None;
            let mut project_filter: Option<String> = None;
            let mut history_scroll: usize = 0;
            let mut last_reload = std::time::Instant::now();
            // Precomputed once at load time, updated only on reload.
            let mut sorted_projects: Vec<(String, f32)> = sorted_projects_from(&report);
            loop {
                // Reload data every 10 seconds regardless of incoming key events
                if last_reload.elapsed() >= std::time::Duration::from_secs(10) {
                    items = read_from(&path).unwrap_or_default();
                    report = aggregate(&items, p.clone(), model_str);
                    sorted_projects = sorted_projects_from(&report);
                    last_reload = std::time::Instant::now();
                }
                let ts = chrono::Utc::now().format("%H:%M:%S").to_string();
                let family_count = match project_filter.as_deref() {
                    Some(proj) => tui::gain::sorted_family_keys_for_project(&items, proj).len(),
                    None => report.by_family.len(),
                };
                let project_count = report.by_project.len();
                let _ = terminal.draw(|f| {
                    tui::gain::render_gain(
                        f,
                        f.area(),
                        &report,
                        &items,
                        Some(&ts),
                        gain_mode,
                        sparkline_mode,
                        selected_family,
                        detail_mode,
                        selected_project,
                        project_filter.as_deref(),
                        history_scroll,
                    );
                });
                if poll(std::time::Duration::from_millis(500)).unwrap_or(false) {
                    if let Ok(Event::Key(key)) = read() {
                        if is_quit_key(&key) {
                            break;
                        }
                        if key.code == KeyCode::Char('b') {
                            project_filter = None;
                            gain_mode = gain_mode.toggle();
                            history_scroll = 0;
                        }
                        if key.code == KeyCode::Char('s') {
                            sparkline_mode = sparkline_mode.next();
                        }
                        if key.code == KeyCode::Char('d')
                            && gain_mode == tui::gain::GainMode::Family
                        {
                            detail_mode = detail_mode.toggle();
                            history_scroll = 0;
                        }
                        if gain_mode == tui::gain::GainMode::Family && family_count > 0 {
                            match key.code {
                                KeyCode::Char('j') => {
                                    selected_family = Some(match selected_family {
                                        None => 0,
                                        Some(i) => (i + 1) % family_count,
                                    });
                                    history_scroll = 0;
                                }
                                KeyCode::Char('u') => {
                                    selected_family = Some(match selected_family {
                                        None => family_count - 1,
                                        Some(i) => {
                                            if i == 0 {
                                                family_count - 1
                                            } else {
                                                i - 1
                                            }
                                        }
                                    });
                                    history_scroll = 0;
                                }
                                _ => {}
                            }
                        }
                        if gain_mode == tui::gain::GainMode::Project && project_count > 0 {
                            match key.code {
                                KeyCode::Char('j') => {
                                    selected_project = Some(match selected_project {
                                        None => 0,
                                        Some(i) => (i + 1) % project_count,
                                    });
                                    history_scroll = 0;
                                }
                                KeyCode::Char('u') => {
                                    selected_project = Some(match selected_project {
                                        None => project_count - 1,
                                        Some(i) => {
                                            if i == 0 {
                                                project_count - 1
                                            } else {
                                                i - 1
                                            }
                                        }
                                    });
                                    history_scroll = 0;
                                }
                                KeyCode::Char('k') => {
                                    history_scroll = history_scroll.saturating_add(1);
                                }
                                KeyCode::Char('i') => {
                                    history_scroll = history_scroll.saturating_sub(1);
                                }
                                _ => {}
                            }
                        }
                        // i/k scroll the history panel in Family Log mode.
                        if gain_mode == tui::gain::GainMode::Family
                            && detail_mode == tui::gain::DetailMode::Log
                        {
                            match key.code {
                                KeyCode::Char('k') => {
                                    history_scroll = history_scroll.saturating_add(1);
                                }
                                KeyCode::Char('i') => {
                                    history_scroll = history_scroll.saturating_sub(1);
                                }
                                _ => {}
                            }
                        }
                        if gain_mode == tui::gain::GainMode::Project
                            && key.code == KeyCode::Enter
                            && project_count > 0
                        {
                            if let Some(idx) = selected_project {
                                if let Some((name, _)) = sorted_projects.get(idx) {
                                    project_filter = Some(name.clone());
                                    gain_mode = tui::gain::GainMode::Family;
                                    selected_family = None;
                                    history_scroll = 0;
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        println!("=== ecotokens gain ({period}) ===");
        println!("Total commands : {}", report.total_interceptions);
        println!("Tokens before  : {}", report.total_tokens_before);
        println!("Tokens after   : {}", report.total_tokens_after);
        println!("Savings        : {:.1}%", report.total_savings_pct);
        if report.cost_avoided_usd > 0.0 {
            println!("Cost avoided   : ${:.4} USD", report.cost_avoided_usd);
        }
    }
}

/// Compute projects sorted by savings percentage (descending).
fn sorted_projects_from(report: &metrics::report::Report) -> Vec<(String, f32)> {
    let mut projects: Vec<(String, f32)> = report
        .by_project
        .iter()
        .map(|(k, v)| {
            let pct = if v.tokens_before == 0 {
                0.0f32
            } else {
                ((1.0 - v.tokens_after as f64 / v.tokens_before as f64) * 100.0) as f32
            };
            (k.clone(), pct)
        })
        .collect();
    projects.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    projects
}

fn cmd_install(target: String, ai_summary: bool, ai_summary_model: Option<String>) {
    let claude_path = default_settings_path();
    let claude_json = default_claude_json_path();
    let gemini_path = install::default_gemini_settings_path();

    let install_claude = matches!(target.as_str(), "claude" | "all");
    let install_gemini = matches!(target.as_str(), "gemini" | "all");

    if !install_claude && !install_gemini {
        eprintln!(
            "unknown target '{}'. Valid values: claude, gemini, all",
            target
        );
        std::process::exit(1);
    }

    if install_claude {
        match install::install_hook(&claude_path, &claude_json) {
            Ok(()) => {
                println!("ecotokens hook installed → {}", claude_path.display());
            }
            Err(e) => {
                eprintln!("install error (claude): {e}");
                std::process::exit(1);
            }
        }
    }

    if install_gemini {
        match gemini_path {
            Some(ref p) => {
                match install::install_gemini_hook(p) {
                    Ok(()) => println!("ecotokens hook installed (Gemini) → {}", p.display()),
                    Err(e) => {
                        eprintln!("install error (gemini hook): {e}");
                        std::process::exit(1);
                    }
                }
            }
            None => {
                eprintln!("cannot determine Gemini settings path on this system");
                std::process::exit(1);
            }
        }
    }

    let enable_ai = ai_summary || ai_summary_model.is_some();
    if enable_ai {
        let mut settings = config::Settings::load();
        settings.ai_summary_enabled = true;
        if let Some(model) = ai_summary_model {
            settings.ai_summary_model = Some(model);
        }
        if let Err(e) = settings.save() {
            eprintln!("failed to save config: {e}");
            std::process::exit(1);
        }
        println!("AI summary configured in ~/.config/ecotokens/config.json");
    }
}

fn cmd_uninstall(target: String) {
    let claude_path = default_settings_path();
    let claude_json = default_claude_json_path();
    let vscode_path = install::default_vscode_settings_path();
    let gemini_path = install::default_gemini_settings_path();

    let uninstall_claude = matches!(target.as_str(), "claude" | "all");
    let uninstall_vscode = matches!(target.as_str(), "vscode" | "all");
    let uninstall_gemini = matches!(target.as_str(), "gemini" | "all");

    if !uninstall_claude && !uninstall_vscode && !uninstall_gemini {
        eprintln!(
            "unknown target '{}'. Valid values: claude, vscode, gemini, all",
            target
        );
        std::process::exit(1);
    }

    if uninstall_claude {
        let had_hook = install::is_hook_installed(&claude_path);
        let had_mcp = install::is_mcp_registered(&claude_json);
        match install::uninstall_hook(&claude_path, &claude_json) {
            Ok(()) => {
                if had_hook {
                    println!("ecotokens hook removed ← {}", claude_path.display());
                }
                if had_mcp {
                    println!(
                        "ecotokens MCP server unregistered ← {}",
                        claude_json.display()
                    );
                }
                if !had_hook && !had_mcp {
                    println!("ecotokens: nothing to uninstall (claude)");
                }
            }
            Err(e) => {
                eprintln!("uninstall error (claude): {e}");
                std::process::exit(1);
            }
        }
    }

    if uninstall_vscode {
        let mcp_json_path = install::default_vscode_mcp_json_path();
        let had_settings = vscode_path
            .as_deref()
            .is_some_and(install::is_vscode_mcp_registered);
        let had_json = mcp_json_path
            .as_deref()
            .is_some_and(install::is_vscode_mcp_json_registered);

        match vscode_path {
            Some(ref p) => match install::uninstall_vscode_mcp(p) {
                Ok(()) => {
                    if had_settings {
                        println!(
                            "ecotokens MCP server unregistered (VS Code settings.json) ← {}",
                            p.display()
                        );
                    }
                }
                Err(e) => {
                    eprintln!("uninstall error (vscode settings.json): {e}");
                    std::process::exit(1);
                }
            },
            None => {
                eprintln!("cannot determine VS Code settings path on this system");
                std::process::exit(1);
            }
        }
        if let Some(ref p) = mcp_json_path {
            match install::uninstall_vscode_mcp_json(p) {
                Ok(()) => {
                    if had_json {
                        println!(
                            "ecotokens MCP server unregistered (VS Code mcp.json) ← {}",
                            p.display()
                        );
                    }
                }
                Err(e) => {
                    eprintln!("uninstall error (vscode mcp.json): {e}");
                    std::process::exit(1);
                }
            }
        }
        if !had_settings && !had_json {
            println!("ecotokens: nothing to uninstall (vscode)");
        }
    }

    if uninstall_gemini {
        match gemini_path {
            Some(ref p) => {
                let had_hook = install::is_gemini_hook_installed(p);
                let had_mcp = install::is_gemini_mcp_registered(p);
                match install::uninstall_gemini(p) {
                    Ok(()) => {
                        if had_hook {
                            println!("ecotokens hook removed (Gemini) ← {}", p.display());
                        }
                        if had_mcp {
                            println!(
                                "ecotokens MCP server unregistered (Gemini) ← {}",
                                p.display()
                            );
                        }
                        if !had_hook && !had_mcp {
                            println!("ecotokens: nothing to uninstall (gemini)");
                        }
                    }
                    Err(e) => {
                        eprintln!("uninstall error (gemini): {e}");
                        std::process::exit(1);
                    }
                }
            }
            None => {
                eprintln!("cannot determine Gemini settings path on this system");
                std::process::exit(1);
            }
        }
    }
}

fn cmd_config(json: bool, embed_provider: Option<String>, embed_url: Option<String>) {
    use config::settings::EmbedProvider;

    let mut settings = config::Settings::load();
    let settings_path = default_settings_path();
    let claude_json = default_claude_json_path();

    // Mutation via --embed-provider
    if let Some(ref provider_name) = embed_provider {
        let default_url = match provider_name.as_str() {
            "ollama" => "http://localhost:11434",
            "lmstudio" => "http://localhost:1234",
            _ => "",
        };
        let url = embed_url.clone().unwrap_or_else(|| default_url.to_string());

        settings.embed_provider = match provider_name.as_str() {
            "ollama" => EmbedProvider::Ollama { url },
            "lmstudio" => EmbedProvider::LmStudio { url },
            "none" => EmbedProvider::None,
            other => {
                eprintln!(
                    "unknown provider: '{}'. Valid values: ollama, lmstudio, none",
                    other
                );
                std::process::exit(1);
            }
        };

        match settings.save() {
            Ok(()) => eprintln!("embed_provider updated"),
            Err(e) => {
                eprintln!("save error: {e}");
                std::process::exit(1);
            }
        }
    }

    let provider_str = match &settings.embed_provider {
        EmbedProvider::None => "none".to_string(),
        EmbedProvider::Ollama { url } => format!("ollama ({})", url),
        EmbedProvider::LmStudio { url } => format!("lmstudio ({})", url),
    };

    let hook_installed = install::is_hook_installed(&settings_path);
    let _ = claude_json;

    if json {
        let mut v = serde_json::to_value(&settings).unwrap();
        v["hook_installed"] = serde_json::Value::Bool(hook_installed);
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    } else {
        println!("hook_installed        : {}", hook_installed);
        println!("debug                 : {}", settings.debug);
        println!("exclusions            : {:?}", settings.exclusions);
        println!("embed_provider        : {}", provider_str);
        println!("ai_summary_enabled    : {}", settings.ai_summary_enabled);
        println!(
            "ai_summary_model      : {}",
            settings
                .ai_summary_model
                .as_deref()
                .unwrap_or("llama3.2:3b (default)")
        );
        println!(
            "ai_summary_url        : {}",
            settings
                .ai_summary_url
                .as_deref()
                .unwrap_or("http://localhost:11434 (default)")
        );
    }
}

fn cmd_index(path: Option<PathBuf>, index_dir: Option<PathBuf>, reset: bool) {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let target = path.unwrap_or(cwd);
    let idx_dir = index_dir.unwrap_or_else(default_index_dir);

    let is_stderr_tty = std::io::IsTerminal::is_terminal(&std::io::stderr());
    let is_stdin_tty = std::io::IsTerminal::is_terminal(&std::io::stdin());
    let is_dumb = std::env::var("TERM").map(|v| v == "dumb").unwrap_or(false);
    let is_automated = std::env::var("CI").is_ok() || std::env::var("ECOTOKENS_BATCH").is_ok();

    if is_stderr_tty && is_stdin_tty && !is_dumb && !is_automated {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        // First pass: count indexable files (same filter as the indexer uses).
        let total = search::index::count_indexable_files(&target);

        let counter = Arc::new(AtomicUsize::new(0));
        let opts = search::index::IndexOptions {
            reset,
            path: target,
            index_dir: idx_dir,
            progress: Some(counter.clone()),
            embed_provider: config::Settings::load().embed_provider,
        };

        if let Err(e) = enable_raw_mode() {
            eprintln!("failed to enable raw mode: {e}");
        }
        if let Err(e) = std::io::stderr().execute(EnterAlternateScreen) {
            eprintln!("failed to enter alternate screen: {e}");
        }
        // Guard ensures terminal is restored even if indexing thread panics.
        let _guard = TerminalGuard::stderr();
        let backend = CrosstermBackend::new(std::io::stderr());
        let handle = std::thread::spawn(move || search::index::index_directory(opts));

        let result = {
            let mut terminal_opt = Terminal::new(backend).ok();
            loop {
                let done = counter.load(Ordering::Relaxed) as u64;
                if let Some(ref mut terminal) = terminal_opt {
                    let _ = terminal.draw(|f| {
                        tui::progress::render_progress(
                            f,
                            f.area(),
                            done,
                            total.max(1),
                            "Indexing…",
                        );
                    });
                }
                if handle.is_finished() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            // Draw 100%
            if let Some(ref mut terminal) = terminal_opt {
                let _ = terminal.draw(|f| {
                    tui::progress::render_progress(f, f.area(), total, total.max(1), "Indexing…");
                });
            }
            handle.join().expect("indexing thread panicked")
        };

        // _guard drops here, restoring terminal before printing result
        drop(_guard);

        match result {
            Ok(stats) => {
                println!(
                    "Indexed {} files, {} chunks",
                    stats.file_count, stats.chunk_count
                )
            }
            Err(e) => {
                eprintln!("index error: {e}");
                std::process::exit(1);
            }
        }
    } else {
        eprintln!("Indexing {}…", target.display());
        let opts = search::index::IndexOptions {
            reset,
            path: target,
            index_dir: idx_dir,
            progress: None,
            embed_provider: config::Settings::load().embed_provider,
        };
        match search::index::index_directory(opts) {
            Ok(stats) => {
                println!(
                    "Indexed {} files, {} chunks",
                    stats.file_count, stats.chunk_count
                )
            }
            Err(e) => {
                eprintln!("index error: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn cmd_outline(path: PathBuf, kinds: Option<Vec<String>>, depth: Option<u32>, json: bool) {
    let opts = search::outline::OutlineOptions {
        path,
        depth,
        kinds,
        base: None,
    };
    match search::outline::outline_path(opts) {
        Ok(symbols) => {
            if json {
                let slim: Vec<_> = symbols
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "id": s.id,
                            "name": s.name,
                            "kind": s.kind,
                            "file_path": s.file_path,
                            "line_start": s.line_start,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&slim).unwrap());
            } else if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                if let Err(e) = enable_raw_mode() {
                    eprintln!("failed to enable raw mode: {e}");
                }
                if let Err(e) = std::io::stdout().execute(EnterAlternateScreen) {
                    eprintln!("failed to enter alternate screen: {e}");
                }
                let _guard = TerminalGuard::stdout();
                let backend = CrosstermBackend::new(std::io::stdout());
                if let Ok(mut terminal) = Terminal::new(backend) {
                    let mut selected = 0usize;
                    let max = symbols.len().saturating_sub(1);
                    loop {
                        let _ = terminal.draw(|f| {
                            tui::outline::render_outline(f, f.area(), &symbols, selected);
                        });
                        if let Ok(Event::Key(key)) = read() {
                            match key.code {
                                KeyCode::Down | KeyCode::Char('j') => {
                                    selected = (selected + 1).min(max);
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    selected = selected.saturating_sub(1);
                                }
                                _ if is_quit_key(&key) => break,
                                _ => {}
                            }
                        }
                    }
                }
            } else {
                for s in &symbols {
                    println!("{}:{} {} {}", s.file_path, s.line_start, s.kind, s.name);
                }
            }
        }
        Err(e) => {
            eprintln!("outline error: {e}");
            std::process::exit(1);
        }
    }
}

fn cmd_symbol(id: String, index_dir: Option<PathBuf>) {
    let idx_dir = index_dir.unwrap_or_else(default_index_dir);
    match search::symbols::lookup_symbol(&id, &idx_dir) {
        Ok(Some(snippet)) => println!("{snippet}"),
        Ok(None) => {
            eprintln!("Symbol not found: {id}");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("lookup error: {e}");
            std::process::exit(1);
        }
    }
}

fn cmd_search(query: String, top_k: usize, index_dir: Option<PathBuf>, json: bool) {
    let idx_dir = index_dir.unwrap_or_else(default_index_dir);
    let embed_provider = config::Settings::load().embed_provider;
    let opts = search::query::SearchOptions {
        query,
        top_k,
        index_dir: idx_dir,
        embed_provider,
    };
    match search::query::search_index(opts) {
        Ok(results) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&results).unwrap());
            } else {
                for r in &results {
                    println!("{} (score: {:.3})", r.file_path, r.score);
                    println!("  {}", r.snippet.lines().next().unwrap_or(""));
                }
            }
        }
        Err(e) => {
            eprintln!("search error: {e}");
            std::process::exit(1);
        }
    }
}

fn display_trace_result(
    edges: Result<Vec<trace::CallEdge>, trace::TraceError>,
    symbol: &str,
    direction: &str,
    json: bool,
) {
    match edges {
        Ok(edges) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&edges).unwrap());
            } else if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                if let Err(e) = enable_raw_mode() {
                    eprintln!("failed to enable raw mode: {e}");
                }
                if let Err(e) = std::io::stdout().execute(EnterAlternateScreen) {
                    eprintln!("failed to enter alternate screen: {e}");
                }
                let _guard = TerminalGuard::stdout();
                let backend = CrosstermBackend::new(std::io::stdout());
                if let Ok(mut terminal) = Terminal::new(backend) {
                    loop {
                        let _ = terminal.draw(|f| {
                            tui::trace::render_trace(f, f.area(), &edges, symbol, direction);
                        });
                        if let Ok(Event::Key(key)) = read() {
                            if is_quit_key(&key) {
                                break;
                            }
                        }
                    }
                }
            } else {
                for e in &edges {
                    println!("{} {}:{}", e.name, e.file_path, e.line);
                }
            }
        }
        Err(e) => {
            eprintln!("trace error: {e}");
            std::process::exit(1);
        }
    }
}

fn cmd_trace_callers(symbol: String, index_dir: Option<PathBuf>, json: bool) {
    let idx_dir = index_dir.unwrap_or_else(default_index_dir);
    let result = trace::callers::find_callers(&symbol, &idx_dir);
    display_trace_result(result, &symbol, "callers", json);
}

fn cmd_trace_callees(symbol: String, depth: u32, index_dir: Option<PathBuf>, json: bool) {
    let idx_dir = index_dir.unwrap_or_else(default_index_dir);
    let result = trace::callees::find_callees(&symbol, &idx_dir, depth);
    display_trace_result(result, &symbol, "callees", json);
}

fn cmd_watch(
    path: Option<PathBuf>,
    index_dir: Option<PathBuf>,
    background: bool,
    status: bool,
    stop: bool,
    json: bool,
) {
    // If --stop is requested, stop the background process and exit.
    if stop {
        if let Some(state) = config::BackgroundState::load() {
            match state.stop() {
                Ok(()) => {
                    println!(
                        "ecotokens watch: background process (PID {}) stopped",
                        state.pid
                    );
                }
                Err(e) => {
                    eprintln!("ecotokens watch: failed to stop process: {}", e);
                    std::process::exit(1);
                }
            }
        } else {
            eprintln!("ecotokens watch: no background process running");
            std::process::exit(1);
        }
        return;
    }

    // If --status is requested, show status and exit.
    if status {
        if let Some(state) = config::BackgroundState::load() {
            let is_running = state.is_running();
            if json {
                let mut obj = serde_json::to_value(&state).unwrap();
                obj["running"] = serde_json::Value::Bool(is_running);
                println!("{}", serde_json::to_string_pretty(&obj).unwrap());
            } else {
                println!("ecotokens watch (background) status:");
                println!("  PID              : {}", state.pid);
                println!("  Watch path       : {}", state.watch_path);
                println!("  Index dir        : {}", state.index_dir);
                println!("  Started at       : {}", state.started_at);
                if let Some(ref log) = state.log_file {
                    println!("  Log file         : {}", log);
                }
                println!(
                    "  Running          : {}",
                    if is_running { "yes" } else { "no" }
                );
            }
        } else {
            eprintln!("ecotokens watch: no background process running");
            std::process::exit(1);
        }
        return;
    }

    // If --background is requested, daemonize the process.
    #[cfg(unix)]
    if background {
        let cwd_temp = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let watch_path_temp = path.as_ref().unwrap_or(&cwd_temp);
        let default_idx = default_index_dir();
        let idx_dir_temp = index_dir.as_ref().unwrap_or(&default_idx);

        let log_path = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("ecotokens")
            .join("watch.log");
        let log_path_str = log_path.to_string_lossy().to_string();

        // Print before daemonizing so the user sees it in the terminal.
        println!("ecotokens watch: starting in background");
        println!("  Watch path: {}", watch_path_temp.display());
        println!("  Log file  : {}", log_path_str);
        println!("Use 'ecotokens watch --status' to check status");
        println!("Use 'ecotokens watch --stop' to stop");

        match daemonize::Daemonize::new().start() {
            Ok(_) => {
                // We are now in the daemon child process.
                let bg_state =
                    config::BackgroundState::new(watch_path_temp, idx_dir_temp, Some(log_path_str));
                let _ = bg_state.save();
            }
            Err(e) => {
                eprintln!("Failed to daemonize: {}", e);
                std::process::exit(1);
            }
        }
    }

    #[cfg(not(unix))]
    if background {
        eprintln!("Background mode is only supported on Unix systems");
        std::process::exit(1);
    }

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let watch_path = path.unwrap_or(cwd);
    let idx_dir = index_dir.unwrap_or_else(default_index_dir);
    let watch_path_str = watch_path.display().to_string();
    let is_interactive = !background && std::io::IsTerminal::is_terminal(&std::io::stdout());

    // Count only truly indexable files for accurate progress.
    let total_files = search::index::count_indexable_files(&watch_path);

    let counter = Arc::new(AtomicUsize::new(0));
    let opts = search::index::IndexOptions {
        reset: false,
        path: watch_path.clone(),
        index_dir: idx_dir.clone(),
        progress: Some(counter.clone()),
        embed_provider: config::Settings::load().embed_provider,
    };

    // Phase A — Initial indexing
    let report = if is_interactive {
        if let Err(e) = enable_raw_mode() {
            eprintln!("failed to enable raw mode: {e}");
        }
        if let Err(e) = std::io::stdout().execute(EnterAlternateScreen) {
            eprintln!("failed to enter alternate screen: {e}");
        }
        // Guard covers both Phase A and Phase B — terminal restored even on panic.
        let _guard = TerminalGuard::stdout();
        let backend = CrosstermBackend::new(std::io::stdout());

        let start = std::time::Instant::now();
        let index_handle = std::thread::spawn(move || search::index::index_directory(opts));

        let index_result = {
            let mut terminal_opt = Terminal::new(backend).ok();
            loop {
                let done = counter.load(Ordering::Relaxed) as u64;
                if let Some(ref mut t) = terminal_opt {
                    let _ = t.draw(|f| {
                        tui::watch::render_indexing(f, f.area(), done, total_files.max(1));
                    });
                }
                if index_handle.is_finished() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            // Draw 100%
            if let Some(ref mut t) = terminal_opt {
                let _ = t.draw(|f| {
                    tui::watch::render_indexing(f, f.area(), total_files, total_files.max(1));
                });
            }
            index_handle.join().expect("indexing thread panicked")
        };

        let elapsed = start.elapsed().as_secs_f64();

        // Phase B — Launch file watcher (alternate screen still active)
        let (event_tx, event_rx) = std::sync::mpsc::channel::<daemon::watcher::WatchEvent>();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let watch_path_clone = watch_path.clone();
        let idx_dir_clone = idx_dir.clone();
        let watcher_handle = std::thread::spawn(move || {
            daemon::watcher::watch_directory(&watch_path_clone, &idx_dir_clone, event_tx, stop_rx)
        });

        let index_report = index_result.ok().map(|stats| tui::watch::IndexReport {
            file_count: stats.file_count,
            chunk_count: stats.chunk_count,
            elapsed_secs: elapsed,
        });

        let backend2 = CrosstermBackend::new(std::io::stdout());
        if let Ok(mut terminal) = Terminal::new(backend2) {
            let mut events: Vec<daemon::watcher::WatchEvent> = Vec::new();
            let mut watch_stats = tui::watch::WatchStats {
                reindexed: 0,
                ignored: 0,
                errors: 0,
            };

            loop {
                while let Ok(e) = event_rx.try_recv() {
                    if e.status == "re-indexed" {
                        watch_stats.reindexed += 1;
                    } else if e.status.starts_with("error") {
                        watch_stats.errors += 1;
                    } else {
                        watch_stats.ignored += 1;
                    }
                    events.push(e);
                }

                let _ = terminal.draw(|f| {
                    tui::watch::render_watch(
                        f,
                        f.area(),
                        &events,
                        &watch_path_str,
                        index_report.as_ref(),
                        &watch_stats,
                    );
                });

                if poll(std::time::Duration::from_millis(100)).unwrap_or(false) {
                    if let Ok(Event::Key(key)) = read() {
                        if is_quit_key(&key) {
                            break;
                        }
                    }
                }
            }
        }

        let _ = stop_tx.send(());
        let _ = watcher_handle.join();

        // _guard drops here, restoring terminal
        index_report
    } else {
        // Non-interactive mode: blocking initial index.
        eprintln!(
            "ecotokens watch: initial indexing of {} files…",
            total_files
        );
        let start = std::time::Instant::now();
        let result = search::index::index_directory(opts);
        let elapsed = start.elapsed().as_secs_f64();

        let index_report = result.ok().map(|stats| tui::watch::IndexReport {
            file_count: stats.file_count,
            chunk_count: stats.chunk_count,
            elapsed_secs: elapsed,
        });

        // Phase B — Launch file watcher
        let (event_tx, event_rx) = std::sync::mpsc::channel::<daemon::watcher::WatchEvent>();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let watch_path_clone = watch_path.clone();
        let idx_dir_clone = idx_dir.clone();
        let watcher_handle = std::thread::spawn(move || {
            daemon::watcher::watch_directory(&watch_path_clone, &idx_dir_clone, event_tx, stop_rx)
        });

        // Background mode: log events to watch.log
        let log_file = config::BackgroundState::load()
            .and_then(|s| s.log_file)
            .map(std::path::PathBuf::from);

        if log_file.is_none() {
            eprintln!(
                "ecotokens watch: warning: no log file configured, events will not be recorded"
            );
        }

        while let Ok(e) = event_rx.recv() {
            if let Some(ref path) = log_file {
                let line = format!("[{}] {} {}\n", e.timestamp, e.path.display(), e.status);
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
            }
        }

        // Clean up state on exit.
        let _ = config::BackgroundState::remove();

        let _ = stop_tx.send(());
        let _ = watcher_handle.join();

        index_report
    };

    let _ = report; // suppress unused warning in non-interactive path
}


#[derive(serde::Serialize)]
struct DuplicatesJsonOutput {
    scanned_symbols: usize,
    threshold: f32,
    min_lines: usize,
    index_stale: bool,
    groups: Vec<duplicates::DuplicateGroup>,
}

fn cmd_duplicates(threshold: f32, min_lines: usize, index_dir: Option<PathBuf>, json: bool) {
    if !(0.0..=100.0).contains(&threshold) {
        eprintln!("Error: threshold must be between 0 and 100.");
        std::process::exit(2);
    }
    let idx_dir = index_dir.unwrap_or_else(default_index_dir);

    // Staleness check
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let stale = duplicates::staleness::check_staleness(&idx_dir, &cwd);
    let index_stale = stale.is_some();
    if stale.is_some() {
        eprintln!("Warning: index may be stale. Run `ecotokens index` to update.");
    }

    let opts = duplicates::DetectionOptions {
        index_dir: idx_dir,
        threshold,
        min_lines,
    };
    match duplicates::detect::detect_duplicates(&opts) {
        Ok(groups) => {
            if json {
                let scanned = groups.iter().map(|g| g.segments.len()).sum();
                let output = DuplicatesJsonOutput {
                    scanned_symbols: scanned,
                    threshold,
                    min_lines,
                    index_stale,
                    groups,
                };
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                print!(
                    "{}",
                    duplicates::proposals::format_duplicates_plain(&groups, threshold, min_lines)
                );
            }
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Hook => hook::handle(),
        Commands::HookGemini => hook::handle_gemini(),
        Commands::Filter { args, debug } => cmd_filter(args, debug),
        Commands::Gain {
            period,
            json,
            model,
        } => cmd_gain(period, json, model),
        Commands::Install {
            target,
            ai_summary,
            ai_summary_model,
        } => cmd_install(target, ai_summary, ai_summary_model),
        Commands::Uninstall { target } => cmd_uninstall(target),
        Commands::Config {
            json,
            embed_provider,
            embed_url,
        } => cmd_config(json, embed_provider, embed_url),
        Commands::Index {
            path,
            index_dir,
            reset,
        } => cmd_index(path, index_dir, reset),
        Commands::Outline {
            path,
            kinds,
            depth,
            json,
        } => cmd_outline(path, kinds, depth, json),
        Commands::Symbol { id, index_dir } => cmd_symbol(id, index_dir),
        Commands::Search {
            query,
            top_k,
            index_dir,
            json,
        } => cmd_search(query, top_k, index_dir, json),
        Commands::Trace { action } => match action {
            TraceAction::Callers {
                symbol,
                index_dir,
                json,
            } => cmd_trace_callers(symbol, index_dir, json),
            TraceAction::Callees {
                symbol,
                depth,
                index_dir,
                json,
            } => cmd_trace_callees(symbol, depth, index_dir, json),
        },
        Commands::Watch {
            path,
            index_dir,
            background,
            status,
            stop,
            json,
        } => cmd_watch(path, index_dir, background, status, stop, json),
        Commands::Duplicates {
            threshold,
            min_lines,
            index_dir,
            json,
        } => cmd_duplicates(threshold, min_lines, index_dir, json),
    }
}
