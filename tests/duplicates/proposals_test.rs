use ecotokens::duplicates::proposals::generate_proposals;
use ecotokens::duplicates::{CodeSegment, ProposalKind};

fn make_seg(file: &str, line: u64, content: &str) -> CodeSegment {
    let lines = content.lines().count() as u64;
    CodeSegment {
        symbol_id: format!("{file}::fn"),
        file_path: file.to_string(),
        line_start: line,
        line_end: line + lines - 1,
        content: content.to_string(),
    }
}

#[test]
fn test_exact_duplicate_proposal() {
    let content = "fn foo() {\n    let x = 1;\n    let y = 2;\n    let z = x + y;\n    println!(\"{z}\");\n}\n";
    let segs = vec![make_seg("a.rs", 1, content), make_seg("b.rs", 10, content)];
    let proposals = generate_proposals(&segs, 100.0);
    assert_eq!(proposals.len(), 1);
    assert!(matches!(proposals[0].kind, ProposalKind::ExactDuplicate));
    let text = &proposals[0].text;
    assert!(
        text.to_lowercase().contains("exact"),
        "text should mention 'exact': {text}"
    );
    assert!(
        text.contains("a.rs"),
        "text should contain first file path: {text}"
    );
    assert!(
        text.contains("b.rs"),
        "text should contain second file path: {text}"
    );
}

#[test]
fn test_near_duplicate_proposal() {
    let a = "fn foo(x: i32) -> i32 {\n    let a = x * 2;\n    let b = a + 1;\n    let c = b - x;\n    let d = c * 3;\n    d\n}\n";
    let b = "fn bar(x: i32) -> i32 {\n    let a = x * 2;\n    let b = a + 1;\n    let c = b - x;\n    let d = c * 4;\n    d\n}\n";
    let segs = vec![make_seg("a.rs", 1, a), make_seg("b.rs", 1, b)];
    let proposals = generate_proposals(&segs, 85.0);
    assert_eq!(proposals.len(), 1);
    assert!(matches!(proposals[0].kind, ProposalKind::NearDuplicate));
    let text = &proposals[0].text;
    assert!(
        text.to_lowercase().contains("near-duplicate"),
        "text should mention 'near-duplicate': {text}"
    );
}

#[test]
fn test_subset_proposal() {
    let small = "fn helper() {\n    do_thing();\n    do_other();\n    finish();\n}\n";
    let large = "fn helper() {\n    do_thing();\n    do_other();\n    finish();\n}\nfn extra() {\n    more_stuff();\n}\n";
    let segs = vec![make_seg("a.rs", 1, small), make_seg("b.rs", 1, large)];
    let proposals = generate_proposals(&segs, 80.0);
    assert_eq!(proposals.len(), 1);
    assert!(matches!(proposals[0].kind, ProposalKind::SubsetOf));
    let text = &proposals[0].text;
    // Should reference the larger segment's file
    assert!(
        text.contains("b.rs"),
        "text should reference larger segment's file: {text}"
    );
}
