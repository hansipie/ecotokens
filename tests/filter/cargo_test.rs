use ecotokens::filter::cargo::filter_cargo;

#[test]
fn successful_build_shows_stats_only() {
    let input =
        "   Compiling ecotokens v0.2.0\n    Finished dev [unoptimized] target(s) in 1.23s\n";
    let out = filter_cargo("cargo build", input);
    assert!(out.contains("Finished"), "stats line should be kept");
}

#[test]
fn build_with_errors_preserves_errors() {
    let input = "error[E0308]: mismatched types\n --> src/main.rs:5:9\n  |\n5 |     let x: i32 = \"hello\";\n  |            ---   ^^^^^^^ expected `i32`, found `&str`\n";
    let out = filter_cargo("cargo build", input);
    assert!(out.contains("error"), "errors should be kept");
    assert!(out.contains("E0308"), "error code should be kept");
}

#[test]
fn many_warnings_produce_summary() {
    let mut input = String::new();
    for i in 0..30 {
        input.push_str(&format!(
            "warning: unused variable `x{i}`\n --> src/lib.rs:{i}:9\n\n"
        ));
    }
    input.push_str("warning: 30 warnings emitted\n");
    input.push_str("    Finished dev target(s) in 2.00s\n");
    let out = filter_cargo("cargo build", &input);
    assert!(out.contains("warning") || out.contains("Warning") || out.contains("[ecotokens]"));
}

#[test]
fn test_output_with_failures_keeps_failures() {
    let input = "running 5 tests\ntest foo ... ok\ntest bar ... FAILED\n\nfailures:\n\nfailures:\n    bar\n\ntest result: FAILED. 4 passed; 1 failed\n";
    let out = filter_cargo("cargo test", input);
    assert!(out.contains("FAILED"), "test failures should be kept");
    assert!(out.contains("bar"), "failed test name should be kept");
}
