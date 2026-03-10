use ecotokens::filter::gh::filter_gh;

#[test]
fn gh_pr_view_strips_html_comments() {
    let input = "number:\t42\ntitle:\tFix the bug\nstate:\tOPEN\nauthor:\talice\nbody:\n<!-- this is a comment -->\nThis PR fixes the issue.\n";
    let out = filter_gh("gh pr view 42", input);
    assert!(
        !out.contains("<!-- this is a comment -->"),
        "HTML comments should be removed"
    );
    assert!(out.contains("Fix the bug"), "title should be kept");
    assert!(out.contains("fixes the issue"), "body text should be kept");
}

#[test]
fn gh_pr_view_short_passes_through() {
    let input = "number:\t1\ntitle:\tSmall PR\nstate:\tOPEN\nauthor:\tbob\n";
    let out = filter_gh("gh pr view 1", input);
    assert!(out.contains("Small PR"), "title should be kept");
}

#[test]
fn gh_pr_list_compact_format() {
    let input = "1\tFix bug\tmain\t2024-01-01\tOPEN\n2\tAdd feature\tdev\t2024-01-02\tMERGED\n";
    let out = filter_gh("gh pr list", input);
    assert!(out.contains("#1"), "PR number should be formatted");
    assert!(out.contains("Fix bug"), "title should be kept");
}

#[test]
fn gh_issue_list_compact_format() {
    let input = "10\tBug report\tbug,high\t2024-01-01\tOPEN\n11\tFeature request\tenhancement\t2024-01-02\tCLOSED\n";
    let out = filter_gh("gh issue list", input);
    assert!(out.contains("#10"), "issue number should be formatted");
    assert!(out.contains("Bug report"), "title should be kept");
}

#[test]
fn gh_run_view_keeps_failures() {
    let mut input = String::from("✓ build (2m 10s)\n✓ lint (30s)\n");
    for _ in 0..30 {
        input.push_str("✓ some-step (10s)\n");
    }
    input.push_str("✗ test (1m 30s)\nerror: 3 tests failed\nconclusion: failure\n");
    let out = filter_gh("gh run view 123", &input);
    assert!(out.contains("test"), "failing step should be in output");
    assert!(out.contains("conclusion"), "conclusion should be kept");
}

#[test]
fn gh_pr_view_long_body_truncated() {
    let mut input =
        String::from("number:\t99\ntitle:\tBig PR\nstate:\tOPEN\nauthor:\tcarol\nbody:\n");
    for i in 0..300 {
        input.push_str(&format!("Body line {}\n", i));
    }
    let out = filter_gh("gh pr view 99", &input);
    assert!(out.contains("truncated"), "long body should be truncated");
}

#[test]
fn gh_unknown_subcommand_uses_generic() {
    let input = "some output from gh\n";
    let out = filter_gh("gh auth status", input);
    assert!(!out.is_empty(), "should return something");
}
