use crate::daemon::watcher::WatchEvent;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub enum WatchPhase {
    Indexing,
    Watching,
}

pub struct IndexReport {
    pub file_count: u32,
    pub chunk_count: u32,
    pub elapsed_secs: f64,
}

pub struct WatchStats {
    pub reindexed: u32,
    pub ignored: u32,
    pub errors: u32,
}

/// Phase rail: [✓ Indexing] -> [● Watching] with an inline report summary.
fn render_phase_rail(
    frame: &mut Frame,
    area: Rect,
    phase: &WatchPhase,
    report: Option<&IndexReport>,
) {
    let mut spans = Vec::new();

    match phase {
        WatchPhase::Indexing => {
            spans.push(Span::styled(
                "[● Indexing]",
                Style::default().fg(Color::Blue),
            ));
            spans.push(Span::raw(" → "));
            spans.push(Span::styled(
                "[ Watching]",
                Style::default().fg(Color::DarkGray),
            ));
        }
        WatchPhase::Watching => {
            spans.push(Span::styled(
                "[✓ Indexing]",
                Style::default().fg(Color::Green),
            ));
            spans.push(Span::raw(" → "));
            spans.push(Span::styled(
                "[● Watching]",
                Style::default().fg(Color::Blue),
            ));
            if let Some(r) = report {
                spans.push(Span::raw("    "));
                spans.push(Span::styled(
                    format!(
                        "{} files · {} chunks · {:.1}s",
                        r.file_count, r.chunk_count, r.elapsed_secs
                    ),
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }
    }

    let rail = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" ecotokens watch "),
    );
    frame.render_widget(rail, area);
}

/// Full rendering during the initial indexing phase.
pub fn render_indexing(frame: &mut Frame, area: Rect, done: u64, total: u64, logs: &[String]) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    render_phase_rail(frame, chunks[0], &WatchPhase::Indexing, None);
    crate::tui::progress::render_progress(frame, chunks[1], done, total, " Initial indexing... ");

    let log_area = chunks[2];
    if logs.is_empty() {
        let placeholder = Paragraph::new("Indexing in progress — watching will start next...")
            .block(Block::default().borders(Borders::ALL).title(" Log "))
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(placeholder, log_area);
    } else {
        let visible = log_area.height.saturating_sub(2) as usize;
        let items: Vec<ListItem> = logs
            .iter()
            .rev()
            .take(visible.max(1))
            .rev()
            .map(|msg| {
                let color = if msg.contains("warning") || msg.contains("Skipping") {
                    Color::Yellow
                } else {
                    Color::DarkGray
                };
                ListItem::new(Line::from(Span::styled(
                    msg.clone(),
                    Style::default().fg(color),
                )))
            })
            .collect();
        let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Log "));
        frame.render_widget(list, log_area);
    }

    let help =
        Paragraph::new(" q/Esc: quit  Ctrl-C: stop").style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, chunks[3]);
}

/// Full rendering during the watching phase.
pub fn render_watch(
    frame: &mut Frame,
    area: Rect,
    events: &[WatchEvent],
    watch_path: &str,
    report: Option<&IndexReport>,
    stats: &WatchStats,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // phase rail + report
            Constraint::Length(3), // stats
            Constraint::Min(1),    // event list
            Constraint::Length(1), // barre d'aide
        ])
        .split(area);

    render_phase_rail(frame, chunks[0], &WatchPhase::Watching, report);

    // Panneau stats
    let stats_text = format!(
        "  {}   re-indexed: {}  ignored: {}  errors: {}",
        watch_path, stats.reindexed, stats.ignored, stats.errors
    );
    let stats_widget = Paragraph::new(stats_text)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(stats_widget, chunks[1]);

    // Titre de la liste avec compteurs
    let events_title = format!(
        " Events - re-indexed:{}  ignored:{}  errors:{} ",
        stats.reindexed, stats.ignored, stats.errors
    );

    if events.is_empty() {
        let empty = Paragraph::new("No events - waiting for file changes...")
            .block(Block::default().borders(Borders::ALL).title(events_title))
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, chunks[2]);
    } else {
        let visible = chunks[2].height.saturating_sub(2) as usize;
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
                    Color::Yellow
                };

                let path_str = e.path.to_string_lossy().into_owned();
                let chars: Vec<char> = path_str.chars().collect();
                let display_path = if chars.len() > 50 {
                    let truncated: String = chars[chars.len() - 49..].iter().collect();
                    format!("…{truncated}")
                } else {
                    path_str
                };

                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("[{}] ", e.timestamp),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(format!("{display_path:<50} ")),
                    Span::styled(e.status.clone(), Style::default().fg(color)),
                ]))
            })
            .collect();

        let list =
            List::new(items).block(Block::default().borders(Borders::ALL).title(events_title));
        frame.render_widget(list, chunks[2]);
    }

    // Barre d'aide
    let help =
        Paragraph::new(" q/Esc: quit  Ctrl-C: stop").style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, chunks[3]);
}
