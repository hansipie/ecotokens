use ecotokens::rewrite::detect::{classify, is_predominantly_code, ContentKind};

#[test]
fn predominantly_code_input_is_flagged() {
    let code = r#"
fn main() {
    let x = compute();
    if x > 0 {
        println!("positive: {}", x);
    } else {
        println!("non-positive: {}", x);
    }
}

fn compute() -> i32 {
    let mut total = 0;
    for i in 0..10 {
        total += i;
    }
    total
}
"#;
    assert!(is_predominantly_code(code));
}

#[test]
fn prose_is_not_flagged() {
    let prose = "The quarterly results show strong growth across every region. \
        Customer satisfaction improved significantly, driven mainly by faster response times \
        and a redesigned onboarding flow that new users have praised in feedback surveys.";
    assert!(!is_predominantly_code(prose));
}

#[test]
fn prose_containing_a_single_code_block_is_not_flagged() {
    let text = "To fix the bug, change the timeout value like this:\n\n```\ntimeout = 30\n```\n\n\
        This resolves the issue reported by several users last week, and the team has \
        confirmed the fix in staging before rolling it out more broadly to production.";
    assert!(!is_predominantly_code(text));
}

#[test]
fn empty_input_is_not_flagged() {
    assert!(!is_predominantly_code(""));
    assert!(!is_predominantly_code("   "));
}

#[test]
fn a_lone_massive_fenced_block_is_flagged() {
    let mut code = String::from("```\n");
    for i in 0..30 {
        code.push_str(&format!("let v{i} = {i};\n"));
    }
    code.push_str("```\n");
    assert!(is_predominantly_code(&code));
}

// ── ContentKind classifier (US6 content gating, FR-035/SC-011) ─────────────
//
// This classifier gates the *automatic* pipeline stage only — it must never
// let code, stack traces, error messages, diffs, or structured data reach
// the local model unattended. Every case here asserts `ContentKind != Prose`
// for exactly the content shapes FR-035 requires passed through untouched.

#[test]
fn classify_flags_code_as_not_prose() {
    let code = r#"
fn main() {
    let x = compute();
    println!("{}", x);
}
"#;
    assert_eq!(classify(code), ContentKind::Code);
}

#[test]
fn classify_flags_a_python_traceback_as_diagnostic() {
    let tb = "Traceback (most recent call last):\n  File \"app.py\", line 12, in <module>\n    \
        raise ValueError(\"bad input\")\nValueError: bad input";
    assert_eq!(classify(tb), ContentKind::Diagnostic);
}

#[test]
fn classify_flags_a_rust_panic_as_diagnostic() {
    let panic_msg = "thread 'main' panicked at 'index out of bounds', src/main.rs:42:5\n\
        note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace";
    assert_eq!(classify(panic_msg), ContentKind::Diagnostic);
}

#[test]
fn classify_flags_a_java_style_stack_trace_as_diagnostic() {
    let trace = "java.lang.NullPointerException\n    at com.example.Foo.bar(Foo.java:42)\n    \
        at com.example.Main.main(Main.java:10)";
    assert_eq!(classify(trace), ContentKind::Diagnostic);
}

#[test]
fn classify_flags_a_unified_diff_as_structured() {
    let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,4 @@\n \
        fn main() {\n+    println!(\"hi\");\n }\n";
    assert_eq!(classify(diff), ContentKind::Structured);
}

#[test]
fn classify_flags_json_as_structured() {
    let json = r#"{"id": 1, "name": "widget", "tags": ["a", "b", "c"], "active": true}"#;
    assert_eq!(classify(json), ContentKind::Structured);
}

#[test]
fn classify_flags_toml_like_config_as_structured() {
    let toml = "[package]\nname = \"ecotokens\"\nversion = \"0.25.2\"\nedition = \"2021\"\n";
    assert_eq!(classify(toml), ContentKind::Structured);
}

#[test]
fn classify_flags_yaml_like_config_as_structured() {
    let yaml = "name: ecotokens\nversion: 0.25.2\ndependencies:\n  serde: \"1\"\n  clap: \"4\"\n";
    assert_eq!(classify(yaml), ContentKind::Structured);
}

#[test]
fn classify_flags_ordinary_prose_as_prose() {
    let prose = "The quarterly results show strong growth across every region. Customer \
        satisfaction improved significantly, driven mainly by faster response times and a \
        redesigned onboarding flow that new users have praised in feedback surveys.";
    assert_eq!(classify(prose), ContentKind::Prose);
}

#[test]
fn classify_biases_uncertain_content_away_from_prose() {
    // Short, low-signal fragments must never be waved through as prose —
    // the automatic stage should skip them rather than guess (research.md §7).
    assert_ne!(classify("x=1"), ContentKind::Prose);
    assert_ne!(classify("42"), ContentKind::Prose);
}
