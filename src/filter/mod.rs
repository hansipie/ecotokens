pub mod ai_summary;
pub mod aws;
pub mod cargo;
pub mod config_file;
pub mod container;
pub mod cpp;
pub mod cwd;
pub mod db;
pub mod fs;
pub mod generic;
pub mod gh;
pub mod git;
pub mod go;
pub mod grep;
pub mod js;
pub mod markdown;
pub mod network;
pub mod python;

pub use cwd::project_root_for_cwd;

use crate::metrics::store::CommandFamily;

/// `prog` is the already-basename-normalised program name (see `detect_family`).
fn is_cpp_command(prog: &str) -> bool {
    matches!(
        prog,
        "gcc"
            | "g++"
            | "cc"
            | "c++"
            | "clang"
            | "clang++"
            | "clang-cl"
            | "make"
            | "cmake"
            | "ninja"
    )
}

/// Extrait la commande passée à `-c` dans `bash -c "..."`, `sh -c '...'`, etc.
fn extract_shell_c_inner(cmd: &str) -> Option<&str> {
    let pos = cmd.find(" -c ")?;
    let inner = cmd[pos + 4..].trim();
    if inner.len() >= 2 {
        let b = inner.as_bytes();
        if (b[0] == b'"' && b[b.len() - 1] == b'"') || (b[0] == b'\'' && b[b.len() - 1] == b'\'') {
            return Some(&inner[1..inner.len() - 1]);
        }
    }
    Some(inner)
}

pub fn detect_family(command: &str) -> CommandFamily {
    let cmd = command.trim();

    // Hermes Agent tool result labels: "hermes-tool:<tool_name>"
    if let Some(tool) = cmd.strip_prefix("hermes-tool:") {
        return match tool {
            "read_file" | "list_directory" | "create_file" | "edit_file" | "delete_file" => {
                CommandFamily::Fs
            }
            "search_files" | "find_files" | "search_in_file" => CommandFamily::Grep,
            "browser_snapshot" | "browser_navigate" | "browser_click" | "browser_type"
            | "web_fetch" | "web_search" => CommandFamily::Network,
            "run_python_code" | "execute_python" => CommandFamily::Python,
            "run_shell_command" | "execute_bash" => CommandFamily::Generic,
            _ => CommandFamily::Generic,
        };
    }

    // Normalize the first token to its basename so that absolute paths
    // (/usr/bin/git), venv paths (.venv/bin/pytest) and version managers
    // (~/.cargo/bin/cargo) are all matched by their bare command name.
    let raw_prog = cmd.split_whitespace().next().unwrap_or("");
    let prog = std::path::Path::new(raw_prog)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(raw_prog);

    if prog == "git" {
        CommandFamily::Git
    } else if prog == "cargo" {
        CommandFamily::Cargo
    } else if is_cpp_command(prog) {
        CommandFamily::Cpp
    } else if prog.starts_with("python")
        || matches!(
            prog,
            "pytest" | "pip" | "ruff" | "mypy" | "uv" | "poetry" | "pipx"
        )
    {
        CommandFamily::Python
    } else if matches!(prog, "ls" | "find" | "tree" | "diff" | "wc") {
        CommandFamily::Fs
    } else if prog == "go" || cmd.contains("golangci-lint") {
        CommandFamily::Go
    } else if matches!(
        prog,
        "npm"
            | "pnpm"
            | "npx"
            | "yarn"
            | "tsc"
            | "vitest"
            | "jest"
            | "mocha"
            | "eslint"
            | "prettier"
            | "next"
    ) || cmd.contains("playwright")
        || cmd.contains("prisma")
    {
        CommandFamily::Js
    } else if prog == "gh" {
        CommandFamily::Gh
    } else if matches!(prog, "docker" | "podman" | "kubectl") {
        CommandFamily::Container
    } else if matches!(prog, "grep" | "rg") {
        CommandFamily::Grep
    } else if prog == "aws" {
        CommandFamily::Aws
    } else if matches!(prog, "curl" | "wget") {
        CommandFamily::Network
    } else if prog == "psql" {
        CommandFamily::Db
    } else if matches!(prog, "bash" | "sh" | "zsh" | "dash") {
        // `bash -c "git status"` → détecter la famille de la commande interne
        extract_shell_c_inner(cmd)
            .map(detect_family)
            .unwrap_or(CommandFamily::Generic)
    } else {
        CommandFamily::Generic
    }
}

// Used by the library crate and tests; unused in the binary.
#[allow(dead_code)]
pub fn apply_filter(command: &str, output: &str) -> String {
    apply_filter_with(command, output, None)
}

