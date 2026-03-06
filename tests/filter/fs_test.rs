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
