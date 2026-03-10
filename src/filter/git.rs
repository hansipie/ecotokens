use crate::filter::generic::filter_generic;

const GIT_LINE_THRESHOLD: u32 = 200;
const GIT_BYTE_THRESHOLD: u32 = 30720; // 30 KB
const DIFF_MAX_LINES: usize = 500;

/// Filter git command output.
pub fn filter_git(command: &str, output: &str) -> String {
    let cmd = command.trim().to_lowercase();

    if cmd.starts_with("git diff") || cmd.starts_with("git show") {
        compact_diff(output)
    } else if cmd.starts_with("git status") {
        filter_git_status(output)
    } else if cmd.starts_with("git log") {
        filter_git_log(output)
    } else {
        filter_generic(output, 500, 51200)
    }
}

/// Remove context lines from diff output, keeping only +/- lines and @@ headers.
/// Also enforces a 500-line limit on the result.
fn compact_diff(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    // If already short, pass through
    if lines.len() <= GIT_LINE_THRESHOLD as usize {
        return output.to_string();
    }

    // Keep: lines starting with +, -, @@, ---, +++, diff
    let filtered: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| {
            l.starts_with('+')
                || l.starts_with('-')
                || l.starts_with("@@")
                || l.starts_with("diff ")
                || l.starts_with("index ")
                || l.starts_with("new file")
                || l.starts_with("deleted file")
                || l.starts_with("Binary")
        })
        .collect();

    let total_filtered = filtered.len();

    // Enforce 500-line max on the compact output
    if total_filtered <= DIFF_MAX_LINES {
        if filtered.len() < lines.len() {
            return format!(
                "{}\n[ecotokens] ... {} context lines removed ({} total diff lines) ...",
                filtered.join("\n"),
                lines.len().saturating_sub(total_filtered),
                lines.len(),
            );
        }
        return output.to_string();
    }

    // Compact output is still too long — truncate to DIFF_MAX_LINES
    let head: Vec<&str> = filtered.iter().take(DIFF_MAX_LINES / 2).copied().collect();
    let tail: Vec<&str> = filtered
        .iter()
        .rev()
        .take(DIFF_MAX_LINES / 2)
        .rev()
        .copied()
        .collect();
    let omitted = total_filtered.saturating_sub(DIFF_MAX_LINES);
    format!(
        "{}\n[ecotokens] ... {} diff lines omitted ({} total) ...\n{}",
        head.join("\n"),
        omitted,
        total_filtered,
        tail.join("\n"),
    )
}

fn filter_git_log(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= GIT_LINE_THRESHOLD as usize {
        return output.to_string();
    }

    // Keep: "commit HASH", "Author:", "Date:", and the first non-empty message line per commit.
    // Skip: file stat lines, diff content, decoration lines
    let mut result = Vec::new();
    let mut after_commit_header = false;
    let mut message_lines = 0usize;

    for line in &lines {
        if line.starts_with("commit ") {
            after_commit_header = true;
            message_lines = 0;
            result.push(*line);
        } else if line.starts_with("Author:")
            || line.starts_with("Date:")
            || line.starts_with("Merge:")
        {
            result.push(*line);
        } else if after_commit_header && message_lines < 2 {
            // Keep short commit message (up to 2 lines)
            if !line.trim().is_empty() || message_lines > 0 {
                result.push(*line);
                message_lines += 1;
            }
        }
        // Skip everything else (file stats, diff stats, etc.)
    }

    if result.len() < lines.len() {
        result.join("\n")
    } else {
        filter_generic(output, GIT_LINE_THRESHOLD, GIT_BYTE_THRESHOLD)
    }
}

fn filter_git_status(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    // Short status: pass through
    if lines.len() <= 30 {
        return output.to_string();
    }

    // Compact format: M/A/D prefix instead of verbose "modified:", "new file:", "deleted:"
    let mut compact = Vec::new();
    let mut header_done = false;

    for line in &lines {
        if line.trim_start().starts_with("modified:") {
            let file = line.trim_start().trim_start_matches("modified:").trim();
            compact.push(format!("M {}", file));
        } else if line.trim_start().starts_with("new file:") {
            let file = line.trim_start().trim_start_matches("new file:").trim();
            compact.push(format!("A {}", file));
        } else if line.trim_start().starts_with("deleted:") {
            let file = line.trim_start().trim_start_matches("deleted:").trim();
            compact.push(format!("D {}", file));
        } else if line.trim_start().starts_with("renamed:") {
            let rest = line.trim_start().trim_start_matches("renamed:").trim();
            compact.push(format!("R {}", rest));
        } else if !header_done {
            // Keep branch/upstream info lines at the top
            compact.push((*line).to_string());
            if line.is_empty() {
                header_done = true;
            }
        }
    }

    let changed_count = compact
        .iter()
        .filter(|l| {
            l.starts_with("M ") || l.starts_with("A ") || l.starts_with("D ") || l.starts_with("R ")
        })
        .count();

    if compact.len() < lines.len() {
        let result = compact.join("\n");
        if changed_count > 20 {
            // Many changes — add a summary
            let shown: Vec<&str> = compact.iter().take(20).map(|s| s.as_str()).collect();
            format!(
                "{}\n[ecotokens] ... {} total changes ({} shown) ...",
                shown.join("\n"),
                changed_count,
                20,
            )
        } else {
            result
        }
    } else {
        // Compact didn't help, use old approach
        let shown: Vec<&str> = lines.iter().take(15).copied().collect();
        let changed: Vec<&str> = lines
            .iter()
            .filter(|l| {
                l.trim_start().starts_with("modified:")
                    || l.trim_start().starts_with("new file:")
                    || l.trim_start().starts_with("deleted:")
            })
            .copied()
            .collect();
        let omitted = changed.len().saturating_sub(10);
        format!(
            "{}\n[ecotokens] ... {} more changed files omitted ({} total changes) ...",
            shown.join("\n"),
            omitted,
            changed.len(),
        )
    }
}
