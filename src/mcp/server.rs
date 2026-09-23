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
    /// Cached at construction so each tool call does not re-read settings from disk.
    embed_provider: crate::config::settings::EmbedProvider,
    // Accessed by the rmcp-generated tool handler; not referenced directly here.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl EcotokensServer {
    pub fn new(index_dir: PathBuf) -> Self {
        Self {
            index_dir,
            embed_provider: crate::config::Settings::load().embed_provider,
            tool_router: Self::tool_router(),
        }
    }
}

/// Resolve a client-supplied path and confine it to the current project.
///
/// MCP tool arguments come from whatever client is connected, so a raw
/// `PathBuf::from(params.path)` would let `/etc` or `../../../.ssh` walk out of
/// the indexed project and have its contents returned. Both the root and the
/// request are canonicalised first — that resolves `..` segments *and* symlinks,
/// so a symlink inside the project pointing outside it cannot be used to escape
/// either. Relative paths are joined onto the root rather than the process cwd.
fn resolve_scoped_path(raw: &str) -> Result<PathBuf, String> {
    let root = crate::config::git_root()
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| "could not determine project root".to_string())?
        .canonicalize()
        .map_err(|e| format!("could not resolve project root: {e}"))?;

    let requested = {
        let p = std::path::Path::new(raw);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            root.join(p)
        }
    };

    let resolved = requested
        .canonicalize()
        .map_err(|e| format!("could not resolve path {raw:?}: {e}"))?;

    if !resolved.starts_with(&root) {
        return Err(format!(
            "path {raw:?} resolves outside the current project ({})",
            root.display()
        ));
    }
    Ok(resolved)
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
            embed_provider: self.embed_provider.clone(),
        };
        match crate::search::query::search_index(opts) {
            Ok(mut results) => {
                // Scope to current git project (same logic as cmd_search)
                if let Some(root) = crate::config::git_root() {
                    results.retain(|r| root.join(&r.file_path).exists());
                }
                serde_json::to_string_pretty(&results)
                    .unwrap_or_else(|e| json!({"error": e.to_string()}).to_string())
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
        let path = match resolve_scoped_path(&params.path) {
            Ok(p) => p,
            Err(e) => return json!({"error": e}).to_string(),
        };
        let opts = crate::search::outline::OutlineOptions {
            path,
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
            project_root: crate::config::git_root().or_else(|| std::env::current_dir().ok()),
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

    // Always registered, regardless of the `rewrite` feature: `#[cfg]` on an
    // individual method inside a `#[tool_router]`-attributed impl block does
    // not reliably suppress the macro's registration of that method (the
    // macro sees the raw token stream before cfg-stripping runs), so instead
    // the method body delegates to a free function that IS cleanly split by
    // `#[cfg(feature = "rewrite")]` / `#[cfg(not(...))]` below.
    #[tool(
        description = "Rewrite, retone, simplify, or translate a block of prose using the local \
        model. Returns the transformed text. Not for source code — code is refused rather than \
        transformed. Falls back to returning the original text unchanged if the local model is \
        unavailable."
    )]
    fn ecotokens_rewrite(&self, Parameters(params): Parameters<RewriteParams>) -> String {
        rewrite_tool_impl(params)
    }
}

#[cfg(feature = "rewrite")]
fn rewrite_tool_impl(params: RewriteParams) -> String {
    let settings = crate::config::Settings::load();
    let model_name = params
        .model
        .clone()
        .or_else(|| settings.rewrite_model.clone())
        .or_else(|| settings.ai_summary_model.clone())
        .unwrap_or_else(|| "llama3.2:3b".to_string());

    let provider = match crate::rewrite::provider::OllamaProvider::new(
        settings.rewrite_url.as_deref(),
        model_name.clone(),
    ) {
        Ok(p) => p,
        Err(e) => return json!({"error": e}).to_string(),
    };

    let judge = crate::jev::judge_from_settings(&settings);
    match handle_rewrite_with_judge(params, &settings, model_name, &provider, judge.as_deref()) {
        Ok(json) => json,
        Err(e) => json!({"error": e}).to_string(),
    }
}

#[cfg(not(feature = "rewrite"))]
fn rewrite_tool_impl(_params: RewriteParams) -> String {
    json!({"error": "the rewrite feature is not enabled in this build"}).to_string()
}