/// [`apply_filter`] with an optional Jev judge used for generic-family line
/// selection (`generic::filter_generic_with_judge`). `None` is exactly
/// [`apply_filter`].
pub fn apply_filter_with(
    command: &str,
    output: &str,
    jev: Option<crate::jev::JevContext<'_>>,
) -> String {
    let ext = std::path::Path::new(command)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match detect_family(command) {
        CommandFamily::Git => git::filter_git(command, output),
        CommandFamily::Cargo => cargo::filter_cargo(command, output),
        CommandFamily::Cpp => cpp::filter_cpp(command, output),
        CommandFamily::Python => python::filter_python(command, output),
        CommandFamily::Fs => fs::filter_fs(command, output),
        CommandFamily::Markdown => markdown::filter_markdown(output),
        CommandFamily::ConfigFile => config_file::filter_config_file(output, ext),
        CommandFamily::Go => go::filter_go(command, output),
        CommandFamily::Js => js::filter_js(command, output),
        CommandFamily::Gh => gh::filter_gh(command, output),
        CommandFamily::Container => container::filter_container(command, output),
        CommandFamily::Grep => grep::filter_grep(output),
        CommandFamily::Aws => aws::filter_aws(output),
        CommandFamily::Network => network::filter_network(command, output),
        CommandFamily::Db => db::filter_db(output),
        CommandFamily::Generic => match jev {
            Some(ctx) => generic::filter_generic_with_judge(output, 200, 51200, ctx),
            None => generic::filter_generic(output, 200, 51200),
        },
        CommandFamily::NativeRead => output.to_string(),
    }
}

/// Run the full filter pipeline with an optional working directory for git_root detection.
/// Returns `(filtered_output, tokens_before, tokens_after)`.
#[cfg_attr(test, allow(unused_variables))]
pub fn run_filter_pipeline_with_cwd(
    command: &str,
    raw: &str,
    duration_ms: u32,
    cwd: Option<&std::path::Path>,
    hook_type: crate::metrics::store::HookType,
) -> (String, u32, u32) {
    let settings = crate::config::Settings::load();
    let (masked, redacted) = crate::masking::mask(raw);
    let filtered = if raw.len() < 200 {
        masked.clone()
    } else {
        // Input is already masked here, so line selection never sends secrets.
        let judge = if settings.jev_line_select_enabled {
            crate::jev::judge_from_settings(&settings)
        } else {
            None
        };
        let jev = judge
            .as_deref()
            .map(|j| crate::jev::JevContext::new(j, &settings));
        let mut f = apply_filter_with(command, &masked, jev);
        let masked_tokens = crate::tokens::count_tokens(&masked) as u32;
        let filtered_tokens = crate::tokens::count_tokens(&f) as u32;
        if f == masked || (masked_tokens > 0 && filtered_tokens >= masked_tokens) {
            let candidate = ai_summary::ai_summary_or_fallback(&masked, &settings);
            let candidate_tokens = crate::tokens::count_tokens(&candidate) as u32;
            if masked_tokens > 0 && candidate_tokens < masked_tokens {
                f = candidate;
            } else {
                f = masked.clone();
            }
        }
        f
    };

    let filtered = if settings.abbreviations_enabled {
        crate::abbreviations::abbreviate(&filtered, &settings).0
    } else {
        filtered
    };

    let tokens_before = crate::tokens::count_tokens(raw) as u32;
    let filtered_tokens = crate::tokens::count_tokens(&filtered) as u32;
    // Compare the filtered result against the *masked* baseline (what filtering
    // actually operates on), not the raw text: masking can legitimately expand
    // token count, and comparing filtered vs raw would unfairly discard a good
    // filtered result. `tokens_before` stays raw so user-facing savings reflect
    // the original output.
    let masked_tokens = crate::tokens::count_tokens(&masked) as u32;
    let (filtered, tokens_after) = if filtered_tokens > masked_tokens {
        (masked.clone(), masked_tokens)
    } else {
        (filtered, filtered_tokens)
    };

    #[cfg(not(test))]
    if let Some(path) = crate::metrics::store::metrics_path() {
        let mode = if tokens_after < tokens_before {
            crate::metrics::store::FilterMode::Filtered
        } else {
            #[cfg(feature = "ai-summary")]
            {
                crate::metrics::store::FilterMode::Summarized
            }
            #[cfg(not(feature = "ai-summary"))]
            {
                crate::metrics::store::FilterMode::Filtered
            }
        };
        let family = detect_family(command);
        let effective_cwd = cwd
            .map(|p| p.to_path_buf())
            .or_else(|| std::env::current_dir().ok());
        let git_root = effective_cwd.as_deref().and_then(project_root_for_cwd);
        let rec = crate::metrics::store::Interception::new(
            command.to_string(),
            family,
            git_root,
            tokens_before,
            tokens_after,
            mode,
            redacted,
            duration_ms,
            Some(masked),
            Some(filtered.clone()),
        )
        .with_hook_type(hook_type.clone());
        let _ = crate::metrics::store::append_to(&path, &rec);
    }

    // ── Automatic rewrite stage (opt-in, User Story 6) ──────────────────────
    // Deliberately runs AFTER the anti-expansion clamp above and its metrics
    // recording, never before: rewriting routinely *grows* text (paraphrase,
    // translation), and the clamp above would silently discard any such
    // transformation if this ran earlier (research.md §10). Off by default —
    // `rewrite_auto_enabled` — so the default interception path is completely
    // unaffected by this stage existing at all (FR-034, SC-010).
    #[cfg(all(not(test), feature = "rewrite"))]
    {
        if settings.rewrite_auto_enabled && !redacted {
            if let Some((rewritten, tokens_after_auto)) =
                try_auto_rewrite(&filtered, tokens_after, &settings)
            {
                record_auto_rewrite_metrics(
                    command,
                    cwd,
                    hook_type,
                    tokens_after,
                    tokens_after_auto,
                );
                return (rewritten, tokens_before, tokens_after_auto);
            }
        }
    }

    (filtered, tokens_before, tokens_after)
}

