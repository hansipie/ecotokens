use crate::filter::generic::filter_generic;
use lazy_regex::regex;

/// Filter Python command output (pytest, pip, ruff, etc.).
pub fn filter_python(command: &str, output: &str) -> String {
    let cmd = command.trim().to_lowercase();

    if cmd.starts_with("pytest") || cmd.contains("pytest") {
        filter_pytest(output)
    } else if cmd.starts_with("pip ") {
        filter_pip(output)
    } else if cmd.starts_with("ruff ") {
        filter_ruff(output)
    } else {
        filter_generic(output, 500, 51200)
    }
}

fn filter_pytest(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();

    // Always keep: FAILED, ERROR, summary line (e.g. "=== 1 failed, 2 passed in 0.12s ===")
    // and the "FAILURES" section headers.
    let failure_re = regex!(r"^(FAILURES|ERRORS)($|\s)");
    let summary_re = regex!(r"^={3,}.+={3,}$");

    let mut important = Vec::new();
    let mut in_failure_section = false;

    for line in &lines {
        if failure_re.is_match(line) {
            in_failure_section = true;
            important.push(*line);
            continue;
        }

        if summary_re.is_match(line) {
            in_failure_section = false;
            important.push(*line);
            continue;
        }

        if line.starts_with("E   ") || line.contains("FAILED") || line.contains("ERROR") {
            important.push(*line);
        } else if in_failure_section {
            // Keep content of failure sections
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
        if line.starts_with("Successfully installed") || line.contains("ERROR:") || line.contains("WARNING:") {
            result.push(*line);
        }
    }

    if result.is_empty() {
        return filter_generic(output, 20, 51200);
    }

    result.join("\n")
}

fn filter_ruff(output: &str) -> String {
    // Ruff output is usually concise, but can be long if many errors.
    // We keep all error lines but maybe summarize if too many.
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 100 {
        return output.to_string();
    }

    filter_generic(output, 100, 51200)
}
