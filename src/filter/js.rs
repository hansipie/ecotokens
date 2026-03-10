use crate::filter::generic::filter_generic;
use lazy_regex::regex;
use std::collections::HashMap;

/// Filter JavaScript/TypeScript tooling output.
pub fn filter_js(command: &str, output: &str) -> String {
    let cmd = command.trim().to_lowercase();

    if cmd.starts_with("npm install")
        || cmd.starts_with("npm i ")
        || cmd.starts_with("pnpm install")
        || cmd.starts_with("pnpm i ")
    {
        filter_npm_install(output)
    } else if cmd.starts_with("tsc") {
        filter_tsc(output)
    } else if cmd.starts_with("vitest") || cmd.contains("vitest") {
        filter_vitest(output)
    } else if cmd.starts_with("eslint") {
        filter_eslint(output)
    } else if cmd.starts_with("prettier") {
        filter_prettier(output)
    } else if cmd.contains("playwright") {
        filter_playwright(output)
    } else if cmd.contains("prisma") {
        filter_prisma(output)
    } else if cmd.contains("next build") {
        filter_next(output)
    } else {
        filter_generic(output, 500, 51200)
    }
}

fn filter_npm_install(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 10 {
        return output.to_string();
    }

    // Keep only the last summary line and any error lines
    let mut result: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| {
            l.contains("packages added")
                || l.contains("packages removed")
                || l.contains("packages changed")
                || l.contains("packages updated")
                || l.to_lowercase().contains("error")
                || l.to_lowercase().contains("warn")
        })
        .collect();

    if result.is_empty() {
        // Fall back to last line as summary
        if let Some(last) = lines.last() {
            result.push(last);
        }
    }

    result.join("\n")
}

fn filter_tsc(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return output.to_string();
    }

    // Pattern: file(line,col): error|warning TSxxxx: message
    let error_re = regex!(r"^(.+?)\((\d+),(\d+)\):\s+(error|warning)\s+(TS\d+):\s+(.+)$");

    // Group errors by file
    let mut file_errors: HashMap<String, Vec<(String, u32)>> = HashMap::new();
    let mut total_errors = 0usize;

    for line in &lines {
        if let Some(caps) = error_re.captures(line) {
            let file = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let code = caps.get(5).map_or("", |m| m.as_str()).to_string();
            let entry = file_errors.entry(file).or_default();
            // Track code counts
            if let Some(pos) = entry.iter().position(|(c, _)| c == &code) {
                entry[pos].1 += 1;
            } else {
                entry.push((code, 1));
            }
            total_errors += 1;
        }
    }

    if file_errors.is_empty() {
        // No TSC-style errors; pass through if short
        if lines.len() <= 50 {
            return output.to_string();
        }
        return filter_generic(output, 50, 51200);
    }

    let mut result = Vec::new();
    let mut sorted_files: Vec<String> = file_errors.keys().cloned().collect();
    sorted_files.sort();

    for file in sorted_files {
        let codes = &file_errors[&file];
        let count: u32 = codes.iter().map(|(_, c)| c).sum();
        let codes_str: Vec<String> = codes
            .iter()
            .map(|(code, cnt)| {
                if *cnt > 1 {
                    format!("{} ({}x)", code, cnt)
                } else {
                    code.clone()
                }
            })
            .collect();
        result.push(format!(
            "[{}] {} errors: {}",
            file,
            count,
            codes_str.join(", ")
        ));
    }

    result.push(format!(
        "[ecotokens] {} total TypeScript errors",
        total_errors
    ));
    result.join("\n")
}

fn filter_vitest(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 30 {
        return output.to_string();
    }

    let mut failures = Vec::new();
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut in_failure = false;

    for line in &lines {
        // Vitest failure markers
        if line.contains(" × ") || line.contains(" ✗ ") || line.trim_start().starts_with("× ") {
            in_failure = true;
            failed += 1;
            failures.push(*line);
        } else if line.contains(" ✓ ") || line.trim_start().starts_with("✓ ") {
            in_failure = false;
            passed += 1;
        } else if line.contains(" ↓ ") || line.trim_start().starts_with("↓ ") {
            in_failure = false;
            skipped += 1;
        } else if line.trim_start().starts_with("FAIL ") {
            failed += 1;
            failures.push(*line);
            in_failure = false;
        } else if in_failure {
            // Keep context of failure
            failures.push(*line);
        } else if line.contains("Tests ") && (line.contains("failed") || line.contains("passed")) {
            // Summary line
            failures.push(*line);
        }
    }

    if failures.is_empty() {
        return output.to_string();
    }

    let mut result = failures.join("\n");
    result.push_str(&format!("\n✓ {} | ✗ {} | ⊘ {}", passed, failed, skipped));
    result
}

