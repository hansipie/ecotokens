use std::io::{self, Read};

use crate::config::settings::Settings;
use crate::metrics::store::{CommandFamily, FilterMode, HookType, Interception};
use serde::{Deserialize, Serialize};

use super::glob_handler::handle_glob;
use super::grep_handler::handle_grep;
use super::read_handler::handle_read;

#[derive(Debug, Clone, Deserialize)]
pub struct PostHookInput {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_response: serde_json::Value,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub session_id: Option<String>,
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
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let content = input
                .tool_response
                .get("file")
                .and_then(|f| f.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("");
            let result = handle_read(file_path, content, depth);
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
            let filenames = extract_glob_filenames(&input.tool_response);
            let result = handle_glob(&filenames);
            (result, CommandFamily::Fs)
        }
        _ => (PostFilterResult::Passthrough, CommandFamily::Generic),
    }
}

/// Extract filenames from a Glob tool_response (handles both array and newline-separated string).
fn extract_glob_filenames(tool_response: &serde_json::Value) -> String {
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
    String::new()
}

pub fn handle_post() {
    let settings = Settings::load();
    let depth = settings.post_hook_depth;

    let mut stdin_buf = String::new();
    if io::stdin().read_to_string(&mut stdin_buf).is_err() {
        print!("{{}}");
        return;
    }

    let input: PostHookInput = match serde_json::from_str(&stdin_buf) {
        Ok(i) => i,
        Err(_) => {
            print!("{{}}");
            return;
        }
    };

    let (result, family) = handle_post_input(&input, depth);

    let output = match &result {
        PostFilterResult::Filtered {
            output,
            tokens_before,
            tokens_after,
        } => {
            // Record metrics
            let mode = if tokens_after < tokens_before {
                FilterMode::Filtered
            } else {
                FilterMode::Passthrough
            };
            let interception = Interception::new(
                input.tool_name.clone(),
                family,
                input.cwd.clone(),
                *tokens_before,
                *tokens_after,
                mode,
                false,
                0,
                None,
                None,
            )
            .with_hook_type(HookType::PostToolUse);

            if let Some(path) = crate::metrics::store::metrics_path() {
                let _ = crate::metrics::store::append_to(&path, &interception);
            }

            PostHookOutput::with_context(output.clone())
        }
        PostFilterResult::Passthrough => PostHookOutput::passthrough(),
    };

    match serde_json::to_string(&output) {
        Ok(json) => print!("{json}"),
        Err(_) => print!("{{}}"),
    }
}
