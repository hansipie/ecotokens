use ecotokens::filter::fs::filter_fs;

#[test]
fn short_ls_passes_through() {
    let input = "file1.rs\nfile2.rs\nfile3.rs\n";
    let out = filter_fs("ls", input);
    assert_eq!(out, input);
}

#[test]
fn long_ls_is_truncated() {
    let lines: Vec<String> = (0..200).map(|i| format!("file{i}.rs")).collect();
    let input = lines.join("\n");
    let out = filter_fs("ls", &input);
    assert!(out.len() < input.len(), "long ls should be truncated");
    assert!(out.contains("[ecotokens]"), "should have summary marker");
}

#[test]
fn long_find_is_summarized() {
    let lines: Vec<String> = (0..600).map(|i| format!("./src/file{i}.rs")).collect();
    let input = lines.join("\n");
    let out = filter_fs("find . -name '*.rs'", &input);
    assert!(out.len() < input.len(), "long find should be summarized");
    assert!(out.contains("[ecotokens]"), "should have summary marker");
}

#[test]
fn tree_short_passes_through() {
    let input = ".\n├── src\n│   └── main.rs\n└── Cargo.toml\n";
    let out = filter_fs("tree", input);
    assert!(out.contains("main.rs"), "short tree output should pass through");
}

#[test]
fn tree_filters_noisy_dirs() {
    let mut input = String::from(".\n");
    for i in 0..60 {
        input.push_str(&format!("├── src/file{}.rs\n", i));
    }
    input.push_str("├── node_modules/some/package\n");
    input.push_str("└── target/debug/binary\n");
    let out = filter_fs("tree", &input);
    assert!(!out.contains("node_modules"), "node_modules should be filtered");
    assert!(!out.contains("target/debug"), "target should be filtered");
}

#[test]
fn diff_short_passes_through() {
    let input = "--- a/file.rs\n+++ b/file.rs\n@@ -1,3 +1,4 @@\n+new line\n";
    let out = filter_fs("diff a.rs b.rs", input);
    assert!(out.contains("new line"), "short diff should pass through");
}

#[test]
fn diff_removes_context_lines() {
    let mut input = String::new();
    for i in 0..25 {
        input.push_str(&format!(" context line {}\n", i));
        input.push_str(&format!("+added line {}\n", i));
        input.push_str(&format!("-removed line {}\n", i));
    }
    let out = filter_fs("diff old.rs new.rs", &input);
    assert!(out.contains("+added"), "added lines should be kept");
    assert!(out.contains("-removed"), "removed lines should be kept");
    assert!(!out.contains(" context"), "context lines should be removed");
}

#[test]
fn wc_short_passes_through() {
    let input = "  42  100  500 file.rs\n";
    let out = filter_fs("wc file.rs", input);
    assert!(out.contains("42"), "line count should be present");
}

#[test]
fn wc_formats_compactly() {
    let mut input = String::new();
    for i in 0..10 {
        input.push_str(&format!("  {}  {}  {} file{}.rs\n", i * 10, i * 20, i * 100, i));
    }
    let out = filter_fs("wc *.rs", &input);
    assert!(out.contains('L'), "should use compact L/W/C format");
    assert!(out.contains('W'), "should use compact L/W/C format");
    assert!(out.contains('C'), "should use compact L/W/C format");
}
