use crate::filter::generic::filter_generic;
use std::collections::HashMap;

const MAX_MATCHES_PER_FILE: usize = 10;
const MAX_LINE_LEN: usize = 120;
const PASSTHROUGH_THRESHOLD: usize = 30;

/// Filter grep/rg output by grouping matches per file.
pub fn filter_grep(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= PASSTHROUGH_THRESHOLD {
        return output.to_string();
    }

    let mut file_matches: HashMap<String, Vec<(Option<usize>, String)>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for line in &lines {
        if let Some((file, entry)) = parse_grep_line(line) {
            if !file_matches.contains_key(&file) {
                order.push(file.clone());
            }
            file_matches.entry(file).or_default().push(entry);
        }
    }

    if file_matches.is_empty() {
        return filter_generic(output, 100, 51200);
    }

    let total_matches: usize = file_matches.values().map(|v| v.len()).sum();
    let file_count = file_matches.len();

    let mut result = vec![format!("🔍 {} matches in {} files:", total_matches, file_count)];

    for file in &order {
        let matches = &file_matches[file];
        let shown = matches.len().min(MAX_MATCHES_PER_FILE);
        let extra = matches.len().saturating_sub(MAX_MATCHES_PER_FILE);
        result.push(format!("{}:", file));
        for (lineno, content) in &matches[..shown] {
            let content = truncate_line(content);
            if let Some(n) = lineno {
                result.push(format!("  {}:{}", n, content));
            } else {
                result.push(format!("  {}", content));
            }
        }
        if extra > 0 {
            result.push(format!("  ... +{} more", extra));
        }
    }

    result.join("\n")
}

fn parse_grep_line(line: &str) -> Option<(String, (Option<usize>, String))> {
    // Try "file:line:content" first
    let parts: Vec<&str> = line.splitn(3, ':').collect();
    if parts.len() >= 3 {
        if let Ok(lineno) = parts[1].trim().parse::<usize>() {
            return Some((parts[0].to_string(), (Some(lineno), parts[2].to_string())));
        }
    }
    // Try "file:content" — file must look like a path
    if parts.len() >= 2 {
        let file = parts[0];
        if file.contains('/') || file.contains('.') {
            return Some((file.to_string(), (None, parts[1..].join(":"))));
        }
    }
    None
}

fn truncate_line(s: &str) -> String {
    if s.len() <= MAX_LINE_LEN {
        s.to_string()
    } else {
        let mut i = MAX_LINE_LEN;
        while i > 0 && !s.is_char_boundary(i) {
            i -= 1;
        }
        format!("{}…", &s[..i])
    }
}
