use std::path::PathBuf;

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
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
            Err(e) => format!("{{\"error\": \"{e}\"}}")
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
            Err(e) => format!("{{\"error\": \"{e}\"}}")
        }
    }

    #[tool(description = "Look up a symbol by its stable ID and return its source code")]
    fn ecotokens_symbol(&self, Parameters(params): Parameters<SymbolParams>) -> String {
        match crate::search::symbols::lookup_symbol(&params.id, &self.index_dir) {
            Ok(Some(snippet)) => snippet,
            Ok(None) => format!("{{\"error\": \"symbol not found: {}\"}}", params.id),
            Err(e) => format!("{{\"error\": \"{e}\"}}")
        }
    }

    #[tool(description = "Find all callers of a symbol in the indexed codebase")]
    fn ecotokens_trace_callers(&self, Parameters(params): Parameters<TraceParams>) -> String {
        match crate::trace::callers::find_callers(&params.symbol, &self.index_dir) {
            Ok(edges) => serde_json::to_string_pretty(&edges).unwrap_or_default(),
            Err(e) => format!("{{\"error\": \"{e}\"}}")
        }
    }

    #[tool(description = "Find all callees (functions called by) a symbol")]
    fn ecotokens_trace_callees(&self, Parameters(params): Parameters<TraceCalleesParams>) -> String {
        match crate::trace::callees::find_callees(&params.symbol, &self.index_dir, params.depth.unwrap_or(1)) {
            Ok(edges) => serde_json::to_string_pretty(&edges).unwrap_or_default(),
            Err(e) => format!("{{\"error\": \"{e}\"}}")
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for EcotokensServer {
    fn get_info(&self) -> ServerInfo {
        let server_info = Implementation::new("ecotokens", env!("CARGO_PKG_VERSION"));
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(server_info)
            .with_instructions("ecotokens: token-saving companion for Claude Code — search, outline, symbol lookup, and call graph tracing")
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
