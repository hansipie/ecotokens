use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::search::symbols::Symbol;

/// Render a scrollable list of symbols into the given area.
/// `selected` is the currently highlighted index (0-based).
pub fn render_outline(frame: &mut Frame, area: Rect, symbols: &[Symbol], selected: usize) {
    let block = Block::default().borders(Borders::ALL).title(" Outline ");

    if symbols.is_empty() {
        let list = List::new(vec![ListItem::new("No symbols found")])
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(list, area);
        return;
    }

    let items: Vec<ListItem> = symbols
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let base = if i == selected {
                Style::default()
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let line = Line::from(vec![
                Span::styled(format!("{:<8}", s.kind), Style::default().fg(Color::Cyan)),
                Span::styled(format!("{} ", s.name), base),
                Span::styled(
                    format!("{}:{}", s.file_path, s.line_start),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}
