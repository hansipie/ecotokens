use rmcp::schemars;
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    #[schemars(description = "The search query")]
    pub query: String,
    #[schemars(description = "Maximum number of results (default: 5)")]
    pub top_k: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OutlineParams {
    #[schemars(description = "Path to a file or directory")]
    pub path: String,
    #[schemars(description = "Recursion depth for directories")]
    pub depth: Option<u32>,
    #[schemars(description = "Filter by symbol kinds (fn, struct, impl, etc.)")]
    pub kinds: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SymbolParams {
    #[schemars(description = "Stable symbol ID (e.g., lib.rs::greet#fn)")]
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TraceParams {
    #[schemars(description = "Symbol name to trace")]
    pub symbol: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TraceCalleesParams {
    #[schemars(description = "Symbol name to trace")]
    pub symbol: String,
    #[schemars(description = "Recursion depth (default: 1)")]
    pub depth: Option<u32>,
}