/// Core MCP `ecotokens_rewrite` logic, parameterized over the provider so it
/// is directly testable with `StubProvider` — the `#[tool]`-annotated method
/// above is a thin wrapper that constructs the real `OllamaProvider` and
/// resolves the model name, both of which need real `Settings`/network
/// access that a unit test should not depend on.
///
/// Returns `Err` only for validation failures (unknown mode, missing/forbidden
/// target, unrecognized language) — a genuine tool error with zero provider
/// calls (contracts/mcp-rewrite-tool.md). A model/generation failure is never
/// an `Err` here: it surfaces as `Ok` with `status: "fallback"` in the JSON,
/// a normal successful tool result (FR-031).
///
/// `diff_path` is deliberately omitted from the returned JSON even though
/// `RewriteResult` may carry one — the path is a local filesystem detail the
/// agent has no use for (contracts/mcp-rewrite-tool.md).
#[cfg(feature = "rewrite")]
// Used by the library crate and tests; unused in the binary.
#[allow(dead_code)]
pub fn handle_rewrite(
    params: RewriteParams,
    settings: &crate::config::Settings,
    model_name: String,
    provider: &dyn crate::rewrite::provider::RewriteProvider,
) -> Result<String, String> {
    handle_rewrite_with_judge(params, settings, model_name, provider, None)
}

/// [`handle_rewrite`] with an optional Jev judge (see
/// `rewrite::rewrite_with_judge`); `None` is exactly [`handle_rewrite`].
#[cfg(feature = "rewrite")]
pub fn handle_rewrite_with_judge(
    params: RewriteParams,
    settings: &crate::config::Settings,
    model_name: String,
    provider: &dyn crate::rewrite::provider::RewriteProvider,
    judge: Option<&dyn crate::jev::Judge>,
) -> Result<String, String> {
    let mode = crate::rewrite::modes::Mode::parse(&params.mode, params.target.as_deref())
        .map_err(|e| e.to_string())?;

    let request = crate::rewrite::RewriteRequest {
        text: params.text,
        mode,
        model: model_name,
        timeout: std::time::Duration::from_millis(settings.rewrite_timeout_ms),
        origin: crate::rewrite::Origin::Mcp,
        truncation_ratio: settings.rewrite_truncation_ratio,
        context_tokens: settings.rewrite_context_tokens,
        // Diff saving applies identically to agent invocations (FR-032) —
        // there is no MCP-level override, so this follows config alone.
        save_diff: settings.rewrite_save_diff,
        diff_dir: settings
            .rewrite_diff_dir
            .clone()
            .unwrap_or_else(std::env::temp_dir),
        diff_retention: settings.rewrite_diff_retention,
    };

    let jev = judge.map(|j| crate::jev::JevContext::new(j, settings));
    let result =
        crate::rewrite::rewrite_with_judge(request, provider, jev).map_err(|e| e.to_string())?;

    Ok(json!({
        "status": result.status,
        "reason": result.reason,
        "mode": result.mode,
        "target": result.target,
        "model": result.model,
        "text": result.text,
        "tokens_in": result.tokens_in,
        "tokens_out": result.tokens_out,
        "chunk_count": result.chunk_count,
        "duration_ms": result.duration_ms,
    })
    .to_string())
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

#[cfg(test)]
mod tests {
    use super::resolve_scoped_path;

    // `resolve_scoped_path` is a security control: without it, `ecotokens_outline`
    // returns the contents of any file an MCP client names. These run against the
    // crate's own git root, which is the project resolved at test time.

    #[test]
    fn accepts_a_relative_path_inside_the_project() {
        let resolved = resolve_scoped_path("src/mcp/server.rs").expect("in-project path");
        assert!(resolved.ends_with("src/mcp/server.rs"));
    }

    #[test]
    fn rejects_an_absolute_path_outside_the_project() {
        let err = resolve_scoped_path("/etc").expect_err("must not escape the project");
        assert!(err.contains("outside the current project"), "got: {err}");
    }

    #[test]
    fn rejects_a_traversal_escape() {
        let err = resolve_scoped_path("../../../../etc/passwd")
            .expect_err("must not escape via .. segments");
        // Either it resolves outside the root, or it does not exist from here —
        // both refuse to hand back the file, which is what matters.
        assert!(
            err.contains("outside the current project") || err.contains("could not resolve path"),
            "got: {err}"
        );
    }
}
