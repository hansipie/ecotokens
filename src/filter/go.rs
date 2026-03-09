use crate::filter::generic::filter_generic;
use lazy_regex::regex;
use std::collections::HashMap;

/// Filter Go command output.
pub fn filter_go(command: &str, output: &str) -> String {
    let cmd = command.trim().to_lowercase();

    if cmd.starts_with("go test") {
        filter_go_test(output)
    } else if cmd.starts_with("go build") {
        filter_go_build(output)
    } else if cmd.starts_with("go vet") {
        filter_go_vet(output)
    } else if cmd.contains("golangci-lint") {
        filter_golangci_lint(output)
    } else {
        filter_generic(output, 500, 51200)
    }
}

fn filter_go_test(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return output.to_string();
    }

    let mut failures = Vec::new();
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut in_failure = false;

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        if line.starts_with("--- FAIL:") {
            in_failure = true;
            failed += 1;
            failures.push(line);
        } else if line.starts_with("--- PASS:") || line.starts_with("=== RUN") {
            in_failure = false;
            if line.starts_with("--- PASS:") {
                passed += 1;
            }
        } else if line.starts_with("FAIL") || line.starts_with("ok ") {
            // Package-level result line
            if line.starts_with("FAIL") {
                failures.push(line);
            }
            in_failure = false;
        } else if in_failure {
            // Keep error context lines within a failure
            failures.push(line);
        }

        i += 1;
    }

    let has_any_results = passed > 0 || failed > 0;
    if failures.is_empty() && !has_any_results {
        return output.to_string();
    }

    // Short output: pass through
    if lines.len() <= 30 && failures.is_empty() {
        return output.to_string();
    }

    let mut result = failures.join("\n");
    let summary = format!("\n✓ {} passed | ✗ {} failed", passed, failed);
    result.push_str(&summary);
    result
}

fn filter_go_build(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 20 {
        return output.to_string();
    }

    // Keep only error lines (lines with ": " that look like compiler errors)
    let errors: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| {
            // Go build errors: "file.go:line:col: message" or lines starting with "#"
            l.starts_with('#')
                || (l.contains(".go:") && l.contains(": "))
                || l.to_lowercase().starts_with("error")
        })
        .collect();

    if errors.is_empty() {
        return output.to_string();
    }

    errors.join("\n")
}

fn filter_go_vet(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 20 {
        return output.to_string();
    }

    // go vet outputs "file.go:line:col: message" format
    let issues: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| l.contains(".go:") || l.starts_with('#'))
        .collect();

    if issues.is_empty() {
        return output.to_string();
    }

    issues.join("\n")
}

/// Filter golangci-lint output: group issues by linter.
pub fn filter_golangci_lint(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 30 {
        return output.to_string();
    }

    // Parse "file:line:col: message (linter)"
    let lint_re = regex!(r"^.+:\d+:\d+:\s+(.+?)\s+\((\w+)\)$");

    let mut linter_issues: HashMap<String, HashMap<String, usize>> = HashMap::new();
    for line in &lines {
        if let Some(caps) = lint_re.captures(line) {
            let message = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let linter = caps.get(2).map_or("", |m| m.as_str()).to_string();
            *linter_issues
                .entry(linter)
                .or_default()
                .entry(message)
                .or_insert(0) += 1;
        }
    }

    if linter_issues.is_empty() {
        return output.to_string();
    }

    let mut result = Vec::new();
    let mut linters: Vec<String> = linter_issues.keys().cloned().collect();
    linters.sort();

    for linter in &linters {
        let messages = &linter_issues[linter];
        let total: usize = messages.values().sum();
        let mut msg_list: Vec<String> = messages
            .iter()
            .map(|(msg, cnt)| {
                if *cnt > 1 {
                    format!("\"{}\" ({}x)", msg, cnt)
                } else {
                    format!("\"{}\"", msg)
                }
            })
            .collect();
        msg_list.sort();
        result.push(format!(
            "[{}] {} issues: {}",
            linter,
            total,
            msg_list.join(", ")
        ));
    }

    result.join("\n")
}
