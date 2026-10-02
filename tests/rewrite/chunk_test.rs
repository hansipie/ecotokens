use ecotokens::rewrite::chunk::split_into_chunks;

fn reassembled(text: &str, budget: u32) -> String {
    let (chunks, _warnings) = split_into_chunks(text, budget);
    chunks.iter().map(|c| c.text.as_str()).collect::<String>()
}

// ── T052: byte-for-byte reassembly, the strongest invariant in the feature ──

#[test]
fn reassembly_is_byte_exact_for_plain_prose() {
    let text = "This is a simple paragraph of prose with several sentences. \
        It has no special structure at all, just plain words.";
    assert_eq!(reassembled(text, 10), text);
}

#[test]
fn reassembly_is_byte_exact_across_paragraphs() {
    let text = "Paragraph one has some words in it.\n\nParagraph two follows after a blank line.\n\nAnd a third paragraph here.";
    assert_eq!(reassembled(text, 5), text);
}

#[test]
fn reassembly_preserves_multiple_consecutive_blank_lines() {
    let text = "First paragraph.\n\n\n\nSecond paragraph after three blank lines.";
    assert_eq!(reassembled(text, 3), text);
}

#[test]
fn reassembly_preserves_trailing_newline() {
    let text = "A short paragraph.\n";
    assert_eq!(reassembled(text, 10), text);
}

#[test]
fn reassembly_preserves_absence_of_trailing_newline() {
    let text = "A short paragraph with no trailing newline";
    assert_eq!(reassembled(text, 10), text);
}

#[test]
fn reassembly_preserves_leading_whitespace() {
    let text = "   Leading spaces before the first word.\n\nAnd another paragraph.";
    assert_eq!(reassembled(text, 5), text);
}

#[test]
fn reassembly_is_byte_exact_with_unicode() {
    let text = "Café résumé naïve.\n\nDeuxième paragraphe avec des accents : é è ê ë. 日本語のテキストも。";
    assert_eq!(reassembled(text, 4), text);
}

#[test]
fn reassembly_is_byte_exact_for_a_large_multi_paragraph_document() {
    let mut text = String::new();
    for i in 0..200 {
        text.push_str(&format!(
            "This is paragraph number {i} with a handful of words in it to give it some bulk.\n\n"
        ));
    }
    let (chunks, _warnings) = split_into_chunks(&text, 100);
    assert!(chunks.len() > 1, "expected the document to actually split");
    let reassembled: String = chunks.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(reassembled, text);
}

#[test]
fn reassembly_is_byte_exact_when_input_fits_in_a_single_chunk() {
    let text = "Short enough to be a single chunk.";
    let (chunks, _warnings) = split_into_chunks(text, 10_000);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].text, text);
}

#[test]
fn empty_input_produces_no_chunks() {
    let (chunks, warnings) = split_into_chunks("", 100);
    assert!(chunks.is_empty());
    assert!(warnings.is_empty());
}

// ── T053: boundary selection tiers ──

#[test]
fn splits_on_paragraph_boundaries_when_the_whole_exceeds_budget() {
    let text = "Paragraph one.\n\nParagraph two.\n\nParagraph three.";
    let (chunks, _warnings) = split_into_chunks(text, 3);
    assert!(chunks.len() > 1);
    // Every chunk individually should be within (or close to) budget for
    // this input, since no single paragraph alone exceeds it.
    for c in &chunks {
        assert!(!c.atomic);
    }
}

#[test]
fn falls_back_to_sentence_boundaries_for_an_oversized_paragraph() {
    let long_paragraph = "Sentence one is here. Sentence two follows right after. \
        Sentence three continues the thought. Sentence four keeps going. \
        Sentence five wraps up this oversized paragraph nicely.";
    // Budget small enough that the paragraph as a whole doesn't fit, but each
    // sentence does.
    let (chunks, warnings) = split_into_chunks(long_paragraph, 15);
    assert!(chunks.len() > 1);
    assert!(
        warnings.is_empty(),
        "sentence tier should not need a warning"
    );
    let reassembled: String = chunks.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(reassembled, long_paragraph);
}

#[test]
fn falls_back_to_whitespace_boundaries_for_an_oversized_sentence_and_warns() {
    // One giant sentence with no internal punctuation to split on.
    let long_sentence = (0..200)
        .map(|i| format!("word{i}"))
        .collect::<Vec<_>>()
        .join(" ")
        + ".";
    let (chunks, warnings) = split_into_chunks(&long_sentence, 5);
    assert!(chunks.len() > 1);
    assert!(
        !warnings.is_empty(),
        "whitespace fallback must record a warning"
    );
    let reassembled: String = chunks.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(reassembled, long_sentence);
}
