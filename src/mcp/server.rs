use std::path::PathBuf;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ServerHandler,
};

use super::tools::*;

#[derive(Debug, Clone)]
pub struct EcotokensServer {
    index_dir: PathBuf,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl EcotokensServer {
    pub fn new(index_dir: PathBuf) -> Self {
        Self {
            index_dir,
            tool_router: Self::tool_router(),
        }
    }

    fn resolve_run_cwd(&self, cwd: Option<&str>) -> Result<PathBuf, String> {
        if let Some(cwd) = cwd {
            let path = PathBuf::from(cwd);
            let canonical = std::fs::canonicalize(&path)
                .map_err(|e| format!("cwd does not exist or is not accessible: {cwd}: {e}"))?;
            if !canonical.is_dir() {
                return Err(format!("cwd is not a directory: {cwd}"));
            }
            return Ok(canonical);
        }

        if let Ok(ws_root) = std::env::var("ECOTOKENS_WORKSPACE_ROOT") {
            if let Ok(canonical) = std::fs::canonicalize(&ws_root) {
                if canonical.is_dir() {
                    return Ok(canonical);
                }
            }
        }

        if let Ok(path) = std::env::current_dir() {
            if path.is_dir() {
                return Ok(path);
            }
        }

        Err("unable to resolve a valid working directory".to_string())
    }
}

#[tool_router]
impl EcotokensServer {
    #[tool(description = "Search the indexed codebase using BM25 fulltext search")]
    fn ecotokens_search(&self, Parameters(params): Parameters<SearchParams>) -> String {
        let opts = crate::search::query::SearchOptions {
            query: params.query,
            top_k: params.top_k.unwrap_or(5),
            index_dir: self.index_dir.clone(),
            embed_provider: crate::config::Settings::load().embed_provider,
        };
        match crate::search::query::search_index(opts) {
            Ok(results) => serde_json::to_string_pretty(&results).unwrap_or_default(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    #[tool(description = "List symbols (functions, structs, etc.) in a file or directory")]
    fn ecotokens_outline(&self, Parameters(params): Parameters<OutlineParams>) -> String {
        let opts = crate::search::outline::OutlineOptions {
            path: PathBuf::from(&params.path),
            depth: params.depth,
            kinds: params.kinds,
        };
        match crate::search::outline::outline_path(opts) {
            Ok(symbols) => serde_json::to_string_pretty(&symbols).unwrap_or_default(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    #[tool(description = "Look up a symbol by its stable ID and return its source code")]
    fn ecotokens_symbol(&self, Parameters(params): Parameters<SymbolParams>) -> String {
        match crate::search::symbols::lookup_symbol(&params.id, &self.index_dir) {
            Ok(Some(snippet)) => snippet,
            Ok(None) => format!("{{\"error\": \"symbol not found: {}\"}}", params.id),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    #[tool(description = "Find all callers of a symbol in the indexed codebase")]
    fn ecotokens_trace_callers(&self, Parameters(params): Parameters<TraceParams>) -> String {
        match crate::trace::callers::find_callers(&params.symbol, &self.index_dir) {
            Ok(edges) => serde_json::to_string_pretty(&edges).unwrap_or_default(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    #[tool(description = "Find all callees (functions called by) a symbol")]
    fn ecotokens_trace_callees(
        &self,
        Parameters(params): Parameters<TraceCalleesParams>,
    ) -> String {
        match crate::trace::callees::find_callees(
            &params.symbol,
            &self.index_dir,
            params.depth.unwrap_or(1),
        ) {
            Ok(edges) => serde_json::to_string_pretty(&edges).unwrap_or_default(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    #[tool(
        description = "Execute a shell command and return token-optimized output. \
        Use this instead of the terminal for commands that may produce large output \
        (e.g. 'git log', 'cargo test', 'find . -type f'). \
        Output is automatically filtered and compressed to save context tokens, \
        and token savings are recorded in the metrics store."
    )]
    fn ecotokens_run(&self, Parameters(params): Parameters<RunParams>) -> String {
        let command = params.command.trim();
        if command.is_empty() {
            return "{\"error\": \"empty command\"}".to_string();
        }

        let cwd = match self.resolve_run_cwd(params.cwd.as_deref()) {
            Ok(path) => path,
            Err(e) => return format!("{{\"error\": \"{e}\"}}"),
        };

        // Only attribute metrics to a project when the cwd is authoritative:
        // either explicitly provided by the caller, or from ECOTOKENS_WORKSPACE_ROOT.
        // Falling back to the server process cwd risks attributing commands to the
        // wrong project (e.g. VS Code's own directory when started by Copilot).
        let authoritative_cwd =
            params.cwd.is_some() || std::env::var("ECOTOKENS_WORKSPACE_ROOT").is_ok();

        let start = std::time::Instant::now();
        // The command string is trusted to come from an AI agent, not from arbitrary
        // user input, so shell injection is an accepted risk here.
        let output = std::process::Command::new("bash")
            .arg("-lc")
            .arg(command)
            .current_dir(&cwd)
            .output();
        let raw = match output {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
                if stderr.is_empty() {
                    stdout
                } else {
                    format!("{stdout}{stderr}")
                }
            }
            Err(e) => return format!("{{\"error\": \"failed to run command: {e}\"}}"),
        };
        let duration_ms = start.elapsed().as_millis() as u32;
        let metrics_cwd = authoritative_cwd.then_some(cwd.as_path());
        let (filtered, _before, _after) =
            crate::filter::run_filter_pipeline_with_cwd(command, &raw, duration_ms, metrics_cwd);
        filtered
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for EcotokensServer {
    fn get_info(&self) -> ServerInfo {
        let server_info = Implementation::new("ecotokens", env!("CARGO_PKG_VERSION"));
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(server_info)
            .with_instructions(
                "ecotokens: token-saving companion for Claude Code and GitHub Copilot. \
                Tools: search (BM25 codebase search), outline (list symbols), \
                symbol (source by ID), trace callers/callees (call graph), \
                run (execute shell commands with token-optimized output).",
            )
    }
}

/// Start the MCP server on stdio.
pub async fn run_server(index_dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let server = EcotokensServer::new(index_dir);
    let transport = rmcp::transport::io::stdio();
    let service = rmcp::service::serve_server(server, transport).await?;
    service.waiting().await?;
    Ok(())
}
