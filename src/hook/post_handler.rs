use std::io::{self, Read};

use crate::config::settings::Settings;
use crate::metrics::store::{CommandFamily, FilterMode, HookType, Interception};
use serde::{Deserialize, Serialize};

use super::glob_handler::handle_glob;
use super::grep_handler::handle_grep;
use super::read_handler::handle_read;

/// Map Gemini CLI AfterTool tool names to canonical ecotokens names.
fn normalize_gemini_tool_name(name: &str) -> &str {
    match name {
        "read_file" => "Read",
        "search_file_content" => "Grep",
        "list_directory" => "Glob",
        other => other,
    }
}

/// Map Qwen Code PostToolUse tool names to canonical ecotokens names.
fn normalize_qwen_tool_name(name: &str) -> &str {
    match name {
        "read_file" => "Read",
        "search_files" => "Grep",
        "list_dir" | "list_directory" => "Glob",
        other => other,
    }
}

#[derive(Debug, Clone, Serialize)]
struct GeminiAfterToolOutput {
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: GeminiAfterToolSpecificOutput,
}

#[derive(Debug, Clone, Serialize)]
struct GeminiAfterToolSpecificOutput {
    #[serde(rename = "hookEventName")]
    hook_event_name: String,
    decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl GeminiAfterToolOutput {
    fn allow() -> Self {
        GeminiAfterToolOutput {
            hook_specific_output: GeminiAfterToolSpecificOutput {
                hook_event_name: "AfterTool".to_string(),
                decision: "allow".to_string(),
                reason: None,
            },
        }
    }

    fn deny(reason: String) -> Self {
        GeminiAfterToolOutput {
            hook_specific_output: GeminiAfterToolSpecificOutput {
                hook_event_name: "AfterTool".to_string(),
                decision: "deny".to_string(),
                reason: Some(reason),
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostHookInput {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_response: serde_json::Value,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostHookOutput {
    #[serde(rename = "hookSpecificOutput", skip_serializing_if = "Option::is_none")]
    pub hook_specific_output: Option<PostHookSpecificOutput>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostHookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String,
    #[serde(rename = "additionalContext", skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PostFilterResult {
    Filtered {
        output: String,
        tokens_before: u32,
        tokens_after: u32,
        content_before: String,
    },
    Passthrough,
}

impl PostHookOutput {
    pub fn passthrough() -> Self {
        PostHookOutput {
            hook_specific_output: None,
        }
    }

    pub fn with_context(context: String) -> Self {
        PostHookOutput {
            hook_specific_output: Some(PostHookSpecificOutput {
                hook_event_name: "PostToolUse".to_string(),
                additional_context: Some(context),
            }),
        }
    }
}

/// Route a PostToolUse input to the appropriate handler.
/// Returns (PostFilterResult, CommandFamily) for metrics recording.
pub fn handle_post_input(input: &PostHookInput, depth: u32) -> (PostFilterResult, CommandFamily) {
    match input.tool_name.as_str() {
        "Read" => {
            let file_path = input
                .tool_input
                .get("file_path")
                .or_else(|| input.tool_input.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let content = input
                .tool_response
                .get("file")
                .and_then(|f| f.get("content"))
                .and_then(|c| c.as_str())
                // Pi format: { "output": "..." }
                .or_else(|| input.tool_response.get("output").and_then(|v| v.as_str()))
                // Flat format: { "content": "..." }
                .or_else(|| input.tool_response.get("content").and_then(|v| v.as_str()))
                .unwrap_or("");
            let result = handle_read(
                file_path,
                content,
                depth,
                input.cwd.as_deref().map(std::path::Path::new),
            );
            (result, CommandFamily::NativeRead)
        }
        "Grep" => {
            let grep_output = input
                .tool_response
                .get("output")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let result = handle_grep(grep_output, depth);
            (result, CommandFamily::Grep)
        }
        "Glob" => {
            let filenames = format_glob_output(&input.tool_response);
            let result = handle_glob(&filenames);
            (result, CommandFamily::Fs)
        }
        _ => (PostFilterResult::Passthrough, CommandFamily::Generic),
    }
}

/// Extract filenames from a Glob tool_response (handles both array and newline-separated string).
fn format_glob_output(tool_response: &serde_json::Value) -> String {
    // Try array form: { "filenames": ["a", "b", ...] }
    if let Some(arr) = tool_response.get("filenames").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join("\n");
    }
    // Try string form: { "filenames": "a\nb\nc" }
    if let Some(s) = tool_response.get("filenames").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    // Pi format: { "output": "a\nb\nc" }
    if let Some(s) = tool_response.get("output").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    String::new()
}

pub fn metrics_command(input: &PostHookInput) -> String {
    if input.tool_name == "Read" {
        if let Some(file_path) = input
            .tool_input
            .get("file_path")
            .or_else(|| input.tool_input.get("path"))
            .and_then(|v| v.as_str())
        {
            let trimmed = file_path.trim();
            if !trimmed.is_empty() {
                return format!("Read {trimmed}");
            }
        }
    }

    input.tool_name.clone()
}

/// Shared stdin reader — returns None if oversized or unparseable.
fn read_post_input() -> Option<PostHookInput> {
    use super::MAX_STDIN_BYTES;
    let mut buf = String::new();
    if io::stdin()
        .take(MAX_STDIN_BYTES as u64 + 1)
        .read_to_string(&mut buf)
        .is_err()
    {
        return None;
    }
    if buf.len() > MAX_STDIN_BYTES {
        return None;
    }
    serde_json::from_str(&buf).ok()
}

/// Record metrics for a post-hook interception.
fn record_post_metrics(
    input: &PostHookInput,
    family: CommandFamily,
    tokens_before: u32,
    tokens_after: u32,
    content_before: &str,
    final_output: &str,
) {
    let mode = if tokens_after < tokens_before {
        FilterMode::Filtered
    } else {
        FilterMode::Passthrough
    };
    let interception = Interception::new(
        metrics_command(input),
        family,
        input.cwd.clone(),
        tokens_before,
        tokens_after,
        mode,
        false,
        0,
        Some(content_before.to_string()),
        Some(final_output.to_string()),
    )
    .with_hook_type(HookType::PostToolUse);
    if let Some(path) = crate::metrics::store::metrics_path() {
        let _ = crate::metrics::store::append_to(&path, &interception);
    }
}

fn process_filter_result(
    result: PostFilterResult,
    input: &PostHookInput,
    family: CommandFamily,
    settings: &Settings,
) -> Option<String> {
    match result {
        PostFilterResult::Filtered {
            output,
            tokens_before,
            tokens_after,
            content_before,
        } => {
            let (final_output, final_tokens_after) = if settings.abbreviations_enabled {
                let abbreviated = crate::abbreviations::abbreviate(&output, settings).0;
                let recomputed = crate::tokens::count_tokens(&abbreviated) as u32;
                if recomputed < tokens_after {
                    (abbreviated, recomputed)
                } else {
                    (output, tokens_after)
                }
            } else {
                (output, tokens_after)
            };
            record_post_metrics(
                input,
                family,
                tokens_before,
                final_tokens_after,
                &content_before,
                &final_output,
            );
            Some(final_output)
        }
        PostFilterResult::Passthrough => None,
    }
}

/// AfterTool handler for Gemini CLI — replaces tool results with compressed output.
/// Gemini uses `decision: "deny"` + `reason` to substitute the tool result.
pub fn handle_post_gemini() {
    let settings = Settings::load();
    let depth = settings.post_hook_depth;

    let input = match read_post_input() {
        Some(mut i) => {
            i.tool_name = normalize_gemini_tool_name(&i.tool_name).to_string();
            i
        }
        None => {
            if let Ok(s) = serde_json::to_string(&GeminiAfterToolOutput::allow()) {
                print!("{s}");
            }
            return;
        }
    };

    let (result, family) = handle_post_input(&input, depth);
    let output =
        if let Some(final_output) = process_filter_result(result, &input, family, &settings) {
            GeminiAfterToolOutput::deny(final_output)
        } else {
            GeminiAfterToolOutput::allow()
        };

    match serde_json::to_string(&output) {
        Ok(json) => print!("{json}"),
        Err(_) => {
            if let Ok(s) = serde_json::to_string(&GeminiAfterToolOutput::allow()) {
                print!("{s}");
            }
        }
    }
}

/// PostToolUse handler for Qwen Code — injects compressed output as additionalContext.
/// Qwen's PostToolUse uses the same additionalContext mechanism as Claude Code.
pub fn handle_post_qwen() {
    let settings = Settings::load();
    let depth = settings.post_hook_depth;

    let input = match read_post_input() {
        Some(mut i) => {
            i.tool_name = normalize_qwen_tool_name(&i.tool_name).to_string();
            i
        }
        None => {
            print!("{{}}");
            return;
        }
    };

    let (result, family) = handle_post_input(&input, depth);
    let output =
        if let Some(final_output) = process_filter_result(result, &input, family, &settings) {
            PostHookOutput::with_context(final_output)
        } else {
            PostHookOutput::passthrough()
        };

    match serde_json::to_string(&output) {
        Ok(json) => print!("{json}"),
        Err(_) => print!("{{}}"),
    }
}

pub fn handle_post() {
    let settings = Settings::load();
    let depth = settings.post_hook_depth;

    let input = match read_post_input() {
        Some(i) => i,
        None => {
            print!("{{}}");
            return;
        }
    };

    let (result, family) = handle_post_input(&input, depth);
    let output =
        if let Some(final_output) = process_filter_result(result, &input, family, &settings) {
            PostHookOutput::with_context(final_output)
        } else {
            PostHookOutput::passthrough()
        };

    match serde_json::to_string(&output) {
        Ok(json) => print!("{json}"),
        Err(_) => print!("{{}}"),
    }
}
