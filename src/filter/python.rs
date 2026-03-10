use crate::filter::generic::filter_generic;
use lazy_regex::regex;
use std::collections::HashMap;

/// Filter Python command output (pytest, pip, ruff, mypy, etc.).
pub fn filter_python(command: &str, output: &str) -> String {
    let cmd = command.trim().to_lowercase();

    if cmd.starts_with("pytest") || cmd.contains("pytest") {
        filter_pytest(output)
    } else if cmd.starts_with("pip ") || cmd.starts_with("uv pip ") {
        filter_pip(output)
    } else if cmd.starts_with("ruff ") {
        filter_ruff(output)
    } else if cmd.starts_with("mypy") || cmd.contains("mypy") {
        filter_mypy(output)
    } else {
        filter_generic(output, 500, 51200)
    }
}

fn filter_pytest(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();

    // pytest section headers look like: "=== FAILURES ===" or "=== short test summary info ==="
    // We detect them and track whether we're inside a failure/error section.
    let section_re = regex!(r"^={3,}\s+(.+?)\s+={3,}$");

    let mut important = Vec::new();
    let mut in_failure_section = false;

    for line in &lines {
        if let Some(caps) = section_re.captures(line) {
            let title = caps.get(1).map_or("", |m| m.as_str()).to_uppercase();
            in_failure_section = title.contains("FAILURES") || title.contains("ERRORS");
            important.push(*line);
            continue;
        }

        if line.starts_with("E   ") || line.contains("FAILED") || line.contains("ERROR") {
            important.push(*line);
        } else if in_failure_section {
            // Keep all content inside failure/error sections (tracebacks, assertions, etc.)
            important.push(*line);
        }
    }

    if lines.len() <= 50 || important.is_empty() {
        return output.to_string();
    }

    important.join("\n")
}

fn filter_pip(output: &str) -> String {
    // For pip, we often care about the end (Successfully installed...) or errors
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 20 {
        return output.to_string();
    }

    let mut result = Vec::new();
    for line in &lines {
        if line.starts_with("Successfully installed")
            || line.contains("ERROR:")
            || line.contains("WARNING:")
        {
            result.push(*line);
        }
    }

    if result.is_empty() {
        return filter_generic(output, 20, 51200);
    }

    result.join("\n")
}

fn filter_ruff(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 100 {
        return output.to_string();
    }

    // Keep structured diagnostic lines (file:line:col: CODE msg) and summary lines.
    // This avoids head+tail which loses errors in the middle.
    let diag_re = regex!(r"^\S.*:\d+:\d+: [A-Z]\d+ ");
    let result: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| diag_re.is_match(l) || l.starts_with("Found ") || l.starts_with("error:"))
        .collect();

    if result.is_empty() {
        return filter_generic(output, 100, 51200);
    }

    result.join("\n")
}

/// Filter mypy output: group diagnostics by file.
/// Pattern: `file:line: error/warning/note: message [code]`
pub fn filter_mypy(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 30 {
        return output.to_string();
    }

    // Pattern: file.py:line: severity: message [code]
    let diag_re = regex!(r"^([^:]+\.py):(\d+): (error|warning|note): (.+?)(?:\s+\[([^\]]+)\])?$");

    // Group by file
    let mut file_diags: HashMap<String, Vec<String>> = HashMap::new();
    let mut file_order: Vec<String> = Vec::new();
    let mut summary_lines: Vec<&str> = Vec::new();

    for line in &lines {
        if let Some(caps) = diag_re.captures(line) {
            let file = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let lineno = caps.get(2).map_or("", |m| m.as_str());
            let severity = caps.get(3).map_or("", |m| m.as_str());
            let message = caps.get(4).map_or("", |m| m.as_str());
            let code = caps.get(5).map_or("", |m| m.as_str());

            let formatted = if code.is_empty() {
                format!("[{}:{}] {}: {}", file, lineno, severity, message)
            } else {
                format!("[{}:{}] {}: {} [{}]", file, lineno, severity, message, code)
            };

            if !file_diags.contains_key(&file) {
                file_order.push(file.clone());
            }
            file_diags.entry(file).or_default().push(formatted);
        } else if line.contains("Found ") || line.contains("Success:") || line.contains("error:") {
            // Summary line
            summary_lines.push(line);
        }
    }

    if file_diags.is_empty() {
        return output.to_string();
    }

    let mut result = Vec::new();
    for file in &file_order {
        let diags = &file_diags[file];
        result.push(format!("--- {} ({} issues) ---", file, diags.len()));
        for d in diags {
            result.push(d.clone());
        }
    }

    result.extend(summary_lines.iter().map(|s| s.to_string()));
    result.join("\n")
}
