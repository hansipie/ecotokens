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
        daemon: bool,
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
                    loop {
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
                        if poll(std::time::Duration::from_secs(1)).unwrap_or(false) {
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
                        } else {
                            items = read_from(&path).unwrap_or_default();
                            report = aggregate(&items, p.clone(), model.as_deref().unwrap_or("sonnet"));
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

        Commands::Install { with_mcp, target } => {
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

        Commands::Watch { path, index_dir, daemon } => {
            let cwd = std::env::current_dir().expect("cannot get current dir");
            let watch_path = path.unwrap_or(cwd);
            let idx_dir = index_dir.unwrap_or_else(default_index_dir);

            let (event_tx, event_rx) = std::sync::mpsc::channel::<daemon::watcher::WatchEvent>();
            let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

            let watch_path_clone = watch_path.clone();
            let idx_dir_clone = idx_dir.clone();
            let watcher_handle = std::thread::spawn(move || {
                daemon::watcher::watch_directory(&watch_path_clone, &idx_dir_clone, event_tx, stop_rx)
            });

            let watch_path_str = watch_path.display().to_string();

            if daemon || !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                // Mode non-interactif : écrire les événements sur stdout
                eprintln!("ecotokens watch: surveillance de {} (Ctrl-C pour arrêter)", watch_path.display());
                while let Ok(e) = event_rx.recv() {
                    println!("[{}] {} {}", e.timestamp, e.path.display(), e.status);
                }
            } else {
                // Mode TUI interactif
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
                    let mut events: Vec<daemon::watcher::WatchEvent> = Vec::new();

                    loop {
                        // Drainer les nouveaux événements
                        while let Ok(e) = event_rx.try_recv() {
                            events.push(e);
                        }

                        let _ = terminal.draw(|f| {
                            tui::watch::render_watch(f, f.area(), &events, &watch_path_str);
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
