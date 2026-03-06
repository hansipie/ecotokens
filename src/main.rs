use clap::{Parser, Subcommand};

mod config;
mod filter;
mod hook;
mod masking;
mod metrics;
mod tokens;

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
        by_command: bool,
        #[arg(long)]
        history: bool,
        #[arg(short, long, default_value = "10")]
        n: usize,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        no_tui: bool,
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
        embed_provider: Option<String>,
        #[arg(long)]
        embed_url: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Hook => eprintln!("hook: not yet implemented"),
        Commands::Filter { .. } => eprintln!("filter: not yet implemented"),
        Commands::Gain { .. } => eprintln!("gain: not yet implemented"),
        Commands::Install { .. } => eprintln!("install: not yet implemented"),
        Commands::Uninstall => eprintln!("uninstall: not yet implemented"),
        Commands::Config { .. } => eprintln!("config: not yet implemented"),
    }
}
