use crate::filter::generic::filter_generic;
use lazy_regex::regex;
use std::collections::HashMap;
use std::collections::HashSet;

const FS_LINE_THRESHOLD: u32 = 100;

/// Noisy directories that should be excluded from ls output.
const NOISY_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "__pycache__",
    ".next",
    "dist",
    "build",
    ".cache",
    ".venv",
    "venv",
    "vendor",
    ".tox",
    "coverage",
    ".nyc_output",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    "htmlcov",
    ".eggs",
    "buck-out",
    ".gradle",
    ".idea",
    ".vscode",
];

fn noisy_dirs_set() -> &'static HashSet<&'static str> {
    static NOISY_DIRS_SET: std::sync::OnceLock<HashSet<&'static str>> = std::sync::OnceLock::new();
    NOISY_DIRS_SET.get_or_init(|| NOISY_DIRS.iter().copied().collect())
}

fn is_noisy_entry(name: &str) -> bool {
    let base = name.trim().trim_end_matches('/');
    if noisy_dirs_set().contains(base) {
        return true;
    }
    // Match *.egg-info pattern
    if base.ends_with(".egg-info") {
        return true;
    }
    false
}

/// Filter filesystem command output (ls, find, tree, diff, wc).
pub fn filter_fs(command: &str, output: &str) -> String {
    let cmd = command.trim().to_lowercase();
    if cmd.starts_with("ls") {
        filter_ls(output)
    } else if cmd.starts_with("find") {
        filter_find(output)
    } else if cmd.starts_with("tree") {
        filter_tree(output)
    } else if cmd.starts_with("diff") {
        filter_diff(output)
    } else if cmd.starts_with("wc") {
        filter_wc(output)
    } else {
        filter_generic(output, FS_LINE_THRESHOLD, 51200)
    }
}

fn filter_ls(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();

    // Filter out noisy directories
    let filtered: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| {
            // For ls -l lines, the last token is the filename
            // For simple ls, each token is a filename
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.is_empty() {
                return true; // Keep blank lines (section separators)
            }
            let name = tokens.last().unwrap_or(&"");
            !is_noisy_entry(name)
        })
        .collect();

    let result = filtered.join("\n");

    if filtered.len() < lines.len() {
        let removed = lines.len() - filtered.len();
        if result.is_empty() {
            format!("[ecotokens] {} noisy entries excluded", removed)
        } else {
            format!("{}\n[ecotokens] {} noisy entries excluded", result, removed)
        }
    } else if lines.len() > FS_LINE_THRESHOLD as usize {
        filter_generic(output, FS_LINE_THRESHOLD, 51200)
    } else {
        output.to_string()
    }
}

const FIND_MAX_LINES: usize = 500;

fn filter_find(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();

    if lines.len() <= 50 {
        return output.to_string();
    }

    // Filter out entries under noisy directories
    let filtered: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| {
            let path = line.trim();
            // Check if any component of the path is a noisy dir
            for component in path.split('/') {
                if is_noisy_entry(component) {
                    return false;
                }
            }
            true
        })
        .collect();

    let noisy_removed = lines.len() - filtered.len();
    let source = if filtered.len() < lines.len() {
        &filtered[..]
    } else {
        &lines[..]
    };

    // Group by parent directory to reduce repetition
    let grouped = group_by_directory(source);
    let grouped_lines: Vec<&str> = grouped.lines().collect();

    let mut result = if grouped_lines.len() > FIND_MAX_LINES {
        // Still too long — truncate with a marker
        let head: Vec<&str> = grouped_lines.iter().take(FIND_MAX_LINES).copied().collect();
        let omitted = grouped_lines.len().saturating_sub(FIND_MAX_LINES);
        format!(
            "{}\n[ecotokens] ... {} more entries omitted ({} total) ...",
            head.join("\n"),
            omitted,
            grouped_lines.len(),
        )
    } else {
        grouped
    };

    if noisy_removed > 0 {
        result.push_str(&format!(
            "\n[ecotokens] {} entries in noisy dirs excluded",
            noisy_removed
        ));
    }

    result
}

