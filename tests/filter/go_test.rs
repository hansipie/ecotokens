use ecotokens::filter::go::filter_go;

#[test]
fn short_go_test_passes_through() {
    let input = "ok  \tgithub.com/example/pkg\t0.123s\n";
    let out = filter_go("go test ./...", input);
    assert!(out.contains("ok"), "short go test should pass through");
}

#[test]
fn go_test_keeps_failures() {
    let input = "=== RUN   TestFoo
--- FAIL: TestFoo (0.00s)
    foo_test.go:10: expected 1, got 2
--- PASS: TestBar (0.00s)
FAIL
FAIL\tgithub.com/example/pkg\t0.001s
";
    let out = filter_go("go test ./...", input);
    assert!(out.contains("FAIL: TestFoo"), "failed test should be kept");
    assert!(out.contains("foo_test.go:10"), "failure context should be kept");
    assert!(out.contains("✓"), "summary should show pass count");
    assert!(out.contains("✗"), "summary should show fail count");
}

#[test]
fn go_test_summary_shows_counts() {
    let mut input = String::new();
    for i in 0..10 {
        input.push_str(&format!("--- PASS: TestCase{} (0.00s)\n", i));
    }
    input.push_str("--- FAIL: TestBad (0.00s)\n");
    input.push_str("    bad_test.go:5: assertion failed\n");
    input.push_str("FAIL\n");
    let out = filter_go("go test ./...", &input);
    assert!(out.contains("✗ 1 failed"), "should show 1 failure");
}

#[test]
fn go_build_keeps_errors() {
    let input = "# github.com/example/pkg\n./main.go:10:5: undefined: Foo\n./main.go:15:3: cannot use x (type int) as type string\n";
    let out = filter_go("go build ./...", input);
    assert!(out.contains("undefined: Foo"), "build error should be kept");
    assert!(out.contains("cannot use x"), "second error should be kept");
}

#[test]
fn go_build_short_passes_through() {
    let input = "# github.com/example/pkg\n./main.go:5:1: syntax error\n";
    let out = filter_go("go build ./...", input);
    assert!(out.contains("syntax error"), "short build error should be kept");
}

#[test]
fn go_vet_keeps_issues() {
    let input = "# github.com/example/pkg\n./main.go:10:1: unreachable code\n./utils.go:20:5: suspect call\n";
    let out = filter_go("go vet ./...", input);
    assert!(out.contains("unreachable code"), "vet issue should be kept");
    assert!(out.contains("utils.go"), "second vet issue should be kept");
}

#[test]
fn go_other_commands_use_generic() {
    let input = "go version go1.21.0 linux/amd64\n";
    let out = filter_go("go version", input);
    assert!(!out.is_empty(), "should return something");
}

#[test]
fn golangci_lint_short_passes_through() {
    let input = "src/main.go:10:5: exported function Foo should have comment (golint)\n";
    let out = filter_go("golangci-lint run", input);
    assert!(out.contains("Foo"), "short golangci-lint output should pass through");
}

#[test]
fn golangci_lint_groups_by_linter() {
    let mut input = String::new();
    for i in 0..35 {
        input.push_str(&format!(
            "src/main.go:{}:1: exported function Func{} should have comment (golint)\n",
            i, i
        ));
        input.push_str(&format!(
            "src/utils.go:{}:1: variable x is unused (unused)\n",
            i
        ));
    }
    let out = filter_go("golangci-lint run ./...", &input);
    assert!(out.contains("[golint]"), "should group by linter name");
    assert!(out.contains("[unused]"), "should group by linter name");
}

#[test]
fn golangci_lint_deduplicates_messages() {
    let mut input = String::new();
    for i in 0..35 {
        input.push_str(&format!(
            "src/file{}.go:1:1: exported function should have comment (golint)\n",
            i
        ));
    }
    let out = filter_go("golangci-lint run", &input);
    // The same message repeated 35 times should be deduplicated
    assert!(out.contains("35x") || out.contains("(35x)"), "repeated messages should show count");
}
