//! `ecotokens jev` — detailed view of the Jev service usage: calls, success
//! and fallback rates, latency, tokens, cost and the recent call log.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Sparkline},
    Frame,
};

use super::gain::fmt_tok;
use crate::jev::stats::{JevSummary, Purpose};

/// Static facts about the Jev configuration, shown in the header.
#[derive(Debug, Clone, Default)]
pub struct JevStatus {
    pub enabled: bool,
    pub has_key: bool,
    pub url: Option<String>,
}

fn label(text: &str) -> Span<'static> {
    Span::styled(text.to_string(), Style::default().fg(Color::Cyan))
}

fn pct(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_jev(
    frame: &mut Frame,
    area: Rect,
    summary: &JevSummary,
    status: &JevStatus,
    period: &str,
    timestamp: Option<&str>,
    selected: Option<usize>,
    log_scroll: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(8),
            Constraint::Length(4),
        ])
        .split(area);
    render_header(frame, chunks[0], summary, status, period, timestamp);
    if summary.calls == 0 {
        let msg = Paragraph::new(vec![
            Line::from(""),
            Line::from("No Jev call recorded for this period."),
            Line::from(Span::styled(
                "Enable Jev with jev_enabled + TYPESAFE_API_KEY, or the model router.",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .block(Block::default().borders(Borders::ALL).title("Jev calls"));
        frame.render_widget(msg, chunks[1]);
    } else {
        let mid = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(chunks[1]);
        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(mid[0]);
        render_purposes(frame, left[0], summary);
        render_errors(frame, left[1], summary);
        render_log(frame, mid[1], summary, selected, log_scroll);
    }
    render_timeline(frame, chunks[2], summary);
}

fn render_header(
    frame: &mut Frame,
    area: Rect,
    s: &JevSummary,
    status: &JevStatus,
    period: &str,
    timestamp: Option<&str>,
) {
    let state = match (status.enabled, status.has_key) {
        (true, true) => Span::styled("enabled", Style::default().fg(Color::Green)),
        (true, false) => Span::styled(
            "enabled, TYPESAFE_API_KEY missing",
            Style::default().fg(Color::Yellow),
        ),
        (false, _) => Span::styled("disabled", Style::default().fg(Color::DarkGray)),
    };
    let cost = match s.cost_usd {
        Some(c) => format!("${c:.4}"),
        None => "n/a (no price configured)".to_string(),
    };
    let text = vec![
        Line::from(vec![
            label("Status: "),
            state,
            Span::raw(format!(
                "   URL: {}",
                status.url.as_deref().unwrap_or("default")
            )),
        ]),
        Line::from(vec![
            label("Calls: "),
            Span::raw(format!("{}   ", fmt_tok(s.calls))),
            label("Success: "),
            Span::raw(format!("{} ({:.1}%)   ", fmt_tok(s.ok), pct(s.ok, s.calls))),
            label("Fell back to heuristic: "),
            Span::raw(format!(
                "{} ({:.1}%)",
                fmt_tok(s.fallbacks),
                pct(s.fallbacks, s.calls)
            )),
        ]),
        Line::from(vec![
            label("Latency avg: "),
            Span::raw(format!("{} ms   ", s.avg_latency_ms)),
            label("p95: "),
            Span::raw(format!("{} ms", s.p95_latency_ms)),
        ]),
        Line::from(vec![
            label("Tokens in: "),
            Span::raw(format!("{}   ", fmt_tok(s.input_tokens))),
            label("out: "),
            Span::raw(format!("{}   ", fmt_tok(s.output_tokens))),
            label("Cost: "),
            Span::raw(cost),
        ]),
    ];
    let mut title = format!("ecotokens jev ({period})");
    if let Some(ts) = timestamp {
        title.push_str(&format!(" — {ts}"));
    }
    title.push_str(" — q quit, j/k select, o/l scroll");
    frame.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}

fn render_purposes(frame: &mut Frame, area: Rect, s: &JevSummary) {
    let block = Block::default().borders(Borders::ALL).title("By purpose");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let rows: Vec<Purpose> = Purpose::ALL
        .into_iter()
        .filter(|p| s.by_purpose.get(p.as_str()).is_some_and(|st| st.calls > 0))
        .collect();
    for (i, p) in rows.iter().enumerate() {
        if i as u16 >= inner.height {
            break;
        }
        let st = &s.by_purpose[p.as_str()];
        let row = Rect {
            x: inner.x,
            y: inner.y + i as u16,
            width: inner.width,
            height: 1,
        };
        let text = format!(
            "{:<13} {:>5} calls  ok {:>3.0}%  {:>5} ms  {}/{} tok",
            p.as_str(),
            st.calls,
            pct(st.ok, st.calls),
            st.avg_latency_ms,
            fmt_tok(st.input_tokens),
            fmt_tok(st.output_tokens),
        );
        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(Color::Green).bg(Color::Black))
            .ratio((st.calls as f64 / s.calls.max(1) as f64).clamp(0.0, 1.0))
            .label(text);
        frame.render_widget(gauge, row);
    }
}

fn render_errors(frame: &mut Frame, area: Rect, s: &JevSummary) {
    let mut lines = Vec::new();
    if s.errors.is_empty() {
        lines.push(Line::from(Span::styled(
            "No failure.",
            Style::default().fg(Color::Green),
        )));
    } else {
        let mut errs: Vec<_> = s.errors.iter().collect();
        errs.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for (kind, n) in errs {
            lines.push(Line::from(vec![
                Span::styled(format!("{n:>5}  "), Style::default().fg(Color::Red)),
                Span::raw(kind.clone()),
            ]));
        }
    }
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Failures")),
        area,
    );
}

fn render_log(
    frame: &mut Frame,
    area: Rect,
    s: &JevSummary,
    selected: Option<usize>,
    scroll: usize,
) {
    let height = area.height.saturating_sub(2) as usize;
    // Keep the selected row visible whatever the manual scroll is.
    let mut start = scroll.min(s.recent.len().saturating_sub(1));
    if let Some(sel) = selected {
        if sel < start {
            start = sel;
        } else if height > 0 && sel >= start + height {
            start = sel + 1 - height;
        }
    }
    let lines: Vec<Line> = s
        .recent
        .iter()
        .enumerate()
        .skip(start)
        .take(height)
        .map(|(i, c)| {
            let time = c.timestamp.get(11..19).unwrap_or(&c.timestamp);
            let (mark, color) = if c.ok {
                ("ok  ", Color::Green)
            } else {
                ("FAIL", Color::Red)
            };
            let mut spans = vec![
                Span::raw(format!("{time} ")),
                Span::styled(mark, Style::default().fg(color)),
                Span::raw(format!(
                    " {:<12} {:>5} ms {:>6}/{:<5}",
                    c.purpose,
                    c.latency_ms,
                    fmt_tok(c.input_tokens),
                    fmt_tok(c.output_tokens)
                )),
            ];
            if let Some(agent) = &c.agent {
                spans.push(Span::styled(
                    format!(" -> {agent}"),
                    Style::default().fg(Color::Cyan),
                ));
            }
            if let Some(kind) = &c.error_kind {
                let code = c.http_status.map(|h| format!(" {h}")).unwrap_or_default();
                spans.push(Span::styled(
                    format!(" {kind}{code}"),
                    Style::default().fg(Color::Red),
                ));
            }
            let mut line = Line::from(spans);
            if selected == Some(i) {
                line = line.style(Style::default().add_modifier(Modifier::REVERSED));
            }
            line
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Recent calls (newest first)"),
        ),
        area,
    );
}

fn render_timeline(frame: &mut Frame, area: Rect, s: &JevSummary) {
    let data: Vec<u64> = if s.timeline.is_empty() {
        vec![0]
    } else {
        s.timeline.clone()
    };
    frame.render_widget(
        Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Calls over time"),
            )
            .data(&data)
            .style(Style::default().fg(Color::Cyan)),
        area,
    );
}

