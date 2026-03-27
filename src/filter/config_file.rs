use lazy_regex::regex;

const CONFIG_LINE_THRESHOLD: usize = 100;
const MAX_ROOT_KEYS: usize = 30;

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

fn format_summary(format_name: &str, key_noun: &str, total_lines: usize, keys: &[&str]) -> String {
    format!(
        "[ecotokens] {format_name} summary ({total_lines} lines, {} {key_noun}):\n{}",
        keys.len(),
        keys.join(", ")
    )
}

fn filter_toml(content: &str, total_lines: usize) -> String {
    let table_re = regex!(r"^\[([^\]]+)\]");
    let tables = extract_root_keys(content, table_re, MAX_ROOT_KEYS);
    format_summary("TOML", "top-level tables", total_lines, &tables)
}

fn filter_json(content: &str, total_lines: usize) -> String {
    let key_re = regex!(r#"^\s{0,2}"([^"]+)"\s*:"#);
    let keys = extract_root_keys(content, key_re, MAX_ROOT_KEYS);
    format_summary("JSON", "root keys shown", total_lines, &keys)
}

fn filter_yaml(content: &str, total_lines: usize) -> String {
    let key_re = regex!(r"^([a-zA-Z_][a-zA-Z0-9_-]*):");
    let keys = extract_root_keys(content, key_re, MAX_ROOT_KEYS);
    format_summary("YAML", "root keys shown", total_lines, &keys)
}