/// Attempt the automatic prose-rewrite stage. Pre-gated by size threshold and
/// content classification before any provider call (FR-035, FR-036); secret
/// exclusion is the caller's `!redacted` check above (FR-039). Returns `None`
/// on any gate rejection, config problem, or transformation failure — meaning
/// "pass through unchanged," never a partial or corrupted result.
#[cfg(all(not(test), feature = "rewrite"))]
fn try_auto_rewrite(
    text: &str,
    tokens: u32,
    settings: &crate::config::Settings,
) -> Option<(String, u32)> {
    if tokens < settings.rewrite_auto_min_tokens {
        return None;
    }
    let judge = crate::jev::judge_from_settings(settings);
    let jev = judge
        .as_deref()
        .map(|j| crate::jev::JevContext::new(j, settings));
    if crate::rewrite::detect::classify_with(text, jev)
        != crate::rewrite::detect::ContentKind::Prose
    {
        return None;
    }

    let mode_name = settings.rewrite_auto_mode.as_deref()?;
    let mode =
        crate::rewrite::modes::Mode::parse(mode_name, settings.rewrite_auto_target.as_deref())
            .ok()?;

    let model_name = settings
        .rewrite_model
        .clone()
        .or_else(|| settings.ai_summary_model.clone())
        .unwrap_or_else(|| "llama3.2:3b".to_string());
    let provider = crate::rewrite::provider::OllamaProvider::new(
        settings.rewrite_url.as_deref(),
        model_name.clone(),
    )
    .ok()?;

    let request = crate::rewrite::RewriteRequest {
        text: text.to_string(),
        mode,
        model: model_name,
        // Stricter interactive budget than the CLI/MCP surfaces: this stage
        // runs *in addition to* existing filtering, not instead of it
        // (FR-037).
        timeout: std::time::Duration::from_millis(settings.rewrite_auto_timeout_ms),
        origin: crate::rewrite::Origin::AutoPipeline,
        truncation_ratio: settings.rewrite_truncation_ratio,
        context_tokens: settings.rewrite_context_tokens,
        save_diff: settings.rewrite_save_diff,
        diff_dir: settings
            .rewrite_diff_dir
            .clone()
            .unwrap_or_else(std::env::temp_dir),
        diff_retention: settings.rewrite_diff_retention,
    };

    match crate::rewrite::rewrite_with_judge(request, &provider, jev) {
        Ok(result) if result.status == crate::rewrite::Status::Transformed => {
            Some((result.text, result.tokens_out))
        }
        Ok(result) => {
            // Model unreachable, timeout, or unusable response — warn at
            // most once per session, not once per interception (FR-038).
            if let Some(reason) = result.reason {
                crate::rewrite::provider::warn_auto_pipeline_unreachable_once(&reason);
            }
            None
        }
        Err(_) => None,
    }
}

/// Emit the second `FilterMode::Rewritten` metrics row for an auto-stage
/// transformation, distinct from the compression row already recorded above
/// for the same interception (research.md §10). `hook_type` here is never
/// `Cli`/`Mcp` — those origins record their own rows inside
/// `rewrite::rewrite()` — which is exactly what lets `aggregate()` attribute
/// this row to `AutoPipeline` overhead rather than excluding it outright
/// (FR-041a, SC-012).
#[cfg(all(not(test), feature = "rewrite"))]
fn record_auto_rewrite_metrics(
    command: &str,
    cwd: Option<&std::path::Path>,
    hook_type: crate::metrics::store::HookType,
    tokens_before: u32,
    tokens_after: u32,
) {
    let Some(path) = crate::metrics::store::metrics_path() else {
        return;
    };
    let effective_cwd = cwd
        .map(|p| p.to_path_buf())
        .or_else(|| std::env::current_dir().ok());
    let git_root = effective_cwd.as_deref().and_then(project_root_for_cwd);
    let mut rec = crate::metrics::store::Interception::new(
        command.to_string(),
        detect_family(command),
        git_root,
        tokens_before,
        tokens_after,
        crate::metrics::store::FilterMode::Rewritten,
        false,
        0,
        None,
        None,
    )
    .with_hook_type(hook_type);
    rec.savings_pct = 0.0;
    let _ = crate::metrics::store::append_to(&path, &rec);
}
