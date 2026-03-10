use ecotokens::daemon::watcher::WatchEvent;
use ecotokens::tui::watch::{render_watch, WatchStats};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::path::PathBuf;

fn empty_stats() -> WatchStats {
    WatchStats {
        reindexed: 0,
        ignored: 0,
        errors: 0,
    }
}

/// Vérifie que le header "ecotokens watch" est présent dans le rendu.
#[test]
fn test_render_watch_contains_header() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    let events: Vec<WatchEvent> = vec![];

    terminal
        .draw(|f| {
            render_watch(f, f.area(), &events, "/some/path", None, &empty_stats());
        })
        .unwrap();

    let content = terminal.backend().to_string();
    assert!(
        content.contains("watch"),
        "header 'watch' absent du rendu : {content}"
    );
}

/// Vérifie qu'un événement est affiché dans la liste.
#[test]
fn test_render_watch_shows_event() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = vec![WatchEvent {
        path: PathBuf::from("src/main.rs"),
        timestamp: "12:00:00".to_string(),
        status: "re-indexed".to_string(),
    }];

    terminal
        .draw(|f| {
            render_watch(f, f.area(), &events, "/some/path", None, &empty_stats());
        })
        .unwrap();

    let content = terminal.backend().to_string();
    assert!(
        content.contains("main.rs") || content.contains("re-indexed"),
        "événement absent du rendu : {content}"
    );
}

/// Vérifie qu'un rendu sans événements ne panique pas.
#[test]
fn test_render_watch_empty_no_panic() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    let events: Vec<WatchEvent> = vec![];
    terminal
        .draw(|f| {
            render_watch(f, f.area(), &events, "/path", None, &empty_stats());
        })
        .unwrap();
}

/// Vérifie que plusieurs événements sont affichés sans panique.
#[test]
fn test_render_watch_multiple_events() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let events: Vec<WatchEvent> = (0..10)
        .map(|i| WatchEvent {
            path: PathBuf::from(format!("src/file{i}.rs")),
            timestamp: format!("12:00:{i:02}"),
            status: "re-indexed".to_string(),
        })
        .collect();

    terminal
        .draw(|f| {
            render_watch(f, f.area(), &events, "/project", None, &empty_stats());
        })
        .unwrap();
}

/// Vérifie que le statut "error" est affiché différemment (sans panique).
#[test]
fn test_render_watch_error_status() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = vec![WatchEvent {
        path: PathBuf::from("src/broken.rs"),
        timestamp: "09:00:00".to_string(),
        status: "error: permission denied".to_string(),
    }];

    terminal
        .draw(|f| {
            render_watch(f, f.area(), &events, "/project", None, &empty_stats());
        })
        .unwrap();

    let content = terminal.backend().to_string();
    assert!(
        content.contains("broken.rs") || content.contains("error"),
        "événement error absent : {content}"
    );
}
