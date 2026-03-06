use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, Row, Table};
use ratatui::Frame;

use crate::trace::CallEdge;

/// Render a trace table (callers or callees) in a ratatui frame.
pub fn render_trace(
    frame: &mut Frame,
    area: Rect,
    edges: &[CallEdge],
    symbol_name: &str,
    direction: &str,
) {
    let title = format!(" {direction} of {symbol_name} ");
    let block = Block::default().borders(Borders::ALL).title(title);

    if edges.is_empty() {
        let msg = format!("No {direction} found for {symbol_name}");
        let list = List::new(vec![ListItem::new(msg)])
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(list, area);
        return;
    }

    let rows: Vec<Row> = edges
        .iter()
        .map(|e| {
            Row::new(vec![
                e.name.clone(),
                e.file_path.clone(),
                e.line.to_string(),
            ])
        })
        .collect();

    let widths = [
        ratatui::layout::Constraint::Percentage(30),
        ratatui::layout::Constraint::Percentage(50),
        ratatui::layout::Constraint::Percentage(20),
    ];

    let table = Table::new(rows, widths)
        .header(
            Row::new(vec!["Name", "File", "Line"])
                .style(Style::default().fg(Color::Cyan)),
        )
        .block(block);

    frame.render_widget(table, area);
}
