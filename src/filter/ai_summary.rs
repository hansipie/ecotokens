#[cfg(feature = "ai-summary")]
use crate::config::settings::Settings;
#[cfg(feature = "ai-summary")]
use crate::filter::generic;
#[cfg(feature = "ai-summary")]
use std::time::Duration;

/// Minimum tokens to trigger AI summarization (avoid overhead for small outputs).
const MIN_TOKENS_FOR_AI: u32 = 2500;

/// Timeout for Ollama API call (fail fast to avoid blocking Claude).
const OLLAMA_TIMEOUT_MS: u64 = 3000;

#[cfg(feature = "ai-summary")]
/// Attempt AI-powered summarization via Ollama. Falls back to generic filter on error.
pub fn try_ai_summary(output: &str, settings: &Settings) -> Result<String, String> {
    // Guard: only summarize large outputs where AI is worth the cost
    let tokens = crate::tokens::estimate_tokens(output) as u32;
    if tokens < MIN_TOKENS_FOR_AI {
        return Err("Output too small for AI summary".into());
    }

    // Guard: check if AI summary is enabled in config
    if !settings.ai_summary_enabled {
        return Err("AI summary disabled in config".into());
    }

    // Extract Ollama URL from settings
    let ollama_url = match &settings.embed_provider {
        crate::config::settings::EmbedProvider::Ollama { url } => url.clone(),
        _ => return Err("Ollama not configured".into()),
    };

    // Prepare prompt
    let prompt = format!(
        "Summarize this command output in <500 tokens. Keep ALL errors, warnings, and critical info. Remove verbose logs, stack traces >10 lines, and boilerplate. Format: brief overview, then bullet list of key points.\n\n{}",
        output
    );

    // Call Ollama API
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(OLLAMA_TIMEOUT_MS))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let payload = serde_json::json!({
        "model": settings.ai_summary_model.as_deref().unwrap_or("llama3.2:3b"),
        "prompt": prompt,
        "stream": false,
        "options": {
            "temperature": 0.1,
            "num_predict": 500,
        }
    });

    let response = client
        .post(format!("{}/api/generate", ollama_url))
        .json(&payload)
        .send()
        .map_err(|e| format!("Ollama request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Ollama returned status {}", response.status()));
    }

    let json: serde_json::Value = response
        .json()
        .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

    let summary = json["response"]
        .as_str()
        .ok_or("Missing 'response' field in Ollama output")?
        .trim()
        .to_string();

    if summary.is_empty() {
        return Err("Ollama returned empty summary".into());
    }

    Ok(format!(
        "[ecotokens] AI summary ({} → ~{} tokens):\n{}",
        tokens,
        crate::tokens::estimate_tokens(&summary),
        summary
    ))
}

#[cfg(feature = "ai-summary")]
/// Main entry point: try AI summary, fallback to generic filter.
pub fn ai_summary_or_fallback(output: &str, settings: &Settings) -> String {
    match try_ai_summary(output, settings) {
        Ok(summary) => summary,
        Err(reason) => {
            if settings.debug {
                eprintln!("[ecotokens] AI summary skipped: {}", reason);
            }
            generic::filter_generic(output, 20, 8192)
        }
    }
}

#[cfg(not(feature = "ai-summary"))]
/// Stub when feature is disabled: applies generic head/tail filter.
pub fn ai_summary_or_fallback(output: &str, _settings: &Settings) -> String {
    crate::filter::generic::filter_generic(output, 20, 8192)
}

#[cfg(not(feature = "ai-summary"))]
use crate::config::settings::Settings;