fn filter_eslint(output: &str) -> String {
    // Always try JSON parsing first (ESLint --format=json produces one long line)
    let trimmed = output.trim();
    if trimmed.starts_with('[') {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
            return format_eslint_json(&json);
        }
    }

    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 30 {
        return output.to_string();
    }

    // Text output: group by file
    let mut result: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| {
            // Keep file path lines (non-indented, non-empty) and error/warning lines
            !l.is_empty()
                && ((!l.starts_with(' ') && !l.starts_with('\t'))
                    || l.contains("error")
                    || l.contains("warning"))
        })
        .collect();

    if result.is_empty() {
        return filter_generic(output, 100, 51200);
    }

    // Deduplicate while preserving order
    result.dedup();
    result.join("\n")
}

fn format_eslint_json(json: &serde_json::Value) -> String {
    let mut result = Vec::new();
    if let Some(files) = json.as_array() {
        for file_entry in files {
            let file_path = file_entry
                .get("filePath")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let messages = file_entry
                .get("messages")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            if messages > 0 {
                result.push(format!("{}: {} issues", file_path, messages));
            }
        }
    }
    if result.is_empty() {
        return "No ESLint issues found".to_string();
    }
    result.join("\n")
}

fn filter_prettier(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 20 {
        return output.to_string();
    }

    // Prettier --check outputs files that need formatting
    let needs_format: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| {
            // Lines starting with a path or containing "needs formatting"
            l.contains("needs formatting")
                || (!l.starts_with(' ')
                    && !l.is_empty()
                    && !l.starts_with("Checking")
                    && !l.starts_with("All matched")
                    && !l.starts_with("Code style issues"))
        })
        .collect();

    if needs_format.is_empty() {
        return output.to_string();
    }

    needs_format.join("\n")
}

/// Filter Playwright test output: keep failures and stack traces.
pub fn filter_playwright(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let mut failures: Vec<&str> = Vec::new();
    let mut total = 0usize;
    let mut failed_count = 0usize;
    let mut in_failure = false;

    for line in &lines {
        if line.contains('✘')
            || line.contains('×')
            || line.contains("FAILED")
            || line.trim_start().starts_with("● ")
        {
            in_failure = true;
            failed_count += 1;
            failures.push(line);
        } else if line.trim_start().starts_with("✓") || line.trim_start().starts_with("✔") {
            in_failure = false;
            total += 1;
        } else if in_failure && (line.starts_with("    ") || line.starts_with('\t')) {
            // Stack trace lines
            failures.push(line);
        } else if line.contains(" passed") || line.contains(" failed") {
            // Summary line
            failures.push(line);
            in_failure = false;
        } else {
            in_failure = false;
        }
    }

    if failures.is_empty() {
        return output.to_string();
    }

    let mut result = failures.join("\n");
    result.push_str(&format!(
        "\n✗ {} of {} tests failed",
        failed_count,
        total + failed_count
    ));
    result
}

/// Filter Prisma output: remove box-drawing and marketing lines.
pub fn filter_prisma(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 10 {
        return output.to_string();
    }

    let filtered: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| {
            // Remove box-drawing characters
            if l.contains('┌') || l.contains('│') || l.contains('└') || l.contains('─') {
                return false;
            }
            // Remove marketing lines
            let lower = l.to_lowercase();
            if lower.contains("accelerate")
                || lower.contains("pulse")
                || lower.contains("speed up")
                || lower.contains("learn more")
            {
                return false;
            }
            // Keep meaningful lines
            let lower2 = l.to_lowercase();
            lower2.contains("migration")
                || lower2.contains("generated")
                || lower2.contains("error")
                || lower2.contains("warning")
                || lower2.contains("warn")
                || l.is_empty()
        })
        .collect();

    if filtered.is_empty() {
        return output.to_string();
    }

    filtered.join("\n")
}

/// Filter `next build` output: summarize route counts and build time.
pub fn filter_next(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 20 {
        return output.to_string();
    }

    let route_re = regex!(r"^[○●λ✓]");
    let build_time_re = regex!(r"(?:Compiled in|Build time:?)\s+([\d.]+\s*s)");
    let warning_re = regex!(r"(?i)warning|warn");

    let mut static_count = 0usize;
    let mut dynamic_count = 0usize;
    let mut build_time: Option<String> = None;
    let mut warnings = 0usize;

    for line in &lines {
        if route_re.is_match(line) {
            if line.contains('λ') || line.contains('●') {
                dynamic_count += 1;
            } else {
                static_count += 1;
            }
        }
        if build_time.is_none() {
            if let Some(caps) = build_time_re.captures(line) {
                build_time = caps.get(1).map(|m| m.as_str().to_string());
            }
        }
        if warning_re.is_match(line) {
            warnings += 1;
        }
    }

    let total_routes = static_count + dynamic_count;
    let mut result = vec![format!(
        "✓ Build: {} routes ({} static, {} dynamic)",
        total_routes, static_count, dynamic_count
    )];

    if let Some(t) = build_time {
        result.push(format!("⏱ Build time: {}", t));
    }
    if warnings > 0 {
        result.push(format!("⚠ {} warnings", warnings));
    }

    result.join("\n")
}
