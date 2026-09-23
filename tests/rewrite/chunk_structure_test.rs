use ecotokens::rewrite::chunk::split_into_chunks;

fn reassembled(text: &str, budget: u32) -> String {
    let (chunks, _warnings) = split_into_chunks(text, budget);
    chunks.iter().map(|c| c.text.as_str()).collect::<String>()
}

#[test]
fn oversized_fenced_code_block_is_never_split() {
    let mut code = String::from("```\n");
    for i in 0..100 {
        code.push_str(&format!("let v{i} = {i};\n"));
    }
    code.push_str("```\n");
    let text = format!("Some intro prose.\n\n{code}\nSome closing prose.");

    let (chunks, _warnings) = split_into_chunks(&text, 20);
    let fence_chunk = chunks
        .iter()
        .find(|c| c.text.contains("```"))
        .expect("a chunk containing the fence must exist");
    assert!(fence_chunk.atomic, "fenced code chunk must be atomic");
    assert!(
        fence_chunk.text.trim_end().ends_with("```"),
        "the whole fence must stay in one chunk, got: {:?}",
        fence_chunk.text
    );
    assert_eq!(reassembled(&text, 20), text);
}

#[test]
fn oversized_markdown_table_is_never_split() {
    let mut table = String::from("| Col A | Col B |\n|---|---|\n");
    for i in 0..80 {
        table.push_str(&format!("| row{i}a | row{i}b |\n"));
    }
    let text = format!("Intro.\n\n{table}\nOutro.");

    let (chunks, _warnings) = split_into_chunks(&text, 15);
    let table_chunk = chunks
        .iter()
        .find(|c| c.text.contains("Col A"))
        .expect("a chunk containing the table header must exist");
    assert!(table_chunk.atomic, "table chunk must be atomic");
    assert!(table_chunk.text.contains("row79b"));
    assert_eq!(reassembled(&text, 15), text);
}

#[test]
fn oversized_list_group_is_never_split() {
    let mut list = String::new();
    for i in 0..80 {
        list.push_str(&format!("- item number {i} in the list\n"));
    }
    let text = format!("Here is a list:\n\n{list}\nDone.");

    let (chunks, _warnings) = split_into_chunks(&text, 15);
    let list_chunk = chunks
        .iter()
        .find(|c| c.text.contains("item number 0"))
        .expect("a chunk containing the list must exist");
    assert!(list_chunk.atomic, "list chunk must be atomic");
    assert!(list_chunk.text.contains("item number 79"));
    assert_eq!(reassembled(&text, 15), text);
}

#[test]
fn numbered_list_is_also_treated_as_atomic() {
    let mut list = String::new();
    for i in 1..=60 {
        list.push_str(&format!("{i}. step number {i}\n"));
    }
    let text = format!("Steps:\n\n{list}\nEnd.");
    let (chunks, _warnings) = split_into_chunks(&text, 15);
    let list_chunk = chunks
        .iter()
        .find(|c| c.text.contains("step number 1\n"))
        .expect("a chunk containing the numbered list must exist");
    assert!(list_chunk.atomic);
    assert_eq!(reassembled(&text, 15), text);
}

#[test]
fn small_fenced_block_within_budget_still_reassembles_exactly() {
    let text = "Before.\n\n```\nlet x = 1;\n```\n\nAfter.";
    assert_eq!(reassembled(text, 1000), text);
}

#[test]
fn prose_surrounding_atomic_structures_is_still_chunked_normally() {
    let mut prose_before = String::new();
    for i in 0..50 {
        prose_before.push_str(&format!("Prose paragraph {i} with some words.\n\n"));
    }
    let code = "```\nfn f() {}\n```\n\n";
    let text = format!("{prose_before}{code}More prose after the code block.");

    let (chunks, _warnings) = split_into_chunks(&text, 20);
    assert!(
        chunks.len() > 2,
        "expected multiple chunks around the atomic block"
    );
    let reassembled: String = chunks.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(reassembled, text);
}
