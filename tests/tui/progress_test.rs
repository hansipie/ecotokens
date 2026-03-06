use ecotokens::search::index::{index_directory, IndexOptions};
use ecotokens::tui::progress::render_progress;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::fs;
use tempfile::TempDir;

fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}

// ── T052t ─────────────────────────────────────────────────────────────────────

#[test]
fn progress_bar_renders_percentage() {
    let backend = TestBackend::new(80, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_progress(frame, frame.area(), 42, 100, "Indexing…"))
        .unwrap();
    let content = buffer_text(&terminal);
    assert!(
        content.contains("42") || content.contains("%"),
        "buffer should show progress percentage: {content:?}"
    );
}

#[test]
fn progress_bar_full_shows_100() {
    let backend = TestBackend::new(80, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_progress(frame, frame.area(), 100, 100, "Done"))
        .unwrap();
    let content = buffer_text(&terminal);
    assert!(
        content.contains("100"),
        "full progress should show 100: {content:?}"
    );
}

#[test]
fn index_directory_produces_bm25_entries() {
    let src = TempDir::new().unwrap();
    let idx = TempDir::new().unwrap();
    fs::write(src.path().join("main.rs"), "fn main() { println!(\"hello\"); }\n").unwrap();
    fs::write(src.path().join("README.md"), "# Hello\n\nThis project does things.\n").unwrap();
    let opts = IndexOptions {
        reset: true,
        path: src.path().to_path_buf(),
        index_dir: idx.path().to_path_buf(),
    };
    let stats = index_directory(opts).unwrap();
    assert!(stats.file_count >= 1, "should index at least 1 file");
    assert!(stats.chunk_count > 0, "should produce BM25 chunks");
}
