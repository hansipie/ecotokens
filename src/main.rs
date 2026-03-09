use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod config;
mod daemon;
mod filter;
mod hook;
mod install;
mod masking;
mod mcp;
mod metrics;
mod search;
mod tokens;
mod trace;
mod tui;

#[derive(Parser)]
#[command(name = "ecotokens", version, about = "Token-saving companion for Claude Code")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Intercept a bash command via PreToolUse hook (reads JSON from stdin)
    Hook,
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
    /// Install ecotokens hook in ~/.claude/settings.json
    Install {
        #[arg(long)]
        with_mcp: bool,
        /// Target AI tool to install for: claude (default), vscode, all
        #[arg(long, default_value = "claude")]
        target: String,
        /// Enable AI-powered output summarization via Ollama
        #[arg(long)]
        ai_summary: bool,
        /// Ollama model to use for AI summary (implies --ai-summary)
        #[arg(long)]
        ai_summary_model: Option<String>,
    },
    /// Remove ecotokens hook from ~/.claude/settings.json
    Uninstall {
        /// Target to uninstall from: claude (default), vscode, all
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
        /// URL du provider d'embeddings (ex: http://localhost:11434)
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
    /// Start MCP server (JSON-RPC over stdio)
    Mcp {
        #[arg(long)]
        index_dir: Option<PathBuf>,
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

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Hook => hook::handle(),

        Commands::Filter { args, debug } => {
            if args.is_empty() {
                eprintln!("ecotokens filter: no command given");
                std::process::exit(1);
            }
            let command = args.join(" ");
            let start = std::time::Instant::now();

            let output = std::process::Command::new(&args[0])
                .args(&args[1..])
                .output();

            let raw = match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
                    let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
                    if !stderr.is_empty() { eprint!("{stderr}"); }
                    stdout
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
        }

        Commands::Gain { period, json, model } => {
            use metrics::report::{aggregate, Period};
            use metrics::store::read_from;
            let path = match metrics::store::metrics_path() {
                Some(p) => p,
                None => { eprintln!("Cannot locate metrics file"); std::process::exit(1); }
            };
            let mut items = read_from(&path).unwrap_or_default();
            let p = Period::parse(&period);
            let mut report = aggregate(&items, p.clone(), model.as_deref().unwrap_or("sonnet"));
            if json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                use ratatui::backend::CrosstermBackend;
                use ratatui::crossterm::event::{poll, read, Event, KeyCode, KeyModifiers};
                use ratatui::crossterm::terminal::{
                    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
                };
                use ratatui::crossterm::ExecutableCommand;
                use ratatui::Terminal;

                let _ = enable_raw_mode();
                let _ = std::io::stdout().execute(EnterAlternateScreen);
                let backend = CrosstermBackend::new(std::io::stdout());
                if let Ok(mut terminal) = Terminal::new(backend) {
                    let mut gain_mode = tui::gain::GainMode::default();
                    let mut sparkline_mode = tui::gain::SparklineMode::default();
                    let mut detail_mode = tui::gain::DetailMode::default();
                    let mut selected_family: Option<usize> = None;
                    let mut selected_project: Option<usize> = None;
                    let mut project_filter: Option<String> = None;
                    let mut last_reload = std::time::Instant::now();
                    loop {
                        // Reload data every 2 seconds regardless of incoming key events
                        if last_reload.elapsed() >= std::time::Duration::from_secs(2) {
                            items = read_from(&path).unwrap_or_default();
                            report = aggregate(&items, p.clone(), model.as_deref().unwrap_or("sonnet"));
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
                                f, f.area(), &report, &items,
                                Some(&ts), gain_mode, sparkline_mode,
                                selected_family,
                                detail_mode,
                                selected_project,
                                project_filter.as_deref(),
                            );
                        });
                        if poll(std::time::Duration::from_millis(100)).unwrap_or(false) {
                            if let Ok(Event::Key(key)) = read() {
                                if matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc)
                                    || (key.code == KeyCode::Char('c')
                                        && key.modifiers.contains(KeyModifiers::CONTROL))
                                {
                                    break;
                                }
                                if key.code == KeyCode::Char('b') {
                                    project_filter = None;
                                    gain_mode = gain_mode.toggle();
                                }
                                if key.code == KeyCode::Char('s') {
                                    sparkline_mode = sparkline_mode.next();
                                }
                                if key.code == KeyCode::Char('d') && gain_mode == tui::gain::GainMode::Family {
                                    detail_mode = detail_mode.toggle();
                                }
                                if gain_mode == tui::gain::GainMode::Family && family_count > 0 {
                                    match key.code {
                                        KeyCode::Down | KeyCode::Char('j') => {
                                            selected_family = Some(match selected_family {
                                                None => 0,
                                                Some(i) => (i + 1) % family_count,
                                            });
                                        }
                                        KeyCode::Up | KeyCode::Char('k') => {
                                            selected_family = Some(match selected_family {
                                                None => family_count - 1,
                                                Some(i) => if i == 0 { family_count - 1 } else { i - 1 },
                                            });
                                        }
                                        _ => {}
                                    }
                                }
                                if gain_mode == tui::gain::GainMode::Project && project_count > 0 {
                                    match key.code {
                                        KeyCode::Down | KeyCode::Char('j') => {
                                            selected_project = Some(match selected_project {
                                                None => 0,
                                                Some(i) => (i + 1) % project_count,
                                            });
                                        }
                                        KeyCode::Up | KeyCode::Char('k') => {
                                            selected_project = Some(match selected_project {
                                                None => project_count - 1,
                                                Some(i) => if i == 0 { project_count - 1 } else { i - 1 },
                                            });
                                        }
                                        _ => {}
                                    }
                                }
                                if gain_mode == tui::gain::GainMode::Project
                                    && key.code == KeyCode::Enter
                                    && project_count > 0
                                {
                                    if let Some(idx) = selected_project {
                                        let mut projects: Vec<(&String, f32)> = report.by_project.iter()
                                            .map(|(k, v)| {
                                                let pct = if v.tokens_before == 0 { 0.0f32 }
                                                    else { ((1.0 - v.tokens_after as f64 / v.tokens_before as f64) * 100.0) as f32 };
                                                (k, pct)
                                            })
                                            .collect();
                                        projects.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                                        if let Some((name, _)) = projects.get(idx) {
                                            project_filter = Some(name.to_string());
                                            gain_mode = tui::gain::GainMode::Family;
                                            selected_family = None;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                let _ = disable_raw_mode();
                let _ = std::io::stdout().execute(LeaveAlternateScreen);
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

        Commands::Install { with_mcp, target, ai_summary, ai_summary_model } => {
            let claude_path = default_settings_path();
            let claude_json = default_claude_json_path();
            let vscode_path = install::default_vscode_settings_path();

            let install_claude = matches!(target.as_str(), "claude" | "all");
            let install_vscode = matches!(target.as_str(), "vscode" | "all");

            if install_claude {
                match install::install_hook(&claude_path, &claude_json, with_mcp) {
                    Ok(()) => {
                        println!("ecotokens hook installed → {}", claude_path.display());
                        if with_mcp {
                            println!("ecotokens MCP server registered → {}", claude_json.display());
                        }
                    }
                    Err(e) => { eprintln!("install error (claude): {e}"); std::process::exit(1); }
                }
            }

            if install_vscode {
                match vscode_path {
                    Some(ref p) => match install::install_vscode_mcp(p) {
                        Ok(()) => println!("ecotokens MCP server registered (VS Code) → {}", p.display()),
                        Err(e) => { eprintln!("install error (vscode): {e}"); std::process::exit(1); }
                    },
                    None => {
                        eprintln!("cannot determine VS Code settings path on this system");
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

        Commands::Uninstall { target } => {
            let claude_path = default_settings_path();
            let claude_json = default_claude_json_path();
            let vscode_path = install::default_vscode_settings_path();

            let uninstall_claude = matches!(target.as_str(), "claude" | "all");
            let uninstall_vscode = matches!(target.as_str(), "vscode" | "all");

            if uninstall_claude {
                let had_hook = install::is_hook_installed(&claude_path);
                let had_mcp = install::is_mcp_registered(&claude_json);
                match install::uninstall_hook(&claude_path, &claude_json) {
                    Ok(()) => {
                        if had_hook { println!("ecotokens hook removed ← {}", claude_path.display()); }
                        if had_mcp { println!("ecotokens MCP server unregistered ← {}", claude_json.display()); }
                        if !had_hook && !had_mcp { println!("ecotokens: nothing to uninstall (claude)"); }
                    }
                    Err(e) => { eprintln!("uninstall error (claude): {e}"); std::process::exit(1); }
                }
            }

            if uninstall_vscode {
                match vscode_path {
                    Some(ref p) => {
                        let had_vscode = install::is_vscode_mcp_registered(p);
                        match install::uninstall_vscode_mcp(p) {
                            Ok(()) => {
                                if had_vscode { println!("ecotokens MCP server unregistered (VS Code) ← {}", p.display()); }
                                else { println!("ecotokens: nothing to uninstall (vscode)"); }
                            }
                            Err(e) => { eprintln!("uninstall error (vscode): {e}"); std::process::exit(1); }
                        }
                    }
                    None => {
                        eprintln!("cannot determine VS Code settings path on this system");
                        std::process::exit(1);
                    }
                }
            }
        }

        Commands::Config { json, embed_provider, embed_url } => {
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
                            "provider inconnu: '{}'. Valeurs valides: ollama, lmstudio, none",
                            other
                        );
                        std::process::exit(1);
                    }
                };

                match settings.save() {
                    Ok(()) => eprintln!("embed_provider mis à jour"),
                    Err(e) => { eprintln!("erreur sauvegarde: {e}"); std::process::exit(1); }
                }
            }

            let provider_str = match &settings.embed_provider {
                EmbedProvider::None => "none".to_string(),
                EmbedProvider::Ollama { url } => format!("ollama ({})", url),
                EmbedProvider::LmStudio { url } => format!("lmstudio ({})", url),
            };

            let hook_installed = install::is_hook_installed(&settings_path);
            let mcp_registered = install::is_mcp_registered(&claude_json);
            let vscode_mcp_registered = install::default_vscode_settings_path()
                .map(|p| install::is_vscode_mcp_registered(&p))
                .unwrap_or(false);

            if json {
                let mut v = serde_json::to_value(&settings).unwrap();
                v["hook_installed"] = serde_json::Value::Bool(hook_installed);
                v["mcp_registered"] = serde_json::Value::Bool(mcp_registered);
                v["vscode_mcp_registered"] = serde_json::Value::Bool(vscode_mcp_registered);
                println!("{}", serde_json::to_string_pretty(&v).unwrap());
            } else {
                println!("hook_installed        : {}", hook_installed);
                println!("mcp_registered        : {}", mcp_registered);
                println!("vscode_mcp_registered : {}", vscode_mcp_registered);
                println!("debug                 : {}", settings.debug);
                println!("threshold_lines       : {}", settings.summary_threshold_lines);
                println!("threshold_bytes       : {}", settings.summary_threshold_bytes);
                println!("exclusions            : {:?}", settings.exclusions);
                println!("embed_provider        : {}", provider_str);
                println!("ai_summary_enabled    : {}", settings.ai_summary_enabled);
                println!("ai_summary_model      : {}", settings.ai_summary_model.as_deref().unwrap_or("llama3.2:3b (default)"));
            }
        }

        Commands::Index { path, index_dir, reset } => {
            let cwd = std::env::current_dir().expect("cannot get current dir");
            let target = path.unwrap_or(cwd);
            let idx_dir = index_dir.unwrap_or_else(default_index_dir);

            if std::io::IsTerminal::is_terminal(&std::io::stderr()) {
                use ratatui::backend::CrosstermBackend;
                use ratatui::crossterm::event::{read, Event, KeyCode, KeyModifiers};
                use ratatui::crossterm::terminal::{
                    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
                };
                use ratatui::crossterm::ExecutableCommand;
                use ratatui::Terminal;
                use std::sync::atomic::{AtomicUsize, Ordering};
                use std::sync::Arc;

                // Premier passage : compter les fichiers indexables
                let total = {
                    let walker = ignore::WalkBuilder::new(&target)
                        .hidden(false)
                        .git_ignore(true)
                        .build();
                    walker
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().is_file())
                        .count() as u64
                };

                let counter = Arc::new(AtomicUsize::new(0));
                let opts = search::index::IndexOptions {
                    reset,
                    path: target,
                    index_dir: idx_dir,
                    progress: Some(counter.clone()),
                    embed_provider: config::Settings::load().embed_provider,
                };

                let _ = enable_raw_mode();
                let _ = std::io::stderr().execute(EnterAlternateScreen);
                let backend = CrosstermBackend::new(std::io::stderr());

                let handle = std::thread::spawn(move || search::index::index_directory(opts));

                let result = {
                    let mut terminal_opt = Terminal::new(backend).ok();
                    loop {
                        let done = counter.load(Ordering::Relaxed) as u64;
                        if let Some(ref mut terminal) = terminal_opt {
                            let _ = terminal.draw(|f| {
                                tui::progress::render_progress(
                                    f, f.area(), done, total.max(1), "Indexing…",
                                );
                            });
                        }
                        if handle.is_finished() {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    if let Some(ref mut terminal) = terminal_opt {
                        let _ = terminal.draw(|f| {
                            tui::progress::render_progress(
                                f, f.area(), total, total.max(1), "Indexing…",
                            );
                        });
                    }
                    let result = handle.join().expect("indexing thread panicked");
                    // Attendre q/Esc/Ctrl-C avant de fermer
                    loop {
                        if let Ok(Event::Key(key)) = read() {
                            if matches!(
                                key.code,
                                KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
                            ) || (key.code == KeyCode::Char('c')
                                && key.modifiers.contains(KeyModifiers::CONTROL))
                            {
                                break;
                            }
                        }
                    }
                    result
                };

                let _ = disable_raw_mode();
                let _ = std::io::stderr().execute(LeaveAlternateScreen);

                match result {
                    Ok(stats) => println!("Indexed {} files, {} chunks", stats.file_count, stats.chunk_count),
                    Err(e) => { eprintln!("index error: {e}"); std::process::exit(1); }
                }
            } else {
                eprintln!("Indexing {}…", target.display());
                let opts = search::index::IndexOptions {
                    reset, path: target, index_dir: idx_dir, progress: None,
                    embed_provider: config::Settings::load().embed_provider,
                };
                match search::index::index_directory(opts) {
                    Ok(stats) => println!("Indexed {} files, {} chunks", stats.file_count, stats.chunk_count),
                    Err(e) => { eprintln!("index error: {e}"); std::process::exit(1); }
                }
            }
        }

        Commands::Outline { path, kinds, depth, json } => {
            let opts = search::outline::OutlineOptions { path, depth, kinds };
            match search::outline::outline_path(opts) {
                Ok(symbols) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&symbols).unwrap());
                    } else if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                        use ratatui::backend::CrosstermBackend;
                        use ratatui::crossterm::event::{read, Event, KeyCode, KeyModifiers};
                        use ratatui::crossterm::terminal::{
                            disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
                        };
                        use ratatui::crossterm::ExecutableCommand;
                        use ratatui::Terminal;

                        let _ = enable_raw_mode();
                        let _ = std::io::stdout().execute(EnterAlternateScreen);
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
                                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => break,
                                        KeyCode::Char('c')
                                            if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                        {
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        let _ = disable_raw_mode();
                        let _ = std::io::stdout().execute(LeaveAlternateScreen);
                    } else {
                        for s in &symbols {
                            println!("{}:{} {} {}", s.file_path, s.line_start, s.kind, s.name);
                        }
                    }
                }
                Err(e) => { eprintln!("outline error: {e}"); std::process::exit(1); }
            }
        }

        Commands::Symbol { id, index_dir } => {
            let idx_dir = index_dir.unwrap_or_else(default_index_dir);
            match search::symbols::lookup_symbol(&id, &idx_dir) {
                Ok(Some(snippet)) => println!("{snippet}"),
                Ok(None) => { eprintln!("Symbol not found: {id}"); std::process::exit(1); }
                Err(e) => { eprintln!("lookup error: {e}"); std::process::exit(1); }
            }
        }

        Commands::Search { query, top_k, index_dir, json } => {
            let idx_dir = index_dir.unwrap_or_else(default_index_dir);
            let embed_provider = config::Settings::load().embed_provider;
            let opts = search::query::SearchOptions { query, top_k, index_dir: idx_dir, embed_provider };
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
                Err(e) => { eprintln!("search error: {e}"); std::process::exit(1); }
            }
        }

        Commands::Trace { action } => {
            match action {
                TraceAction::Callers { symbol, index_dir, json } => {
                    let idx_dir = index_dir.unwrap_or_else(default_index_dir);
                    match trace::callers::find_callers(&symbol, &idx_dir) {
                        Ok(edges) => {
                            if json {
                                println!("{}", serde_json::to_string_pretty(&edges).unwrap());
                            } else if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                                use ratatui::backend::CrosstermBackend;
                                use ratatui::crossterm::event::{read, Event, KeyCode, KeyModifiers};
                                use ratatui::crossterm::terminal::{
                                    disable_raw_mode, enable_raw_mode, EnterAlternateScreen,
                                    LeaveAlternateScreen,
                                };
                                use ratatui::crossterm::ExecutableCommand;
                                use ratatui::Terminal;

                                let _ = enable_raw_mode();
                                let _ = std::io::stdout().execute(EnterAlternateScreen);
                                let backend = CrosstermBackend::new(std::io::stdout());
                                if let Ok(mut terminal) = Terminal::new(backend) {
                                    loop {
                                        let _ = terminal.draw(|f| {
                                            tui::trace::render_trace(
                                                f, f.area(), &edges, &symbol, "callers",
                                            );
                                        });
                                        if let Ok(Event::Key(key)) = read() {
                                            if matches!(
                                                key.code,
                                                KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
                                            ) || (key.code == KeyCode::Char('c')
                                                && key.modifiers.contains(KeyModifiers::CONTROL))
                                            {
                                                break;
                                            }
                                        }
                                    }
                                }
                                let _ = disable_raw_mode();
                                let _ = std::io::stdout().execute(LeaveAlternateScreen);
                            } else {
                                for e in &edges {
                                    println!("{} {}:{}", e.name, e.file_path, e.line);
                                }
                            }
                        }
                        Err(e) => { eprintln!("trace error: {e}"); std::process::exit(1); }
                    }
                }
                TraceAction::Callees { symbol, depth, index_dir, json } => {
                    let idx_dir = index_dir.unwrap_or_else(default_index_dir);
                    match trace::callees::find_callees(&symbol, &idx_dir, depth) {
                        Ok(edges) => {
                            if json {
                                println!("{}", serde_json::to_string_pretty(&edges).unwrap());
                            } else if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                                use ratatui::backend::CrosstermBackend;
                                use ratatui::crossterm::event::{read, Event, KeyCode, KeyModifiers};
                                use ratatui::crossterm::terminal::{
                                    disable_raw_mode, enable_raw_mode, EnterAlternateScreen,
                                    LeaveAlternateScreen,
                                };
                                use ratatui::crossterm::ExecutableCommand;
                                use ratatui::Terminal;

                                let _ = enable_raw_mode();
                                let _ = std::io::stdout().execute(EnterAlternateScreen);
                                let backend = CrosstermBackend::new(std::io::stdout());
                                if let Ok(mut terminal) = Terminal::new(backend) {
                                    loop {
                                        let _ = terminal.draw(|f| {
                                            tui::trace::render_trace(
                                                f, f.area(), &edges, &symbol, "callees",
                                            );
                                        });
                                        if let Ok(Event::Key(key)) = read() {
                                            if matches!(
                                                key.code,
                                                KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
                                            ) || (key.code == KeyCode::Char('c')
                                                && key.modifiers.contains(KeyModifiers::CONTROL))
                                            {
                                                break;
                                            }
                                        }
                                    }
                                }
                                let _ = disable_raw_mode();
                                let _ = std::io::stdout().execute(LeaveAlternateScreen);
                            } else {
                                for e in &edges {
                                    println!("{} {}:{}", e.name, e.file_path, e.line);
                                }
                            }
                        }
                        Err(e) => { eprintln!("trace error: {e}"); std::process::exit(1); }
                    }
                }
            }
        }

        Commands::Watch { path, index_dir, background, status, stop, json } => {
            // Si --stop est demandé, arrêter le processus et terminer
            if stop {
                if let Some(state) = config::BackgroundState::load() {
                    match state.stop() {
                        Ok(()) => {
                            println!("ecotokens watch: background process (PID {}) stopped", state.pid);
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

            // Si --status est demandé, afficher l'état et terminer
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
                        println!("  Running          : {}", if is_running { "yes" } else { "no" });
                    }
                } else {
                    eprintln!("ecotokens watch: no background process running");
                    std::process::exit(1);
                }
                return;
            }

            // Si --background est demandé, daemoniser le processus
            #[cfg(unix)]
            if background {
                // Préparer les chemins pour l'état
                let cwd_temp = std::env::current_dir().expect("cannot get current dir");
                let watch_path_temp = path.as_ref().unwrap_or(&cwd_temp);
                let default_idx = default_index_dir();
                let idx_dir_temp = index_dir.as_ref().unwrap_or(&default_idx);

                // Afficher le message AVANT de daemoniser (pour que l'utilisateur le voie)
                println!("ecotokens watch: starting in background");
                println!("  Watch path: {}", watch_path_temp.display());
                println!("Use 'ecotokens watch --status' to check status");
                println!("Use 'ecotokens watch --stop' to stop");

                // Daemoniser le processus sans redirection de logs
                match daemonize::Daemonize::new().start() {
                    Ok(_) => {
                        // On est maintenant dans le processus enfant (daemon)
                        // Enregistrer l'état
                        let bg_state = config::BackgroundState::new(
                            watch_path_temp,
                            idx_dir_temp,
                            None,
                        );
                        
                        if let Err(_) = bg_state.save() {
                            // Ignorer les erreurs de sauvegarde en mode background
                        }
                    }
                    Err(e) => {
                        // Le parent reste vivant en cas d'erreur
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

            let cwd = std::env::current_dir().expect("cannot get current dir");
            let watch_path = path.unwrap_or(cwd);
            let idx_dir = index_dir.unwrap_or_else(default_index_dir);
            let watch_path_str = watch_path.display().to_string();
            let is_interactive = !background && std::io::IsTerminal::is_terminal(&std::io::stdout());

            // Compter uniquement les fichiers réellement indexables pour une progression fidèle.
            let total_files = search::index::count_indexable_files(&watch_path);

            let counter = Arc::new(AtomicUsize::new(0));
            let opts = search::index::IndexOptions {
                reset: false,
                path: watch_path.clone(),
                index_dir: idx_dir.clone(),
                progress: Some(counter.clone()),
                embed_provider: config::Settings::load().embed_provider,
            };

            // Phase A — Indexation initiale
            let report = if is_interactive {
                use ratatui::backend::CrosstermBackend;
                use ratatui::crossterm::terminal::{
                    enable_raw_mode, EnterAlternateScreen,
                };
                use ratatui::crossterm::ExecutableCommand;
                use ratatui::Terminal;

                let _ = enable_raw_mode();
                let _ = std::io::stdout().execute(EnterAlternateScreen);
                let backend = CrosstermBackend::new(std::io::stdout());

                let start = std::time::Instant::now();
                let index_handle =
                    std::thread::spawn(move || search::index::index_directory(opts));

                let index_result = {
                    let mut terminal_opt = Terminal::new(backend).ok();
                    loop {
                        let done = counter.load(Ordering::Relaxed) as u64;
                        if let Some(ref mut t) = terminal_opt {
                            let _ = t.draw(|f| {
                                tui::watch::render_indexing(
                                    f,
                                    f.area(),
                                    done,
                                    total_files.max(1),
                                );
                            });
                        }
                        if index_handle.is_finished() {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    // Afficher 100 %
                    if let Some(ref mut t) = terminal_opt {
                        let _ = t.draw(|f| {
                            tui::watch::render_indexing(
                                f,
                                f.area(),
                                total_files,
                                total_files.max(1),
                            );
                        });
                    }
                    index_handle.join().expect("indexing thread panicked")
                };

                let elapsed = start.elapsed().as_secs_f64();
                // On reste dans l'écran alternatif pour la phase de surveillance
                index_result.ok().map(|stats| tui::watch::IndexReport {
                    file_count: stats.file_count,
                    chunk_count: stats.chunk_count,
                    elapsed_secs: elapsed,
                })
            } else {
                // Mode non-interactif : indexation bloquante sur stderr
                eprintln!(
                    "ecotokens watch: indexation préalable de {} fichiers…",
                    total_files
                );
                let start = std::time::Instant::now();
                let result = search::index::index_directory(opts);
                let elapsed = start.elapsed().as_secs_f64();
                
                result.ok().map(|stats| tui::watch::IndexReport {
                    file_count: stats.file_count,
                    chunk_count: stats.chunk_count,
                    elapsed_secs: elapsed,
                })
            };

            // Phase B — Lancer le file watcher
            let (event_tx, event_rx) = std::sync::mpsc::channel::<daemon::watcher::WatchEvent>();
            let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
            let watch_path_clone = watch_path.clone();
            let idx_dir_clone = idx_dir.clone();
            let watcher_handle = std::thread::spawn(move || {
                daemon::watcher::watch_directory(
                    &watch_path_clone,
                    &idx_dir_clone,
                    event_tx,
                    stop_rx,
                )
            });

            if is_interactive {
                // Phase C — Boucle TUI watch (écran alternatif déjà actif)
                use ratatui::backend::CrosstermBackend;
                use ratatui::crossterm::event::{poll, read, Event, KeyCode, KeyModifiers};
                use ratatui::crossterm::terminal::{
                    disable_raw_mode, LeaveAlternateScreen,
                };
                use ratatui::crossterm::ExecutableCommand;
                use ratatui::Terminal;

                let backend = CrosstermBackend::new(std::io::stdout());
                if let Ok(mut terminal) = Terminal::new(backend) {
                    let mut events: Vec<daemon::watcher::WatchEvent> = Vec::new();
                    let mut watch_stats =
                        tui::watch::WatchStats { reindexed: 0, ignored: 0, errors: 0 };

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
                                report.as_ref(),
                                &watch_stats,
                            );
                        });

                        if poll(std::time::Duration::from_millis(100)).unwrap_or(false) {
                            if let Ok(Event::Key(key)) = read() {
                                if matches!(
                                    key.code,
                                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
                                ) || (key.code == KeyCode::Char('c')
                                    && key.modifiers.contains(KeyModifiers::CONTROL))
                                {
                                    break;
                                }
                            }
                        }
                    }
                }

                let _ = disable_raw_mode();
                let _ = std::io::stdout().execute(LeaveAlternateScreen);
            } else {
                // Mode background : ignorer les événements silencieusement
                while let Ok(_e) = event_rx.recv() {
                    // Événement reçu, aucune action
                }

                // Nettoyer l'état à l'arrêt
                let _ = config::BackgroundState::remove();
            }

            let _ = stop_tx.send(());
            let _ = watcher_handle.join();
        }

        Commands::Mcp { index_dir } => {
            let idx_dir = index_dir.unwrap_or_else(default_index_dir);
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime");
            rt.block_on(async {
                if let Err(e) = mcp::server::run_server(idx_dir).await {
                    eprintln!("mcp error: {e}");
                    std::process::exit(1);
                }
            });
        }
    }
}
