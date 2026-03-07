use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
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
        .map(|s| {
            let line = Line::from(vec![
                Span::styled(format!("{:<8}", s.kind), Style::default().fg(Color::Cyan)),
                Span::raw(format!("{} ", s.name)),
                Span::styled(
                    format!("{}:{}", s.file_path, s.line_start),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    // highlight_style is applied by ratatui to the entire selected row,
    // which avoids the partial-highlight issue of manual per-span styling.
    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    // ListState drives automatic scrolling so the selected item stays visible.
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}
