use crate::daemon::watcher::WatchEvent;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

/// Affiche le panneau de surveillance :
///   - header  : chemin surveillé
///   - liste   : événements les plus récents en premier
pub fn render_watch(frame: &mut Frame, area: Rect, events: &[WatchEvent], watch_path: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    // Header
    let header = Paragraph::new(format!("Watching: {watch_path}"))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" ecotokens watch — q/Esc pour quitter"),
        )
        .style(Style::default().fg(Color::Green));
    frame.render_widget(header, chunks[0]);

    // Liste des événements
    if events.is_empty() {
        let empty = Paragraph::new("Aucun événement — en attente de modifications…")
            .block(Block::default().borders(Borders::ALL).title(" Événements"))
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, chunks[1]);
        return;
    }

    let visible = chunks[1].height.saturating_sub(2) as usize; // -2 pour les bordures
    let items: Vec<ListItem> = events
        .iter()
        .rev()
        .take(visible.max(1))
        .map(|e| {
            let color = if e.status == "re-indexed" {
                Color::Green
            } else if e.status.starts_with("error") {
                Color::Red
            } else {
                Color::Yellow // "ignored"
            };

            let file_name = e
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("[{}] ", e.timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(format!("{file_name:<30} ")),
                Span::styled(e.status.clone(), Style::default().fg(color)),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Événements ({})", events.len())),
    );
    frame.render_widget(list, chunks[1]);
}
