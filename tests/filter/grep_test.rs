use ecotokens::filter::grep::filter_grep;

#[test]
fn short_grep_passes_through() {
    let input = "src/main.rs:10:fn main() {\n";
    let out = filter_grep(input);
    assert!(out.contains("fn main"), "short grep output should pass through");
}

#[test]
fn grep_groups_by_file() {
    let mut input = String::new();
    for i in 1..=35 {
        input.push_str(&format!("src/main.rs:{}:some match here\n", i));
    }
    let out = filter_grep(&input);
    assert!(out.contains("src/main.rs"), "file should appear in output");
    assert!(out.contains("🔍"), "header should be present");
}

#[test]
fn grep_limits_matches_per_file() {
    let mut input = String::new();
    // Need > 30 lines (PASSTHROUGH_THRESHOLD) and > 10 (MAX_MATCHES_PER_FILE) per file
    for i in 1..=35 {
        input.push_str(&format!("src/lib.rs:{}:match\n", i));
    }
    let out = filter_grep(&input);
    assert!(out.contains("... +"), "should indicate more matches");
}

#[test]
fn grep_truncates_long_lines() {
    let long_line = "a".repeat(200);
    let input = format!("src/file.rs:1:{}\nsrc/file.rs:2:{}\nsrc/file.rs:3:{}\nsrc/file.rs:4:{}\nsrc/file.rs:5:{}\nsrc/file.rs:6:{}\nsrc/file.rs:7:{}\nsrc/file.rs:8:{}\nsrc/file.rs:9:{}\nsrc/file.rs:10:{}\nsrc/file.rs:11:{}\nsrc/file.rs:12:{}\nsrc/file.rs:13:{}\nsrc/file.rs:14:{}\nsrc/file.rs:15:{}\nsrc/file.rs:16:{}\nsrc/file.rs:17:{}\nsrc/file.rs:18:{}\nsrc/file.rs:19:{}\nsrc/file.rs:20:{}\nsrc/file.rs:21:{}\nsrc/file.rs:22:{}\nsrc/file.rs:23:{}\nsrc/file.rs:24:{}\nsrc/file.rs:25:{}\nsrc/file.rs:26:{}\nsrc/file.rs:27:{}\nsrc/file.rs:28:{}\nsrc/file.rs:29:{}\nsrc/file.rs:30:{}\nsrc/file.rs:31:{}\n",
        long_line, long_line, long_line, long_line, long_line,
        long_line, long_line, long_line, long_line, long_line,
        long_line, long_line, long_line, long_line, long_line,
        long_line, long_line, long_line, long_line, long_line,
        long_line, long_line, long_line, long_line, long_line,
        long_line, long_line, long_line, long_line, long_line,
        long_line
    );
    let out = filter_grep(&input);
    // Lines should be truncated
    assert!(!out.contains(&long_line), "long lines should be truncated");
}

#[test]
fn grep_counts_files() {
    let mut input = String::new();
    for i in 1..=35 {
        input.push_str(&format!("src/file{}.rs:1:match here\n", i));
    }
    let out = filter_grep(&input);
    assert!(out.contains("35 files") || out.contains("35 matches"), "should count matches");
}
