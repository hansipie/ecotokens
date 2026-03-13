use ecotokens::trace::CallEdge;
use ecotokens::tui::trace::render_trace;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

mod helpers;
use helpers::buffer_text;

// ── T058at — TUI trace tests ────────────────────────────────────────────────

#[test]
fn render_trace_shows_columns() {
    let edges = vec![
        CallEdge {
            symbol_id: "main.rs::main#fn".to_string(),
            name: "main".to_string(),
            file_path: "main.rs".to_string(),
            line: 3,
        },
        CallEdge {
            symbol_id: "orchestrator.rs::orchestrator#fn".to_string(),
            name: "orchestrator".to_string(),
            file_path: "orchestrator.rs".to_string(),
            line: 2,
        },
    ];

    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            render_trace(f, f.area(), &edges, "helper", "callers");
        })
        .unwrap();

    let content = buffer_text(&terminal);
    assert!(
        content.contains("main"),
        "should display caller name 'main', got: {content:?}"
    );
    assert!(
        content.contains("orchestrator"),
        "should display caller name 'orchestrator'"
    );
    assert!(content.contains("main.rs"), "should display file path");
}

#[test]
fn render_trace_empty_shows_message() {
    let edges: Vec<CallEdge> = vec![];

    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            render_trace(f, f.area(), &edges, "ghost", "callers");
        })
        .unwrap();

    let content = buffer_text(&terminal);
    assert!(
        content.contains("No") || content.contains("no") || content.contains("empty"),
        "empty edges should show 'No callers found' or similar message, got: {content:?}"
    );
}
