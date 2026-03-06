use crate::filter::generic::filter_generic;

const GIT_LINE_THRESHOLD: u32 = 200;
const GIT_BYTE_THRESHOLD: u32 = 30720; // 30 KB

/// Filter git command output.
pub fn filter_git(command: &str, output: &str) -> String {
    let cmd = command.trim().to_lowercase();

    if cmd.starts_with("git diff") || cmd.starts_with("git show") {
        filter_generic(output, GIT_LINE_THRESHOLD, GIT_BYTE_THRESHOLD)
    } else if cmd.starts_with("git status") {
        filter_git_status(output)
    } else if cmd.starts_with("git log") {
        filter_generic(output, GIT_LINE_THRESHOLD, GIT_BYTE_THRESHOLD)
    } else {
        filter_generic(output, 500, 51200)
    }
}

fn filter_git_status(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    // Short status: pass through
    if lines.len() <= 30 {
        return output.to_string();
    }

    // Count changed files
    let changed: Vec<&str> = lines
        .iter()
        .filter(|l| l.trim_start().starts_with("modified:") || l.trim_start().starts_with("new file:") || l.trim_start().starts_with("deleted:"))
        .copied()
        .collect();

    let shown: Vec<&str> = lines.iter().take(15).copied().collect();
    let omitted = changed.len().saturating_sub(10);

    format!(
        "{}\n[ecotokens] ... {} more changed files omitted ({} total changes) ...",
        shown.join("\n"),
        omitted,
        changed.len(),
    )
}
