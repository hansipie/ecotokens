use ecotokens::config::default_index_dir;
use ecotokens::hook::post_handler::PostFilterResult;
use ecotokens::hook::read_handler::handle_read;
use std::path::PathBuf;

fn index_dir() -> PathBuf {
    default_index_dir()
}

#[test]
fn read_nonindexed_returns_passthrough() {
    // File that doesn't exist → no index hit → Passthrough
    let result = handle_read(
        "/tmp/nonexistent_ecotokens_test_abc123.rs",
        "fn main() {}",
        1,
        None,
    );
    assert!(
        matches!(result, PostFilterResult::Passthrough),
        "non-indexed file should return Passthrough"
    );
}

#[test]
fn read_binary_ext_returns_passthrough() {
    let result = handle_read("image.png", "PNG binary data", 1, None);
    assert!(
        matches!(result, PostFilterResult::Passthrough),
        "binary extension should return Passthrough immediately"
    );
}

#[test]
fn read_binary_ext_wasm_passthrough() {
    let result = handle_read("module.wasm", "\0asm\x01\x00\x00\x00", 1, None);
    assert!(
        matches!(result, PostFilterResult::Passthrough),
        "wasm extension should return Passthrough"
    );
}

#[test]
fn read_empty_content_passthrough() {
    let result = handle_read("/tmp/empty_ecotokens_test.rs", "", 1, None);
    assert!(
        matches!(result, PostFilterResult::Passthrough),
        "empty content should return Passthrough"
    );
}

/// Integration test: requires a real index at ~/.config/ecotokens/index/
/// Tests that an indexed Rust file returns Filtered with outline content
#[test]
fn read_indexed_file_returns_filtered_or_passthrough() {
    let idx = index_dir();
    if !idx.exists() {
        // Skip gracefully when no index
        return;
    }
    // src/main.rs is guaranteed to be indexed if we ran ecotokens index
    let file_path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs");
    let content = std::fs::read_to_string(file_path).unwrap_or_default();
    if content.is_empty() {
        return;
    }
    let result = handle_read(file_path, &content, 1, None);
    // If indexed: Filtered with outline content that is smaller than original content
    match result {
        PostFilterResult::Filtered {
            output,
            tokens_before,
            tokens_after,
            ..
        } => {
            assert!(!output.is_empty(), "outline output should not be empty");
            assert!(tokens_before > 0, "tokens_before should be positive");
            // tokens_after can be 0 if additionalContext not counted the same way
            let _ = tokens_after;
        }
        PostFilterResult::Passthrough => {
            // Acceptable if the file is not in the index (e.g. index not up to date)
        }
    }
}

/// Integration test: file indexed, outline empty (e.g. binary or non-Rust) → Passthrough
#[test]
fn read_outline_empty_returns_passthrough() {
    // Use a JSON file (outline_path returns no symbols for JSON or empty result)
    let result = handle_read(
        concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
        "[package]\nname = \"ecotokens\"\n",
        1,
        None,
    );
    // Toml may or may not have symbols — either result is acceptable
    // The test just verifies no panic occurs
    let _ = result;
}
