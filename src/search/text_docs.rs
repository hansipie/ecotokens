use std::path::Path;

use super::symbols::Symbol;

/// Extract structured symbols from a documentation/config file.
/// Supported: Markdown (headings → h1/h2/h3), TOML (tables → table),
/// JSON (root keys → key).
/// `rel_path` is the path stored in each Symbol's `file_path` field.
pub fn index_text_doc(path: &Path, rel_path: &str) -> Result<Vec<Symbol>, std::io::Error> {
    let content = std::fs::read_to_string(path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let symbols = match ext {
        "md" | "markdown" => extract_markdown(&content, rel_path),
        "toml" => extract_toml(&content, rel_path),
        "json" => extract_json(&content, rel_path),
        "yaml" | "yml" => extract_yaml(&content, rel_path),
        _ => vec![],
    };
    Ok(symbols)
}

fn extract_markdown(content: &str, rel_path: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    for (line_idx, line) in content.lines().enumerate() {
        let (level, text) = if let Some(t) = line.strip_prefix("### ") {
            ("h3", t)
        } else if let Some(t) = line.strip_prefix("## ") {
            ("h2", t)
        } else if let Some(t) = line.strip_prefix("# ") {
            ("h1", t)
        } else {
            continue;
        };
        let name = text.trim().to_string();
        let id = format!("{rel_path}::{name}#{level}");
        symbols.push(Symbol {
            id,
            name,
            kind: level.to_string(),
            file_path: rel_path.to_string(),
            line_start: line_idx as u64,
            source: line.to_string(),
        });
    }
    symbols
}

fn extract_toml(content: &str, rel_path: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') && !trimmed.starts_with("[[") {
            let name = trimmed
                .trim_matches(|c| c == '[' || c == ']')
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let id = format!("{rel_path}::{name}#table");
            symbols.push(Symbol {
                id,
                name,
                kind: "table".to_string(),
                file_path: rel_path.to_string(),
                line_start: line_idx as u64,
                source: line.to_string(),
            });
        }
    }
    symbols
}

fn extract_json(content: &str, rel_path: &str) -> Vec<Symbol> {
    // Simple approach: extract top-level keys from a JSON object
    let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
        return vec![];
    };
    let Some(obj) = value.as_object() else {
        return vec![];
    };
    obj.keys()
        .enumerate()
        .map(|(i, key)| {
            let id = format!("{rel_path}::{key}#key");
            Symbol {
                id,
                name: key.clone(),
                kind: "key".to_string(),
                file_path: rel_path.to_string(),
                line_start: i as u64,
                source: key.clone(),
            }
        })
        .collect()
}

fn extract_yaml(content: &str, rel_path: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    for (line_idx, line) in content.lines().enumerate() {
        // Root-level keys: lines that start without indentation and contain ':'
        if !line.starts_with(' ') && !line.starts_with('\t') && !line.starts_with('#') {
            if let Some(key) = line.split(':').next() {
                let name = key.trim().to_string();
                if name.is_empty() || name.starts_with('-') {
                    continue;
                }
                let id = format!("{rel_path}::{name}#key");
                symbols.push(Symbol {
                    id,
                    name,
                    kind: "key".to_string(),
                    file_path: rel_path.to_string(),
                    line_start: line_idx as u64,
                    source: line.to_string(),
                });
            }
        }
    }
    symbols
}
