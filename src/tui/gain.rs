use chrono::{DateTime, Utc};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Sparkline},
    Frame,
};

use crate::metrics::report::Report;
use crate::metrics::store::Interception;

/// Render the gain dashboard:
///   - top:    summary stats (interceptions, tokens, savings %, cost USD)
///   - middle: one Gauge per command family, sorted by savings desc
///   - bottom: Sparkline of tokens saved over the last 14 days
pub fn render_gain(frame: &mut Frame, area: Rect, report: &Report, items: &[Interception], last_updated: Option<&str>, by_project: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // stats
            Constraint::Min(3),    // family / project gauges
            Constraint::Length(4), // sparkline
        ])
        .split(area);

    render_stats(frame, chunks[0], report, last_updated);
    if by_project {
        render_projects(frame, chunks[1], report);
    } else {
        render_families(frame, chunks[1], report);
    }
    render_sparkline(frame, chunks[2], items);
}

// ── Stats panel ───────────────────────────────────────────────────────────────

fn render_stats(frame: &mut Frame, area: Rect, report: &Report, last_updated: Option<&str>) {
    let saved = report.total_tokens_before.saturating_sub(report.total_tokens_after);
    let text = vec![
        Line::from(vec![
            Span::styled("Interceptions: ", Style::default().fg(Color::Cyan)),
            Span::raw(format!("{}   ", report.total_interceptions)),
            Span::styled("Tokens saved: ", Style::default().fg(Color::Cyan)),
            Span::raw(format!("{saved}   ")),
            Span::styled("Savings: ", Style::default().fg(Color::Cyan)),
            Span::raw(format!("{:.1}%", report.total_savings_pct)),
        ]),
        Line::from(vec![
            Span::styled("Cost avoided: ", Style::default().fg(Color::Cyan)),
            Span::raw(format!("${:.4} USD", report.cost_avoided_usd)),
            Span::raw(format!("   (model: {})", report.model_ref)),
        ]),
    ];

    let title = match last_updated {
        Some(ts) => format!(" ecotokens gain – mis à jour {ts} UTC "),
        None => " ecotokens gain ".to_string(),
    };
    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(paragraph, area);
}

// ── Family gauges ─────────────────────────────────────────────────────────────

fn render_families(frame: &mut Frame, area: Rect, report: &Report) {
    let block = Block::default().borders(Borders::ALL).title(" By family ");

    if report.by_family.is_empty() {
        let paragraph = Paragraph::new("No data yet.").block(block);
        frame.render_widget(paragraph, area);
        return;
    }

    // Sort families by savings_pct descending
    let mut families: Vec<(&String, f32)> = report
        .by_family
        .iter()
        .map(|(k, v)| (k, v.savings_pct))
        .collect();
    families.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // One row per family
    let n = families.len() as u16;
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let row_height = (inner.height / n).max(1);
    let constraints: Vec<Constraint> = families.iter().map(|_| Constraint::Length(row_height)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (i, (name, pct)) in families.iter().enumerate() {
        if i >= rows.len() {
            break;
        }
        let ratio = (*pct as f64 / 100.0).clamp(0.0, 1.0);
        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(Color::Green))
            .label(format!("{pct:.1}%"))
            .ratio(ratio)
            .block(Block::default().title(format!(" {name} ")));
        frame.render_widget(gauge, rows[i]);
    }
}

// ── Project gauges ────────────────────────────────────────────────────────────

fn render_projects(frame: &mut Frame, area: Rect, report: &Report) {
    let block = Block::default().borders(Borders::ALL).title(" By project ");

    if report.by_project.is_empty() {
        let paragraph = Paragraph::new("No data yet.").block(block);
        frame.render_widget(paragraph, area);
        return;
    }

    let mut projects: Vec<(&String, f32)> = report
        .by_project
        .iter()
        .map(|(k, v)| {
            let pct = if v.tokens_before == 0 {
                0.0f32
            } else {
                ((1.0 - v.tokens_after as f64 / v.tokens_before as f64) * 100.0) as f32
            };
            (k, pct)
        })
        .collect();
    projects.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let n = projects.len() as u16;
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let row_height = (inner.height / n).max(1);
    let constraints: Vec<Constraint> = projects.iter().map(|_| Constraint::Length(row_height)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (i, (name, pct)) in projects.iter().enumerate() {
        if i >= rows.len() {
            break;
        }
        // Affiche uniquement le dernier segment du chemin pour garder le titre court
        let label = std::path::Path::new(name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(name);
        let ratio = (*pct as f64 / 100.0).clamp(0.0, 1.0);
        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(Color::Yellow))
            .label(format!("{pct:.1}%"))
            .ratio(ratio)
            .block(Block::default().title(format!(" {label} ")));
        frame.render_widget(gauge, rows[i]);
    }
}

// ── Sparkline (14 days) ───────────────────────────────────────────────────────

fn render_sparkline(frame: &mut Frame, area: Rect, items: &[Interception]) {
    let data = build_sparkline_data(items);

    let sparkline = Sparkline::default()
        .block(Block::default().borders(Borders::ALL).title(" Savings (14 days) "))
        .style(Style::default().fg(Color::Green))
        .data(&data);

    frame.render_widget(sparkline, area);
}

/// Bucket tokens_saved per day over the last 14 days.
fn build_sparkline_data(items: &[Interception]) -> Vec<u64> {
    let mut buckets = vec![0u64; 14];
    let now = Utc::now().date_naive();

    for item in items {
        if let Ok(ts) = DateTime::parse_from_rfc3339(&item.timestamp) {
            let date = ts.with_timezone(&Utc).date_naive();
            let diff = (now - date).num_days();
            if (0..14).contains(&diff) {
                let idx = (13 - diff) as usize; // most recent = last bucket
                let saved = (item.tokens_before as u64).saturating_sub(item.tokens_after as u64);
                buckets[idx] = buckets[idx].saturating_add(saved);
            }
        }
    }

    buckets
}
