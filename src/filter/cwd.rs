pub fn project_root_for_cwd(dir: &std::path::Path) -> Option<String> {
    let git_root = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()
        .ok()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        });

    match git_root {
        Some(root) => Some(root),
        None if is_temporary_path(dir) => None,
        None => Some(dir.to_string_lossy().to_string()),
    }
}

fn is_temporary_path(path: &std::path::Path) -> bool {
    let temp_dir = std::env::temp_dir();
    if path.starts_with(&temp_dir) {
        return true;
    }

    match (path.canonicalize(), temp_dir.canonicalize()) {
        (Ok(path), Ok(temp_dir)) => path.starts_with(temp_dir),
        _ => false,
    }
}
