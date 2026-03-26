use super::post_handler::PostFilterResult;
use crate::tokens::counter::count_tokens;

/// Directories that are excluded from Glob results.
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
    ".idea",
    ".vscode",
];

fn is_noisy_path(path: &str) -> bool {
    for component in path.trim().split('/') {
        let c = component.trim_end_matches('/');
        if NOISY_DIRS.contains(&c) || c.ends_with(".egg-info") {
            return true;
        }
    }
    false
}

pub fn handle_glob(filenames: &str) -> PostFilterResult {
    if filenames.trim().is_empty() {
        return PostFilterResult::Passthrough;
    }

    let lines: Vec<&str> = filenames.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return PostFilterResult::Passthrough;
    }

    let clean: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| !is_noisy_path(l))
        .collect();
    let excluded_count = lines.len() - clean.len();

    if excluded_count == 0 {
        return PostFilterResult::Passthrough;
    }

    let mut output = clean.join("\n");
    if excluded_count > 0 {
        output.push_str(&format!(
            "\n[ecotokens: {} entries excluded — noisy dirs (node_modules, target, …)]",
            excluded_count
        ));
    }

    let tokens_before = count_tokens(filenames) as u32;
    let tokens_after = count_tokens(&output) as u32;

    PostFilterResult::Filtered {
        output,
        tokens_before,
        tokens_after,
    }
}
