use ecotokens::rewrite::diff::prune_retention;
use std::fs;
use std::time::{Duration, SystemTime};
use tempfile::tempdir;

fn touch_with_mtime(path: &std::path::Path, mtime: SystemTime) {
    fs::write(path, "diff content").unwrap();
    let file = fs::File::open(path).unwrap();
    file.set_modified(mtime).unwrap();
}

#[test]
fn prunes_oldest_first_beyond_retention() {
    let tmp = tempdir().unwrap();
    let base = SystemTime::now();
    let mut paths = Vec::new();
    for i in 0..5 {
        let p = tmp.path().join(format!("ecotokens-rewrite-{i}-uuid.diff"));
        touch_with_mtime(&p, base + Duration::from_secs(i as u64));
        paths.push(p);
    }

    prune_retention(tmp.path(), 3).unwrap();

    let remaining: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(remaining.len(), 3, "got: {remaining:?}");
    // The two oldest (index 0 and 1) must be gone; the three newest remain.
    assert!(!paths[0].exists());
    assert!(!paths[1].exists());
    assert!(paths[2].exists());
    assert!(paths[3].exists());
    assert!(paths[4].exists());
}

#[test]
fn retention_zero_disables_pruning() {
    let tmp = tempdir().unwrap();
    let base = SystemTime::now();
    for i in 0..10 {
        let p = tmp.path().join(format!("ecotokens-rewrite-{i}-uuid.diff"));
        touch_with_mtime(&p, base + Duration::from_secs(i as u64));
    }

    prune_retention(tmp.path(), 0).unwrap();

    let remaining = fs::read_dir(tmp.path()).unwrap().count();
    assert_eq!(remaining, 10);
}

#[test]
fn does_not_prune_when_under_the_limit() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join("ecotokens-rewrite-0-uuid.diff");
    touch_with_mtime(&p, SystemTime::now());

    prune_retention(tmp.path(), 50).unwrap();

    assert!(p.exists());
}

#[test]
fn ignores_unrelated_files_in_the_directory() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("unrelated.txt"), "not a diff").unwrap();
    let base = SystemTime::now();
    for i in 0..5 {
        let p = tmp.path().join(format!("ecotokens-rewrite-{i}-uuid.diff"));
        touch_with_mtime(&p, base + Duration::from_secs(i as u64));
    }

    prune_retention(tmp.path(), 3).unwrap();

    assert!(tmp.path().join("unrelated.txt").exists());
    let diff_count = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".diff"))
        .count();
    assert_eq!(diff_count, 3);
}
