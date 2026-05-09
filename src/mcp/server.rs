use std::path::PathBuf;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ServerHandler,
};
use serde_json::json;

use super::tools::*;

#[derive(Debug, Clone)]
pub struct EcotokensServer {
    index_dir: PathBuf,
    // Accessed by the rmcp-generated tool handler; not referenced directly here.
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
}

#[tool_router]
impl EcotokensServer {
    #[tool(
        description = "Search the indexed codebase using BM25 + semantic search. \
        Returns results scoped to the current git project, with line numbers \
        pointing to the matching line and context lines around it. \
        Prefer this over grep for code exploration."
    )]
    fn ecotokens_search(&self, Parameters(params): Parameters<SearchParams>) -> String {
        let top_k = params.top_k.unwrap_or(5);
        let opts = crate::search::query::SearchOptions {
            query: params.query.clone(),
            top_k,
            index_dir: self.index_dir.clone(),
            embed_provider: crate::config::Settings::load().embed_provider,
        };
        match crate::search::query::search_index(opts) {
            Ok(mut results) => {
                // Scope to current git project (same logic as cmd_search)
                if let Some(root) = crate::config::git_root() {
                    results.retain(|r| root.join(&r.file_path).exists());
                }
                serde_json::to_string_pretty(&results).unwrap_or_default()
            }
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(
        description = "List symbols (functions, structs, enums, traits, etc.) \
        in a file or directory. Use to explore the structure of a file before \
        reading it in full."
    )]
    fn ecotokens_outline(&self, Parameters(params): Parameters<OutlineParams>) -> String {
        let opts = crate::search::outline::OutlineOptions {
            path: PathBuf::from(&params.path),
            depth: params.depth,
            kinds: params.kinds,
            base: None,
        };
        match crate::search::outline::outline_path(opts) {
            Ok(symbols) => serde_json::to_string_pretty(&symbols).unwrap_or_default(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(
        description = "Look up a symbol by its stable ID and return its full source code. \
        IDs have the form 'src/file.rs::name#kind' (e.g. 'src/main.rs::cmd_search#fn'). \
        Use ecotokens_outline first to discover available IDs."
    )]
    fn ecotokens_symbol(&self, Parameters(params): Parameters<SymbolParams>) -> String {
        match crate::search::symbols::lookup_symbol(&params.id, &self.index_dir) {
            Ok(Some(snippet)) => snippet,
            Ok(None) => json!({"error": format!("symbol not found: {}", params.id)}).to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Find all functions/methods that call a given symbol. \
        Returns file paths, line numbers, and caller names. \
        Use after ecotokens_search finds a definition to discover its usage sites.")]
    fn ecotokens_trace_callers(&self, Parameters(params): Parameters<TraceParams>) -> String {
        match crate::trace::callers::find_callers(&params.symbol, &self.index_dir) {
            Ok(edges) => serde_json::to_string_pretty(&edges).unwrap_or_default(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(
        description = "Find all functions/methods called by a given symbol (callees). \
        Supports recursive depth traversal. \
        Use to understand what a function depends on."
    )]
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
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(
        description = "Detect near-duplicate or structurally similar code blocks \
        in the indexed codebase and return refactoring proposals."
    )]
    fn ecotokens_duplicates(&self, Parameters(params): Parameters<DuplicatesParams>) -> String {
        let opts = crate::duplicates::DetectionOptions {
            index_dir: self.index_dir.clone(),
            threshold: params.threshold.unwrap_or(70.0),
            min_lines: params.min_lines.unwrap_or(5),
        };
        let top_k = params.top_k.unwrap_or(10);
        match crate::duplicates::detect::detect_duplicates(&opts) {
            Ok(mut groups) => {
                groups.truncate(top_k);
                crate::duplicates::proposals::format_duplicates_plain(
                    &groups,
                    opts.threshold,
                    opts.min_lines,
                )
            }
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for EcotokensServer {
    fn get_info(&self) -> ServerInfo {
        let server_info = Implementation::new("ecotokens", env!("CARGO_PKG_VERSION"));
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(server_info)
            .with_instructions(
                "ecotokens code intelligence tools. \
                Use ecotokens_search instead of grep/find for code exploration. \
                Use ecotokens_outline to inspect file/directory structure before reading. \
                Use ecotokens_symbol to retrieve a function/struct source by stable ID. \
                Use ecotokens_trace_callers/callees for call graph navigation. \
                Use ecotokens_duplicates to detect near-duplicate code.",
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
