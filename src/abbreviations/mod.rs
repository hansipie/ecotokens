pub mod dictionary;

use crate::config::settings::Settings;
use regex::{Regex, RegexBuilder};
use std::sync::OnceLock;

struct CompiledRule {
    re: Regex,
    replacement: String,
}

fn compile_rules(pairs: &[(String, String)]) -> Vec<CompiledRule> {
    pairs
        .iter()
        .filter_map(|(word, abbrev)| {
            let pattern = format!(r"\b{}\b", regex::escape(word));
            RegexBuilder::new(&pattern)
                .case_insensitive(true)
                .build()
                .ok()
                .map(|re| CompiledRule {
                    re,
                    replacement: abbrev.clone(),
                })
        })
        .collect()
}

fn default_rules() -> &'static Vec<CompiledRule> {
    static RULES: OnceLock<Vec<CompiledRule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let pairs: Vec<(String, String)> = dictionary::DEFAULT_PAIRS
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        compile_rules(&pairs)
    })
}

fn apply_case(original: &str, replacement: &str) -> String {
    let mut chars = original.chars();
    let first = chars.next();
    let has_upper_tail = chars.clone().any(|c| c.is_uppercase());
    let all_upper = original.chars().filter(|c| c.is_alphabetic()).count() > 0
        && original
            .chars()
            .all(|c| !c.is_alphabetic() || c.is_uppercase());
    match first {
        Some(c) if all_upper && original.len() > 1 => replacement.to_uppercase(),
        Some(c) if c.is_uppercase() && !has_upper_tail => {
            let mut out = String::with_capacity(replacement.len());
            let mut rc = replacement.chars();
            if let Some(r0) = rc.next() {
                for u in r0.to_uppercase() {
                    out.push(u);
                }
            }
            out.extend(rc);
            out
        }
        _ => replacement.to_string(),
    }
}

/// Apply abbreviations to text. Blocks of code delimited by triple backticks
/// are preserved as-is. Returns `(transformed_text, number_of_replacements)`.
pub fn abbreviate(text: &str, settings: &Settings) -> (String, u32) {
    if !settings.abbreviations_enabled {
        return (text.to_string(), 0);
    }

    let custom_rules = if settings.abbreviations_custom.is_empty() {
        None
    } else {
        let pairs = dictionary::merged_pairs(&settings.abbreviations_custom);
        Some(compile_rules(&pairs))
    };
    let rules: &Vec<CompiledRule> = custom_rules.as_ref().unwrap_or_else(|| default_rules());

    let mut out = String::with_capacity(text.len());
    let mut count: u32 = 0;
    let mut in_code = false;
    for (idx, segment) in text.split("```").enumerate() {
        if idx > 0 {
            out.push_str("```");
            in_code = !in_code;
        }
        if in_code {
            out.push_str(segment);
        } else {
            let (transformed, c) = transform_segment(segment, rules);
            count += c;
            out.push_str(&transformed);
        }
    }
    (out, count)
}

fn transform_segment(segment: &str, rules: &[CompiledRule]) -> (String, u32) {
    let mut current = segment.to_string();
    let mut total: u32 = 0;
    for rule in rules {
        let mut changed = false;
        let replaced = rule
            .re
            .replace_all(&current, |caps: &regex::Captures| {
                changed = true;
                total += 1;
                apply_case(&caps[0], &rule.replacement)
            })
            .to_string();
        if changed {
            current = replaced;
        }
    }
    (current, total)
}

/// Build a textual instruction listing the active abbreviations, suitable for
/// injecting as `additionalContext` in a SessionStart hook so the model adopts
/// them in its own responses.
pub fn build_model_instructions(settings: &Settings) -> String {
    let pairs = dictionary::merged_pairs(&settings.abbreviations_custom);
    let mut body = String::from(
        "Token-saving directive: in your textual responses (not in code blocks, \
identifiers, or file paths), prefer these abbreviations over the full words. \
Preserve them when quoting tool output too.\n",
    );
    let mut display = pairs;
    display.sort_by(|a, b| a.0.cmp(&b.0));
    for (word, abbrev) in display {
        body.push_str(&format!("- {word} → {abbrev}\n"));
    }
    body
}
