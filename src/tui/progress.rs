use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Gauge};
use ratatui::Frame;

/// Render a progress bar showing `done` out of `total` items.
/// `label` appears as the gauge title.
pub fn render_progress(frame: &mut Frame, area: Rect, done: u64, total: u64, label: &str) {
    let ratio = if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0)
    };
    let pct = ratio * 100.0;

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(label))
        .gauge_style(Style::default().fg(Color::Green))
        .ratio(ratio)
        .label(Span::raw(format!("{pct:.1}%")));

    frame.render_widget(gauge, area);
}
