use ecotokens::rewrite::chunk::{effective_chunk_budget, split_into_chunks};
use ecotokens::rewrite::provider::{StubOutcome, StubProvider};
use ecotokens::rewrite::{rewrite, Mode, Origin, RewriteRequest};
use std::time::Duration;

fn multi_paragraph_text(n: usize) -> String {
    // Deliberately no standalone digit tokens: `sanitize::extract` protects
    // any `\b\d...` span with a sentinel that the `StubProvider`'s canned
    // response can't echo back, which would make `sanitize::restore` fail
    // (correctly) and turn every chunk here into a fallback. "paragraphN"
    // (digit glued to a letter, no word boundary before it) sidesteps that.
    (0..n)
        .map(|i| format!("paragraph{i} contains a handful of plain words with zero numbers in it"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Direct, in-process calls to `rewrite()` record real metrics via
/// `crate::metrics::store` if `XDG_CONFIG_HOME` resolves to the developer's
/// real config dir. The library's `#[cfg(not(test))]`/`#[cfg(test)]` split
/// does NOT protect against this: `cfg(test)` is not propagated to a crate
/// used as a dependency by an external test binary like this one — only to
/// the crate's own unit tests. `Once` makes this race-free across the
/// parallel test threads in this binary.
fn isolate_metrics_home() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let dir =
            std::env::temp_dir().join(format!("ecotokens-test-metrics-{}", std::process::id()));
        std::env::set_var("XDG_CONFIG_HOME", dir);
    });
}

fn base_request(text: String, context_tokens: u32) -> RewriteRequest {
    isolate_metrics_home();
    RewriteRequest {
        text,
        mode: Mode::Paraphrase,
        model: "stub-model".to_string(),
        timeout: Duration::from_secs(30),
        origin: Origin::Cli,
        // Deliberately tiny: the stub's canned responses are much shorter
        // than the real input paragraphs, and this test cares about chunk
        // count / ordering, not truncation detection.
        truncation_ratio: 0.01,
        context_tokens,
        save_diff: false,
        diff_dir: std::env::temp_dir(),
        diff_retention: 0,
    }
}

/// T056: any single chunk failing must fall back to the whole, original
/// document — never a partial mixture of transformed and untransformed text.
#[test]
fn any_chunk_failure_falls_back_to_the_whole_document() {
    let text = multi_paragraph_text(6);
    let context_tokens = 40; // forces multiple chunks for this input
    let budget = effective_chunk_budget(context_tokens);
    let (expected_chunks, _warnings) = split_into_chunks(&text, budget);
    assert!(
        expected_chunks.len() > 1,
        "test setup must actually produce multiple chunks"
    );

    let stub = StubProvider::new();
    for i in 0..expected_chunks.len() - 1 {
        stub.push(StubOutcome::Text(format!(
            "transformed chunk {i} with enough words to pass the truncation check comfortably"
        )));
    }
    stub.push(StubOutcome::Error(
        "simulated model failure on the last chunk".to_string(),
    ));

    let request = base_request(text.clone(), context_tokens);
    let result = rewrite(request, &stub).expect("validation must pass");

    assert_eq!(format!("{:?}", result.status), "Fallback");
    assert_eq!(
        result.text, text,
        "fallback must be byte-identical to input"
    );
    assert!(result.reason.is_some());
}

/// T057: `chunk_count` reflects the real chunk count, and every source
/// paragraph's chunk contributes exactly once to the final output (verified
/// via each chunk's distinct, order-tagged stub response appearing exactly
/// once, in order).
#[test]
fn chunk_count_is_accurate_and_every_chunk_contributes_exactly_once() {
    let text = multi_paragraph_text(8);
    let context_tokens = 40;
    let budget = effective_chunk_budget(context_tokens);
    let (expected_chunks, _warnings) = split_into_chunks(&text, budget);
    assert!(expected_chunks.len() > 1, "test setup must actually chunk");

    let stub = StubProvider::new();
    let mut expected_tags = Vec::new();
    for i in 0..expected_chunks.len() {
        let tag = format!(
            "TRANSFORMED_CHUNK_{i}_with_enough_padding_words_to_survive_the_truncation_ratio_check"
        );
        expected_tags.push(tag.clone());
        stub.push(StubOutcome::Text(tag));
    }

    let request = base_request(text, context_tokens);
    let result = rewrite(request, &stub).expect("validation must pass");

    assert_eq!(format!("{:?}", result.status), "Transformed");
    assert_eq!(result.chunk_count, expected_chunks.len());

    // Every tagged chunk response appears exactly once, in the right order.
    let mut search_from = 0;
    for tag in &expected_tags {
        let found = result.text[search_from..]
            .find(tag.as_str())
            .expect("each chunk's transformed output must appear in the final text");
        search_from += found + tag.len();
    }
    for tag in &expected_tags {
        assert_eq!(
            result.text.matches(tag.as_str()).count(),
            1,
            "chunk output must appear exactly once: {tag}"
        );
    }
}

/// Regression (found via live-model validation, T092/T094): each chunk's
/// transformed text goes through `sanitize::cleanup_response`'s trim, which
/// strips the paragraph separator that chunking had deliberately attached to
/// the end of the *source* chunk. Reassembly must reattach it — otherwise
/// consecutive chunks glue together in the *transformed* output with zero
/// separation (e.g. "...services.Each team lead...") even though the source
/// document had a clean blank-line break there.
#[test]
fn chunk_boundaries_are_not_glued_together_in_the_transformed_output() {
    let text = multi_paragraph_text(4);
    let context_tokens = 40;
    let budget = effective_chunk_budget(context_tokens);
    let (expected_chunks, _warnings) = split_into_chunks(&text, budget);
    assert!(expected_chunks.len() > 1, "test setup must actually chunk");

    let stub = StubProvider::new();
    let mut expected_tags = Vec::new();
    for i in 0..expected_chunks.len() {
        // No trailing punctuation/whitespace of its own — if the boundary
        // separator were lost, tag i's end would run straight into tag i+1's
        // start with nothing between them.
        let tag = format!("TAG{i}withenoughpaddingwordstosurvivethetruncationratiocheck");
        expected_tags.push(tag.clone());
        stub.push(StubOutcome::Text(tag));
    }

    let request = base_request(text, context_tokens);
    let result = rewrite(request, &stub).expect("validation must pass");
    assert_eq!(format!("{:?}", result.status), "Transformed");

    for pair in expected_tags.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let a_end = result
            .text
            .find(a.as_str())
            .map(|i| i + a.len())
            .expect("tag must be present");
        let b_start = result.text[a_end..]
            .find(b.as_str())
            .map(|i| a_end + i)
            .expect("next tag must be present after this one");
        assert!(
            b_start > a_end,
            "chunk boundary must not glue '{a}' directly to '{b}': {:?}",
            &result.text[a_end.saturating_sub(5)..(b_start + 5).min(result.text.len())]
        );
        let gap = &result.text[a_end..b_start];
        assert!(
            gap.chars().all(char::is_whitespace) && !gap.is_empty(),
            "gap between chunks must be whitespace-only and non-empty, got {gap:?}"
        );
    }
}

#[test]
fn a_document_within_budget_never_enters_the_chunked_path() {
    let text = "A short document that fits in a single chunk easily.".to_string();
    let stub = StubProvider::with_response("A transformed short document.");
    let request = base_request(text, 8192);
    let result = rewrite(request, &stub).expect("validation must pass");
    assert_eq!(result.chunk_count, 1);
    assert_eq!(stub.call_count(), 1);
}
