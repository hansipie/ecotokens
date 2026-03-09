use crate::filter::generic::filter_generic;
use lazy_regex::regex;
use std::collections::HashMap;

const WARNING_THRESHOLD: usize = 10;

/// Filter cargo command output.
pub fn filter_cargo(command: &str, output: &str) -> String {
    let cmd = command.trim().to_lowercase();

    if cmd.starts_with("cargo build") || cmd.starts_with("cargo check") {
        filter_cargo_build(output)
    } else if cmd.starts_with("cargo clippy") {
        filter_cargo_clippy(output)
    } else if cmd.starts_with("cargo test") {
        filter_cargo_test(output)
    } else {
        filter_generic(output, 500, 51200)
    }
}

fn filter_cargo_build(output: &str) -> String {
    let error_re = regex!(r"^error");
    let warning_re = regex!(r"^warning");
    let finish_re = regex!(r"^\s*(Finished|Compiling|Checking|Running|error\[)");

    let lines: Vec<&str> = output.lines().collect();

    // Collect errors (always keep) with their context
    let mut errors: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if error_re.is_match(line) || line.contains("error[") {
            errors.push(line);
            // Also keep the "  --> file:line" context line
            if i + 1 < lines.len() && (lines[i + 1].contains("-->") || lines[i + 1].starts_with("  ")) {
                i += 1;
                errors.push(lines[i]);
            }
        }
        i += 1;
    }

    // Count warnings
    let warning_count = lines.iter().filter(|l| warning_re.is_match(l)).count();
    // Keep Finished/Compiling lines
    let stats: Vec<&str> = lines.iter().filter(|l| finish_re.is_match(l)).copied().collect();

    if errors.is_empty() && warning_count <= WARNING_THRESHOLD {
        return output.to_string();
    }

    let mut result = Vec::new();
    if !errors.is_empty() {
        result.extend_from_slice(&errors);
    }
    let warn_summary;
    if warning_count > WARNING_THRESHOLD {
        warn_summary = format!("[ecotokens] {} warnings total (summarized)", warning_count);
        result.push(&warn_summary);
    }
    result.extend(stats.iter().copied());

    // If we summarized but result is still long, apply generic
    let joined = result.join("\n");
    if joined.len() < output.len() {
        joined
    } else {
        filter_generic(output, 200, 51200)
    }
}

fn filter_cargo_clippy(output: &str) -> String {
    let warning_re = regex!(r"^warning(\[.*?\])?:");
    let error_re = regex!(r"^error(\[.*?\])?:");
    let location_re = regex!(r"^\s+-->");

    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return output.to_string();
    }

    // Group warnings by file
    // State machine: collect diagnostic blocks, extract file from location line
    let mut file_warnings: HashMap<String, Vec<String>> = HashMap::new();
    let mut errors: Vec<&str> = Vec::new();
    let mut finish_lines: Vec<&str> = Vec::new();
    let finish_re = regex!(r"^\s*(Finished|error\[)");

    let mut current_diag: Vec<&str> = Vec::new();
    let mut current_is_warning = false;
    let mut current_file = String::from("unknown");

    for line in &lines {
        if warning_re.is_match(line) || error_re.is_match(line) {
            // Flush previous diagnostic
            if !current_diag.is_empty() {
                if current_is_warning {
                    file_warnings
                        .entry(current_file.clone())
                        .or_default()
                        .push(current_diag.join("\n"));
                } else {
                    errors.extend(current_diag.iter().copied());
                }
                current_diag.clear();
            }
            current_is_warning = warning_re.is_match(line);
            current_diag.push(line);
            current_file = String::from("unknown");
        } else if location_re.is_match(line) {
            // Extract file path from "  --> src/lib.rs:10:5"
            if let Some(path) = line.trim().strip_prefix("-->").map(|s| s.trim()) {
                if let Some(file) = path.split(':').next() {
                    current_file = file.to_string();
                }
            }
            current_diag.push(line);
        } else if finish_re.is_match(line) {
            // Flush last diagnostic
            if !current_diag.is_empty() {
                if current_is_warning {
                    file_warnings
                        .entry(current_file.clone())
                        .or_default()
                        .push(current_diag.join("\n"));
                } else {
                    errors.extend(current_diag.iter().copied());
                }
                current_diag.clear();
            }
            finish_lines.push(line);
        } else if !current_diag.is_empty() {
            current_diag.push(line);
        }
    }
    // Flush any remaining diagnostic
    if !current_diag.is_empty() {
        if current_is_warning {
            file_warnings
                .entry(current_file.clone())
                .or_default()
                .push(current_diag.join("\n"));
        } else {
            errors.extend(current_diag.iter().copied());
        }
    }

    let total_warnings: usize = file_warnings.values().map(|v| v.len()).sum();

    if errors.is_empty() && total_warnings <= WARNING_THRESHOLD {
        return output.to_string();
    }

    let mut result = Vec::new();

    // Include errors first
    if !errors.is_empty() {
        result.extend(errors);
    }

    // Summarize warnings grouped by file
    if total_warnings > 0 {
        let mut sorted_files: Vec<String> = file_warnings.keys().cloned().collect();
        sorted_files.sort();
        for file in sorted_files {
            let count = file_warnings[&file].len();
            result.push(Box::leak(
                format!("[ecotokens] {} warnings in {}", count, file).into_boxed_str(),
            ) as &str);
            // Show first warning per file as example
            if let Some(first) = file_warnings[&file].first() {
                for warn_line in first.lines().take(3) {
                    result.push(Box::leak(warn_line.to_string().into_boxed_str()) as &str);
                }
            }
        }
        result.push(Box::leak(
            format!("[ecotokens] {} total warnings", total_warnings).into_boxed_str(),
        ) as &str);
    }

    result.extend(finish_lines);
    result.join("\n")
}

fn filter_cargo_test(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();

    if lines.len() <= 30 {
        return output.to_string();
    }

    // Capture: FAILED lines, test result summary, failures section, stack traces, error lines
    // Strategy: detect "failures:" section and capture everything until next blank section
    let mut result: Vec<&str> = Vec::new();
    let mut in_failures_section = false;
    let mut has_failures = false;

    for line in &lines {
        if line.contains("FAILED") {
            result.push(*line);
            has_failures = true;
        } else if line.contains("test result:") {
            result.push(*line);
        } else if *line == "failures:" || line.starts_with("failures:") {
            in_failures_section = true;
            result.push(*line);
        } else if in_failures_section {
            // Keep everything in failures section (stack traces, panic messages, etc.)
            result.push(*line);
            // End of failures section: blank line followed by "test result:"
            if line.trim().is_empty() {
                // Peek ahead — we'll handle this by just continuing
            }
        } else if line.to_lowercase().contains("error") && !line.starts_with("   Compiling") {
            result.push(*line);
        }
    }

    if result.is_empty() || !has_failures {
        // No failures — return full output if short, else filter generic
        if lines.len() <= 100 {
            return output.to_string();
        }
        return filter_generic(output, 100, 51200);
    }

    result.join("\n")
}
