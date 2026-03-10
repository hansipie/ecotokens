use ecotokens::filter::js::filter_js;

#[test]
fn npm_install_long_output_shows_summary() {
    let mut input = String::new();
    for i in 0..50 {
        input.push_str(&format!(
            "npm http fetch GET 200 https://registry.npmjs.org/pkg{i} 123ms\n"
        ));
    }
    input.push_str("added 42 packages, and audited 100 packages in 5s\n");
    let out = filter_js("npm install", &input);
    assert!(out.contains("42 packages"), "summary should be kept");
    assert!(
        !out.contains("npm http fetch GET 200 https://registry.npmjs.org/pkg10"),
        "download noise should be removed"
    );
}

#[test]
fn npm_install_short_passes_through() {
    let input = "added 1 package in 0.5s\n";
    let out = filter_js("npm install lodash", input);
    assert!(
        out.contains("added 1 package"),
        "short output should pass through"
    );
}

#[test]
fn tsc_groups_errors_by_file() {
    let input = "src/app.ts(10,5): error TS2322: Type 'string' is not assignable to type 'number'.\n\
src/app.ts(20,3): error TS2345: Argument of type 'string' is not assignable to parameter of type 'number'.\n\
src/utils.ts(5,1): error TS2304: Cannot find name 'Foo'.\n";
    let out = filter_js("tsc --noEmit", input);
    assert!(out.contains("src/app.ts"), "file should be in output");
    assert!(
        out.contains("2 errors"),
        "error count for app.ts should be shown"
    );
    assert!(
        out.contains("src/utils.ts"),
        "second file should be in output"
    );
    assert!(out.contains("TS2322"), "error code should appear");
}

#[test]
fn tsc_deduplicates_repeated_error_codes() {
    let input = "src/app.ts(1,1): error TS2322: type error\n\
src/app.ts(2,2): error TS2322: another type error\n\
src/app.ts(3,3): error TS2322: yet another\n";
    let out = filter_js("tsc", input);
    assert!(
        out.contains("3x") || out.contains("(3x)"),
        "repeated code should be deduplicated"
    );
}

#[test]
fn tsc_single_error_is_summarized() {
    let input = "src/app.ts(1,1): error TS2322: type mismatch\n";
    let out = filter_js("tsc", input);
    // Even a single error is grouped by file for consistency
    assert!(
        out.contains("src/app.ts"),
        "file should appear in tsc output"
    );
    assert!(out.contains("TS2322"), "error code should be kept");
}

#[test]
fn vitest_keeps_failures_only() {
    // Need > 30 lines to trigger filtering
    let mut input = String::new();
    for i in 0..30 {
        input.push_str(&format!("✓ should pass test {} (1ms)\n", i));
    }
    input.push_str(" × should subtract numbers\n");
    input.push_str("   AssertionError: expected 3 to equal 2\n");
    input.push_str("Tests  30 passed | 1 failed (100ms)\n");
    let out = filter_js("vitest run", &input);
    assert!(out.contains("subtract"), "failing test should be kept");
    assert!(
        out.contains("AssertionError"),
        "failure context should be kept"
    );
    assert!(out.contains("✗"), "summary should show fail count");
}

#[test]
fn vitest_short_passes_through() {
    let input = "✓ should work (2ms)\nTests  1 passed (10ms)\n";
    let out = filter_js("vitest run", input);
    assert!(
        out.contains("should work"),
        "short vitest output should pass through"
    );
}

#[test]
fn eslint_json_output_summarized_by_file() {
    let input = r#"[{"filePath":"/project/src/app.js","messages":[{"ruleId":"no-unused-vars","severity":2,"message":"'x' is defined but never used.","line":5,"column":7}],"errorCount":1,"warningCount":0},{"filePath":"/project/src/utils.js","messages":[],"errorCount":0,"warningCount":0}]"#;
    let out = filter_js("eslint src/", input);
    assert!(out.contains("app.js"), "file with issues should appear");
    assert!(out.contains("1 issue"), "issue count should appear");
}

#[test]
fn js_unknown_command_uses_generic() {
    let input = "some output\n";
    let out = filter_js("node script.js", input);
    assert!(!out.is_empty(), "should return something");
}

#[test]
fn playwright_keeps_failures() {
    let mut input = String::new();
    for i in 0..10 {
        input.push_str(&format!("  ✓ test {} passes (100ms)\n", i));
    }
    input.push_str("  ✘ login test fails (500ms)\n");
    input.push_str("    Error: expected to find element\n");
    input.push_str("    at page.click (test.spec.ts:15:5)\n");
    let out = filter_js("npx playwright test", &input);
    assert!(
        out.contains("login test fails"),
        "failed test should be kept"
    );
    assert!(out.contains("Error:"), "error should be kept");
    assert!(out.contains("✗"), "summary should show failures");
}

#[test]
fn playwright_no_failures_passes_through() {
    let input = "  ✓ all tests pass (100ms)\n  ✓ another test (50ms)\n";
    let out = filter_js("playwright test", input);
    assert!(
        out.contains("all tests pass"),
        "passing tests should pass through"
    );
}

#[test]
fn prisma_removes_box_drawing() {
    let mut input = String::new();
    for _ in 0..15 {
        input.push_str("┌────────────────────────────────────────┐\n");
        input.push_str("│ Prisma Accelerate: speed up queries    │\n");
        input.push_str("└────────────────────────────────────────┘\n");
    }
    input.push_str("✓ Generated Prisma Client (5.0.0)\n");
    let out = filter_js("prisma generate", &input);
    assert!(!out.contains('┌'), "box drawing should be removed");
    assert!(out.contains("Generated"), "useful output should be kept");
}

#[test]
fn next_build_summarizes_routes() {
    let mut input = String::new();
    for i in 0..25 {
        input.push_str(&format!("○ /page{} (static)\n", i));
    }
    for i in 0..5 {
        input.push_str(&format!("λ /api/route{} (dynamic)\n", i));
    }
    input.push_str("Compiled in 3.2s\n");
    let out = filter_js("next build", &input);
    assert!(out.contains("routes"), "route count should be summarized");
    assert!(out.contains("static"), "static count should appear");
    assert!(out.contains("dynamic"), "dynamic count should appear");
}
