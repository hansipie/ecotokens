use ecotokens::filter::markdown::filter_markdown;

fn make_md(n_lines: usize) -> String {
    let mut s = String::from("# Title\n\nIntro paragraph.\n\n## Section One\n\n");
    for i in 0..n_lines {
        s.push_str(&format!("Line {i} of content.\n"));
    }
    s.push_str("\n## Section Two\n\nShort section.\n");
    s
}

#[test]
fn short_file_passes_through() {
    let input = "# Hello\n\nSmall file.\n";
    let out = filter_markdown(input);
    assert_eq!(out, input);
}

#[test]
fn long_file_produces_toc_and_first_section() {
    let input = make_md(300);
    let out = filter_markdown(&input);
    assert!(out.len() < input.len(), "output should be shorter");
    assert!(out.contains("[ecotokens]"), "should contain marker");
}

#[test]
fn toc_includes_headings() {
    let input = make_md(300);
    let out = filter_markdown(&input);
    assert!(
        out.contains("Title") || out.contains("Section One"),
        "ToC should have headings"
    );
}

#[test]
fn file_without_headings_falls_back_to_generic() {
    let lines: Vec<String> = (1..=300).map(|i| format!("plain line {i}")).collect();
    let input = lines.join("\n");
    let out = filter_markdown(&input);
    assert!(
        out.contains("[ecotokens]"),
        "should fall back to generic summary"
    );
}
