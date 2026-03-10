use crate::filter::generic::filter_generic;

const PASSTHROUGH_THRESHOLD: usize = 15;

/// Filter curl/wget output.
pub fn filter_network(command: &str, output: &str) -> String {
    let cmd = command.trim().to_lowercase();
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= PASSTHROUGH_THRESHOLD {
        return output.to_string();
    }

    if cmd.starts_with("curl") {
        filter_curl(output)
    } else if cmd.starts_with("wget") {
        filter_wget(output)
    } else {
        filter_generic(output, 100, 51200)
    }
}

fn filter_curl(output: &str) -> String {
    // Remove progress lines
    let cleaned: String = output
        .lines()
        .filter(|l| !is_curl_progress(l))
        .collect::<Vec<_>>()
        .join("\n");

    let trimmed = cleaned.trim();
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return serde_json::to_string(&json).unwrap_or_else(|_| trimmed.to_string());
    }

    filter_generic(&cleaned, 100, 51200)
}

fn is_curl_progress(line: &str) -> bool {
    let t = line.trim();
    t.contains("% Total")
        || t.contains("Dload")
        || (t.contains('%') && t.split_whitespace().count() > 3)
}

fn filter_wget(output: &str) -> String {
    let meaningful: Vec<&str> = output
        .lines()
        .filter(|l| {
            let lower = l.to_lowercase();
            lower.contains("saved")
                || lower.contains("error")
                || lower.contains("downloaded")
                || lower.contains("failed")
        })
        .collect();

    if meaningful.is_empty() {
        return filter_generic(output, 100, 51200);
    }
    meaningful.join("\n")
}
