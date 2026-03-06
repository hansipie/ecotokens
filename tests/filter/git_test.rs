use ecotokens::filter::git::filter_git;

#[test]
fn clean_status_passes_through() {
    let input = "On branch main\nnothing to commit, working tree clean\n";
    let out = filter_git("git status", input);
    assert_eq!(out, input);
}

#[test]
fn status_with_many_changed_files_is_summarized() {
    let mut input = String::from("On branch main\nChanges not staged for commit:\n");
    for i in 0..60 {
        input.push_str(&format!("\tmodified:   src/file{i}.rs\n"));
    }
    let out = filter_git("git status", &input);
    assert!(out.len() < input.len(), "long status should be shorter");
    assert!(out.contains("[ecotokens]"), "should have summary marker");
}

#[test]
fn long_diff_is_truncated() {
    let mut input = String::from("diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n");
    for i in 0..600 {
        input.push_str(&format!("+line {i}\n"));
    }
    let out = filter_git("git diff", &input);
    assert!(out.len() < input.len(), "long diff should be truncated");
    assert!(out.contains("[ecotokens]"), "should have summary marker");
}

#[test]
fn short_diff_passes_through() {
    let input = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n+added line\n";
    let out = filter_git("git diff", input);
    assert_eq!(out, input);
}