/// Group file paths by their parent directory to reduce repetition.
fn group_by_directory(paths: &[&str]) -> String {
    let mut dir_files: HashMap<String, Vec<String>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for path in paths {
        let path = path.trim();
        if path.is_empty() {
            continue;
        }
        // Find parent directory
        let (dir, file) = if let Some(pos) = path.rfind('/') {
            (path[..pos].to_string(), path[pos + 1..].to_string())
        } else {
            (String::from("."), path.to_string())
        };

        if !dir_files.contains_key(&dir) {
            order.push(dir.clone());
        }
        dir_files.entry(dir).or_default().push(file);
    }

    let mut result = Vec::new();
    for dir in &order {
        let files = &dir_files[dir];
        if files.len() == 1 {
            // Single file: show full path
            result.push(format!("{}/{}", dir, files[0]));
        } else {
            // Multiple files: group under directory header
            result.push(format!("{}/", dir));
            for f in files {
                result.push(format!("  {}", f));
            }
        }
    }

    result.join("\n")
}

/// Filter `tree` output: remove noisy dirs, limit to 200 lines.
pub fn filter_tree(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 50 {
        return output.to_string();
    }

    let filtered: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| {
            if line.trim().is_empty() {
                return true;
            }
            // Check if any component of the tree branch is a noisy directory.
            // Tree output usually uses │, ├, └, ──.
            let clean = line.replace(['│', '├', '└', '─'], " ");
            !clean
                .split_whitespace()
                .any(|token| token.split('/').any(is_noisy_entry))
        })
        .collect();

    const MAX_TREE_LINES: usize = 200;
    let total = filtered.len();
    if total <= MAX_TREE_LINES {
        return filtered.join("\n");
    }

    let head: Vec<&str> = filtered.iter().take(MAX_TREE_LINES).copied().collect();
    format!(
        "{}\n... +{} more lines",
        head.join("\n"),
        total - MAX_TREE_LINES
    )
}

/// Filter `diff` output: keep only changed lines (+/-/@@/---/+++), limit to 200.
pub fn filter_diff(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 20 {
        return output.to_string();
    }

    let significant: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| l.starts_with('+') || l.starts_with('-') || l.starts_with("@@"))
        .collect();

    const MAX_DIFF_LINES: usize = 200;
    let total = significant.len();
    if total == 0 {
        return output.to_string();
    }
    if total <= MAX_DIFF_LINES {
        return significant.join("\n");
    }

    let head: Vec<&str> = significant.iter().take(MAX_DIFF_LINES).copied().collect();
    format!(
        "{}\n[ecotokens] ... {} more changed lines omitted ...",
        head.join("\n"),
        total - MAX_DIFF_LINES
    )
}

/// Filter `wc` output: compact format `NL NW NC file`.
pub fn filter_wc(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 5 {
        return output.to_string();
    }

    let wc_re = regex!(r"^\s*(\d+)\s+(\d+)\s+(\d+)(?:\s+(.+))?$");
    let mut compact: Vec<String> = Vec::new();

    for line in &lines {
        if let Some(caps) = wc_re.captures(line) {
            let lines_n = caps.get(1).map_or("", |m| m.as_str());
            let words_n = caps.get(2).map_or("", |m| m.as_str());
            let bytes_n = caps.get(3).map_or("", |m| m.as_str());
            let file = caps.get(4).map_or("", |m| m.as_str().trim());
            if file.is_empty() {
                compact.push(format!("{}L {}W {}C", lines_n, words_n, bytes_n));
            } else {
                compact.push(format!("{}L {}W {}C {}", lines_n, words_n, bytes_n, file));
            }
        } else {
            compact.push(line.to_string());
        }
    }

    compact.join("\n")
}
