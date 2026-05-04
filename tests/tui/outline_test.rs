use ecotokens::search::symbols::Symbol;
use ecotokens::tui::outline::render_outline;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn make_symbol(name: &str, line: u64) -> Symbol {
    Symbol {
        id: format!("lib.rs::{name}#fn"),
        name: name.to_string(),
        kind: "fn".to_string(),
        file_path: "lib.rs".to_string(),
        line_start: line,
        line_end: line,
        source: format!("fn {name}() {{}}"),
    }
}

mod helpers;
use helpers::buffer_text;

// ── T051at ────────────────────────────────────────────────────────────────────

#[test]
fn outline_renders_symbol_names() {
    let symbols = vec![make_symbol("authenticate", 0), make_symbol("logout", 5)];
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_outline(frame, frame.area(), &symbols, 0))
        .unwrap();
    let content = buffer_text(&terminal);
    assert!(
        content.contains("authenticate"),
        "buffer should contain 'authenticate': {content:?}"
    );
}

#[test]
fn outline_empty_shows_no_symbols_found() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_outline(frame, frame.area(), &[], 0))
        .unwrap();
    let content = buffer_text(&terminal);
    assert!(
        content.contains("No symbols found"),
        "empty list should show 'No symbols found': {content:?}"
    );
}

#[test]
fn outline_navigation_does_not_panic() {
    let symbols: Vec<Symbol> = (0..5).map(|i| make_symbol(&format!("fn_{i}"), i)).collect();
    let mut selected = 0usize;
    // Simulate ↓ x3 then ↑ x2 — no panic expected
    for _ in 0..3 {
        selected = (selected + 1).min(symbols.len().saturating_sub(1));
    }
    for _ in 0..2 {
        selected = selected.saturating_sub(1);
    }
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_outline(frame, frame.area(), &symbols, selected))
        .unwrap();
    // No panic = success
}
