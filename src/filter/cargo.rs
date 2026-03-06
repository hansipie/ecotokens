use crate::filter::generic::filter_generic;
use lazy_regex::regex;

const WARNING_THRESHOLD: usize = 10;

/// Filter cargo command output.
pub fn filter_cargo(command: &str, output: &str) -> String {
    let cmd = command.trim().to_lowercase();

    if cmd.starts_with("cargo build") || cmd.starts_with("cargo check") || cmd.starts_with("cargo clippy") {
        filter_cargo_build(output)
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

    // Collect errors (always keep)
    let errors: Vec<&str> = lines.iter().filter(|l| error_re.is_match(l) || l.contains("error[")).copied().collect();
    // Count warnings
    let warning_count = lines.iter().filter(|l| warning_re.is_match(l)).count();
    // Keep Finished/Compiling lines
    let stats: Vec<&str> = lines.iter().filter(|l| finish_re.is_match(l)).copied().collect();

    if errors.is_empty() && warning_count <= WARNING_THRESHOLD {
        return output.to_string();
    }

    let mut result = Vec::new();
    if !errors.is_empty() {
        result.extend(errors);
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

fn filter_cargo_test(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();

    // Always keep: FAILED lines, test result summary, failures section
    let important: Vec<&str> = lines
        .iter()
        .filter(|l| {
            l.contains("FAILED") || l.contains("test result:") || l.contains("failures:") || l.contains("error")
        })
        .copied()
        .collect();

    if lines.len() <= 30 || important.is_empty() {
        return output.to_string();
    }

    // Show failures + result line
    important.join("\n")
}
