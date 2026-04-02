use chrono::{Duration, Utc};
use ecotokens::metrics::store::{
    append_to, read_from, write_to, CommandFamily, FilterMode, Interception,
};
use tempfile::TempDir;

fn metrics_file(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join("metrics.db")
}

fn make_interception_with(
    command: &str,
    family: CommandFamily,
    git_root: Option<&str>,
    timestamp: &str,
) -> Interception {
    let mut item = Interception::new(
        command.to_string(),
        family,
        git_root.map(|s| s.to_string()),
        100,
        50,
        FilterMode::Filtered,
        false,
        10,
        None,
        None,
    );
    item.timestamp = timestamp.to_string();
    item
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn days_ago_rfc3339(days: i64) -> String {
    (Utc::now() - Duration::days(days)).to_rfc3339()
}

// ── write_to ──────────────────────────────────────────────────────────────────

#[test]
fn write_to_replaces_file_with_subset() {
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);

    let items: Vec<Interception> = (0..5)
        .map(|i| {
            make_interception_with(
                &format!("cmd{i}"),
                CommandFamily::Git,
                Some("/repo"),
                &now_rfc3339(),
            )
        })
        .collect();

    for item in &items {
        append_to(&path, item).unwrap();
    }

    // Keep only 2
    write_to(&path, &items[..2]).unwrap();

    let result = read_from(&path).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn write_to_empty_truncates_file() {
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);

    let item = make_interception_with("git status", CommandFamily::Git, None, &now_rfc3339());
    append_to(&path, &item).unwrap();

    write_to(&path, &[]).unwrap();

    let result = read_from(&path).unwrap();
    assert!(result.is_empty(), "file should be empty after write_to([])");
}

#[test]
fn write_to_preserves_content_faithfully() {
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);

    let original = make_interception_with(
        "cargo build",
        CommandFamily::Cargo,
        Some("/myrepo"),
        &now_rfc3339(),
    );
    write_to(&path, std::slice::from_ref(&original)).unwrap();

    let result = read_from(&path).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].command, "cargo build");
    assert_eq!(result[0].command_family, CommandFamily::Cargo);
    assert_eq!(result[0].git_root.as_deref(), Some("/myrepo"));
}

// ── clear scenarios (partition logic, tested through data transforms) ──────────

/// Helper: returns (to_delete, to_keep) using the same logic as cmd_clear.
fn partition_by_family(
    items: Vec<Interception>,
    family: &CommandFamily,
) -> (Vec<Interception>, Vec<Interception>) {
    items
        .into_iter()
        .partition(|item| &item.command_family == family)
}

fn partition_by_project(
    items: Vec<Interception>,
    project: &str,
) -> (Vec<Interception>, Vec<Interception>) {
    items.into_iter().partition(|item| {
        let item_root = item.git_root.as_deref().unwrap_or("").trim();
        if project.trim() == "(unknown)" {
            item_root.is_empty()
        } else {
            item_root == project.trim()
        }
    })
}

fn partition_before_date(
    items: Vec<Interception>,
    cutoff: chrono::DateTime<Utc>,
) -> (Vec<Interception>, Vec<Interception>) {
    use chrono::DateTime;
    items.into_iter().partition(|item| {
        DateTime::parse_from_rfc3339(&item.timestamp)
            .map(|ts| ts.with_timezone(&Utc) < cutoff)
            .unwrap_or(false)
    })
}

#[test]
fn clear_by_family_removes_only_target_family() {
    let items = vec![
        make_interception_with("git status", CommandFamily::Git, None, &now_rfc3339()),
        make_interception_with("cargo build", CommandFamily::Cargo, None, &now_rfc3339()),
        make_interception_with("git diff", CommandFamily::Git, None, &now_rfc3339()),
    ];

    let (deleted, kept) = partition_by_family(items, &CommandFamily::Git);

    assert_eq!(deleted.len(), 2, "2 git interceptions should be deleted");
    assert_eq!(kept.len(), 1, "1 cargo interception should be kept");
    assert!(kept
        .iter()
        .all(|i| i.command_family == CommandFamily::Cargo));
}

