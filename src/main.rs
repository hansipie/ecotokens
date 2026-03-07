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
        by_project: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        model: Option<String>,
    },
    /// Install ecotokens hook in ~/.claude/settings.json
    Install {
        #[arg(long)]
        with_mcp: bool,
    },
    /// Remove ecotokens hook from ~/.claude/settings.json
    Uninstall,
    /// Show or update configuration
    Config {
        #[arg(long)]
        json: bool,
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

fn detect_family(command: &str) -> metrics::store::CommandFamily {
    use metrics::store::CommandFamily;
    let cmd = command.trim();
    if cmd.starts_with("git ") { CommandFamily::Git }
    else if cmd.starts_with("cargo ") { CommandFamily::Cargo }
    else if is_cpp_command(cmd) { CommandFamily::Cpp }
    else if cmd.starts_with("python") || cmd.starts_with("pytest") || cmd.starts_with("pip ") || cmd.starts_with("ruff ") || cmd.starts_with("uv ") { CommandFamily::Python }
    else if cmd.starts_with("ls") || cmd.starts_with("find") || cmd.starts_with("tree") { CommandFamily::Fs }
    else { CommandFamily::Generic }
}

fn is_cpp_command(command: &str) -> bool {
    use std::path::Path;

    let Some(program) = command.split_whitespace().next() else {
        return false;
    };
    let Some(program) = Path::new(program).file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    matches!(
        program,
        "gcc" | "g++" | "cc" | "c++" | "clang" | "clang++" | "clang-cl" | "make" | "cmake" | "ninja"
    )
}

fn apply_filter(command: &str, output: &str) -> String {
    use metrics::store::CommandFamily;
    let ext = std::path::Path::new(command)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match detect_family(command) {
        CommandFamily::Git => filter::git::filter_git(command, output),
        CommandFamily::Cargo => filter::cargo::filter_cargo(command, output),
        CommandFamily::Cpp => filter::cpp::filter_cpp(command, output),
        CommandFamily::Python => filter::python::filter_python(command, output),
        CommandFamily::Fs => filter::fs::filter_fs(command, output),
        CommandFamily::Markdown => filter::markdown::filter_markdown(output),
        CommandFamily::ConfigFile => filter::config_file::filter_config_file(output, ext),
        _ => filter::generic::filter_generic(output, 200, 51200),
    }
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

            let (masked, redacted) = masking::mask(&raw);
            let filtered = apply_filter(&command, &masked);

            let duration_ms = start.elapsed().as_millis() as u32;
            let tokens_before = tokens::estimate_tokens(&raw) as u32;
            let tokens_after = tokens::estimate_tokens(&filtered) as u32;

            if debug {
                eprintln!("[ecotokens debug] command={command} tokens_before={tokens_before} tokens_after={tokens_after}");
            }

            print!("{filtered}");

            // Record metrics
            if let Some(path) = metrics::store::metrics_path() {
                let mode = if tokens_after < tokens_before {
                    metrics::store::FilterMode::Filtered
                } else {
                    metrics::store::FilterMode::Passthrough
                };
                let family = detect_family(&command);
                let git_root = std::process::Command::new("git")
                    .args(["rev-parse", "--show-toplevel"])
                    .output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
                let rec = metrics::store::Interception::new(
                    command, family, git_root,
                    tokens_before, tokens_after, mode, redacted, duration_ms,
                );
                let _ = metrics::store::append_to(&path, &rec);
            }
        }

        Commands::Gain { period, by_project, json, model } => {
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
                    loop {
                        let ts = chrono::Utc::now().format("%H:%M:%S").to_string();
                        let _ = terminal.draw(|f| {
                            tui::gain::render_gain(f, f.area(), &report, &items, Some(&ts), by_project);
                        });
                        if poll(std::time::Duration::from_secs(1)).unwrap_or(false) {
                            if let Ok(Event::Key(key)) = read() {
                                if matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc)
                                    || (key.code == KeyCode::Char('c')
                                        && key.modifiers.contains(KeyModifiers::CONTROL))
                                {
                                    break;
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
                println!("Cost avoided   : ${:.4} USD", report.cost_avoided_usd);
                if by_project {
                    println!("\nBy project:");
                    for (k, v) in &report.by_project {
                        println!("  {k}: {} cmds", v.count);
                    }
                }
            }
        }

        Commands::Install { with_mcp } => {
            let path = default_settings_path();
            match install::install_hook(&path, with_mcp) {
                Ok(()) => println!("ecotokens hook installed → {}", path.display()),
                Err(e) => { eprintln!("install error: {e}"); std::process::exit(1); }
            }
        }

        Commands::Uninstall => {
            let path = default_settings_path();
            match install::uninstall_hook(&path) {
                Ok(()) => println!("ecotokens hook removed"),
                Err(e) => { eprintln!("uninstall error: {e}"); std::process::exit(1); }
            }
        }

        Commands::Config { json } => {
            let settings = config::Settings::load();
            if json {
                println!("{}", serde_json::to_string_pretty(&settings).unwrap());
            } else {
                println!("debug          : {}", settings.debug);
                println!("threshold_lines: {}", settings.summary_threshold_lines);
                println!("threshold_bytes: {}", settings.summary_threshold_bytes);
                println!("exclusions     : {:?}", settings.exclusions);
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
            let opts = search::query::SearchOptions { query, top_k, index_dir: idx_dir };
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
