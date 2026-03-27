use lazy_regex::regex;

const CONFIG_LINE_THRESHOLD: usize = 100;

/// Extract root-level keys from content using a regex pattern.
fn extract_root_keys<'a>(content: &'a str, key_re: &regex::Regex, max: usize) -> Vec<&'a str> {
    content
        .lines()
        .filter_map(|l| {
            key_re
                .captures(l)
                .and_then(|c| c.get(1).map(|m| m.as_str()))
        })
        .take(max)
        .collect()
}

/// Filter structured config files (TOML, JSON, YAML) by showing only top-level keys/tables.
pub fn filter_config_file(content: &str, ext: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() <= CONFIG_LINE_THRESHOLD {
        return content.to_string();
    }

    match ext {
        "toml" => filter_toml(content, lines.len()),
        "json" => filter_json(content, lines.len()),
        "yaml" | "yml" => filter_yaml(content, lines.len()),
        _ => content.to_string(),
    }
}

fn filter_toml(content: &str, total_lines: usize) -> String {
    let table_re = regex!(r"^\[([^\]]+)\]");
    let tables = extract_root_keys(content, &table_re, 30);

    format!(
        "[ecotokens] TOML summary ({total_lines} lines, {} top-level tables):\n{}",
        tables.len(),
        tables.join(", ")
    )
}

fn filter_json(content: &str, total_lines: usize) -> String {
    let key_re = regex!(r#"^\s{0,2}"([^"]+)"\s*:"#);
    let keys = extract_root_keys(content, &key_re, 30);

    format!(
        "[ecotokens] JSON summary ({total_lines} lines, {} root keys shown):\n{}",
        keys.len(),
        keys.join(", ")
    )
}

fn filter_yaml(content: &str, total_lines: usize) -> String {
    let key_re = regex!(r"^([a-zA-Z_][a-zA-Z0-9_-]*):");
    let keys = extract_root_keys(content, &key_re, 30);

    format!(
        "[ecotokens] YAML summary ({total_lines} lines, {} root keys shown):\n{}",
        keys.len(),
        keys.join(", ")
    )
}