#[test]
fn clear_by_project_removes_only_target_project() {
    let items = vec![
        make_interception_with(
            "git status",
            CommandFamily::Git,
            Some("/project/a"),
            &now_rfc3339(),
        ),
        make_interception_with(
            "git status",
            CommandFamily::Git,
            Some("/project/b"),
            &now_rfc3339(),
        ),
        make_interception_with(
            "git status",
            CommandFamily::Git,
            Some("/project/a"),
            &now_rfc3339(),
        ),
    ];

    let (deleted, kept) = partition_by_project(items, "/project/a");

    assert_eq!(deleted.len(), 2);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].git_root.as_deref(), Some("/project/b"));
}

#[test]
fn clear_by_date_removes_old_entries() {
    let old_ts = days_ago_rfc3339(10);
    let new_ts = now_rfc3339();

    let items = vec![
        make_interception_with("old cmd", CommandFamily::Git, None, &old_ts),
        make_interception_with("new cmd", CommandFamily::Git, None, &new_ts),
    ];

    // cutoff = 5 days ago → old (10d) should be deleted, new (now) kept
    let cutoff = Utc::now() - Duration::days(5);
    let (deleted, kept) = partition_before_date(items, cutoff);

    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0].command, "old cmd");
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].command, "new cmd");
}

#[test]
fn clear_all_removes_everything() {
    let dir = TempDir::new().unwrap();
    let path = metrics_file(&dir);

    for _ in 0..3 {
        let item = make_interception_with("git status", CommandFamily::Git, None, &now_rfc3339());
        append_to(&path, &item).unwrap();
    }

    // Simulate --all: keep nothing
    write_to(&path, &[]).unwrap();

    let result = read_from(&path).unwrap();
    assert!(result.is_empty(), "all interceptions should be deleted");
}

#[test]
fn clear_unknown_family_returns_zero_matches() {
    let items = vec![
        make_interception_with("git status", CommandFamily::Git, None, &now_rfc3339()),
        make_interception_with("cargo build", CommandFamily::Cargo, None, &now_rfc3339()),
    ];

    // Partition by a family that isn't present → nothing deleted
    let (deleted, kept) = partition_by_family(items, &CommandFamily::Python);

    assert_eq!(deleted.len(), 0);
    assert_eq!(kept.len(), 2);
}

#[test]
fn clear_combined_family_and_project_uses_and_logic() {
    let items = vec![
        // git + /repo/a  → matches both
        make_interception_with("git s", CommandFamily::Git, Some("/repo/a"), &now_rfc3339()),
        // git + /repo/b  → family matches, project doesn't
        make_interception_with("git s", CommandFamily::Git, Some("/repo/b"), &now_rfc3339()),
        // cargo + /repo/a → project matches, family doesn't
        make_interception_with(
            "cargo b",
            CommandFamily::Cargo,
            Some("/repo/a"),
            &now_rfc3339(),
        ),
    ];

    let target_family = CommandFamily::Git;
    let target_project = "/repo/a";

    let (deleted, kept): (Vec<_>, Vec<_>) = items.into_iter().partition(|item| {
        item.command_family == target_family
            && item.git_root.as_deref().unwrap_or("").trim() == target_project
    });

    assert_eq!(deleted.len(), 1, "only git+/repo/a should be deleted");
    assert_eq!(kept.len(), 2);
}

#[test]
fn clear_by_project_unknown_removes_entries_without_git_root() {
    let items = vec![
        make_interception_with("git s", CommandFamily::Git, None, &now_rfc3339()),
        make_interception_with("git s", CommandFamily::Git, Some(""), &now_rfc3339()),
        make_interception_with("git s", CommandFamily::Git, Some("/repo/a"), &now_rfc3339()),
    ];

    let (deleted, kept) = partition_by_project(items, "(unknown)");

    assert_eq!(
        deleted.len(),
        2,
        "None et Some(\"\") doivent être supprimés"
    );
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].git_root.as_deref(), Some("/repo/a"));
}