pub fn status_from_settings(settings: &crate::config::Settings) -> JevStatus {
    JevStatus {
        enabled: settings.jev_enabled || settings.router_enabled,
        has_key: std::env::var(crate::jev::API_KEY_ENV)
            .map(|k| !k.trim().is_empty())
            .unwrap_or(false),
        url: settings.jev_url.clone(),
    }
}

/// Reads the Jev call log for `period`. An unreadable log reads as empty.
pub fn load_summary(
    settings: &crate::config::Settings,
    period: &crate::metrics::report::Period,
) -> JevSummary {
    let since = crate::metrics::report::period_start(period);
    match crate::jev::stats::db_path() {
        Some(path) => crate::jev::stats::summarize(&path, settings, since).unwrap_or_default(),
        None => JevSummary::default(),
    }
}

/// Interactive loop: returns when the user presses q / Esc / v / Ctrl-C.
pub fn run<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    period: &crate::metrics::report::Period,
) -> bool {
    use ratatui::crossterm::event::{poll, read, Event, KeyCode, KeyEventKind, KeyModifiers};

    let settings = crate::config::Settings::load();
    let status = status_from_settings(&settings);
    let mut summary = load_summary(&settings, period);
    let mut last_reload = std::time::Instant::now();
    let mut selected: Option<usize> = None;
    let mut scroll = 0usize;
    loop {
        if last_reload.elapsed() >= std::time::Duration::from_secs(10) {
            summary = load_summary(&settings, period);
            last_reload = std::time::Instant::now();
        }
        let ts = chrono::Utc::now().format("%H:%M:%S").to_string();
        let _ = terminal.draw(|f| {
            render_jev(
                f,
                f.area(),
                &summary,
                &status,
                &period.to_string(),
                Some(&ts),
                selected,
                scroll,
            )
        });
        if !poll(std::time::Duration::from_millis(500)).unwrap_or(false) {
            continue;
        }
        let Ok(Event::Key(key)) = read() else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        let last = summary.recent.len().saturating_sub(1);
        match key.code {
            KeyCode::Char('v') => return true,
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => return false,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return false,
            KeyCode::Char('j') | KeyCode::Down => {
                selected = Some(selected.map_or(0, |i| (i + 1).min(last)));
            }
            KeyCode::Char('k') | KeyCode::Up => {
                selected = Some(selected.map_or(0, |i| i.saturating_sub(1)));
            }
            KeyCode::Char('l') => scroll = (scroll + 1).min(last),
            KeyCode::Char('o') => scroll = scroll.saturating_sub(1),
            _ => {}
        }
    }
}
