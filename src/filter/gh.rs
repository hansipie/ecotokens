use crate::filter::generic::filter_generic;
use lazy_regex::regex;

/// Filter GitHub CLI (gh) command output.
pub fn filter_gh(command: &str, output: &str) -> String {
    let cmd = command.trim().to_lowercase();

    if cmd.starts_with("gh pr view") {
        filter_gh_pr_view(output)
    } else if cmd.starts_with("gh pr list") {
        filter_gh_pr_list(output)
    } else if cmd.starts_with("gh issue view") {
        filter_gh_issue_view(output)
    } else if cmd.starts_with("gh issue list") {
        filter_gh_issue_list(output)
    } else if cmd.starts_with("gh run view") {
        filter_gh_run_view(output)
    } else {
        filter_generic(output, 500, 51200)
    }
}

fn strip_html_comments(text: &str) -> String {
    // Remove <!-- ... --> HTML comments (possibly multiline)
    let comment_re = regex!(r"(?s)<!--.*?-->");
    comment_re.replace_all(text, "").to_string()
}

fn filter_gh_pr_view(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 30 {
        // Still strip HTML comments even for short output
        return strip_html_comments(output);
    }

    // Keep: number, title, state, author, body (up to 200 lines)
    let mut result = Vec::new();
    let mut in_body = false;
    let mut body_lines = 0usize;
    const MAX_BODY_LINES: usize = 200;

    for line in &lines {
        let lower = line.to_lowercase();
        if lower.starts_with("number:")
            || lower.starts_with("title:")
            || lower.starts_with("state:")
            || lower.starts_with("author:")
            || lower.starts_with("url:")
        {
            result.push(*line);
            in_body = false;
        } else if lower.starts_with("body:") || lower.starts_with("--") {
            in_body = true;
            result.push(*line);
            body_lines = 0;
        } else if in_body && body_lines < MAX_BODY_LINES {
            result.push(*line);
            body_lines += 1;
        } else if in_body && body_lines >= MAX_BODY_LINES {
            result.push("[ecotokens] ... body truncated ...");
            in_body = false;
        }
    }

    let joined = result.join("\n");
    strip_html_comments(&joined)
}

/// Format tab-separated gh list output as compact `#N [state] Title` lines.
/// `min_parts` is the minimum number of tab-separated fields required to
/// attempt formatting (3 for PR, 2 for issue).
fn filter_gh_list(output: &str, min_parts: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let mut result = Vec::new();
    for line in &lines {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= min_parts {
            let number = parts[0].trim();
            let title = parts[1].trim();
            let state = if parts.len() > 3 { parts[3].trim() } else { "open" };
            result.push(format!("#{} [{}] {}", number, state, title));
        } else {
            result.push(line.to_string());
        }
    }
    if result.is_empty() {
        return output.to_string();
    }
    result.join("\n")
}

fn filter_gh_pr_list(output: &str) -> String {
    // gh pr list: tab-separated NUMBER  TITLE  BRANCH  CREATED_AT
    filter_gh_list(output, 3)
}

fn filter_gh_issue_view(output: &str) -> String {
    // Similar to PR view
    filter_gh_pr_view(output)
}

fn filter_gh_issue_list(output: &str) -> String {
    // gh issue list: tab-separated NUMBER  TITLE  LABELS  CREATED_AT
    filter_gh_list(output, 2)
}

fn filter_gh_run_view(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 20 {
        return output.to_string();
    }

    // Keep failures, conclusions, and summary info
    let mut result: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| {
            let lower = l.to_lowercase();
            lower.contains("fail")
                || lower.contains("error")
                || lower.contains("conclusion")
                || lower.contains("status:")
                || lower.contains("✓")
                || lower.contains("✗")
                || lower.contains("x ")
                || l.trim_start().starts_with("FAIL")
                || l.trim_start().starts_with("ERROR")
        })
        .collect();

    if result.is_empty() {
        return filter_generic(output, 50, 51200);
    }

    result.dedup();
    result.join("\n")
}
