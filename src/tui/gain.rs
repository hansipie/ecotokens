use chrono::{DateTime, Utc};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Sparkline, Wrap},
    Frame,
};
use similar::{ChangeTag, TextDiff};

use crate::metrics::report::Report;
use crate::metrics::store::Interception;

#[derive(Clone, Copy, Default)]
pub enum SparklineMode {
    #[default]
    Linear,
    Log,
    Capped,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum DetailMode {
    #[default]
    Split,
    Diff,
    Log,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum GainMode {
    #[default]
    Family,
    Project,
}

impl GainMode {
    pub fn toggle(self) -> Self {
        match self {
            GainMode::Family => GainMode::Project,
            GainMode::Project => GainMode::Family,
        }
    }
}

impl DetailMode {
    pub fn toggle(self) -> Self {
        match self {
            DetailMode::Split => DetailMode::Diff,
            DetailMode::Diff => DetailMode::Log,
            DetailMode::Log => DetailMode::Split,
        }
    }
}

impl SparklineMode {
    pub fn next(self) -> Self {
        match self {
            SparklineMode::Linear => SparklineMode::Log,
            SparklineMode::Log => SparklineMode::Capped,
            SparklineMode::Capped => SparklineMode::Linear,
        }
    }

    fn label(self) -> &'static str {
        match self {
            SparklineMode::Linear => "linear",
            SparklineMode::Log => "log",
            SparklineMode::Capped => "capped",
        }
    }
}

fn project_label(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return "(unknown project)".to_string();
    }
    let base = std::path::Path::new(trimmed)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(trimmed)
        .trim();
    if base.is_empty() {
        trimmed.to_string()
    } else {
        base.to_string()
    }
}

/// Returns true when an interception belongs to `project`.
/// `"(unknown)"` matches items with a blank or absent `git_root`.
fn matches_project(item: &Interception, project: &str) -> bool {
    if project == "(unknown)" {
        item.git_root
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
    } else {
        item.git_root.as_deref() == Some(project)
    }
}

/// Returns family names (sorted by descending savings) for a given project.
pub fn sorted_family_keys_for_project(items: &[Interception], project: &str) -> Vec<String> {
    use std::collections::HashMap;
    let mut map: HashMap<String, (u64, u64)> = HashMap::new();
    for item in items.iter().filter(|i| matches_project(i, project)) {
        if let Some(family) = serde_json::to_value(&item.command_family)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
        {
            let entry = map.entry(family).or_insert((0, 0));
            entry.0 += item.tokens_before as u64;
            entry.1 += item.tokens_after as u64;
        }
    }
    let mut families: Vec<(String, f32)> = map
        .into_iter()
        .map(|(k, (before, after))| {
            let pct = if before == 0 {
                0.0f32
            } else {
                ((1.0 - after as f64 / before as f64) * 100.0) as f32
            };
            (k, pct)
        })
        .collect();
    families.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    families.into_iter().map(|(k, _)| k).collect()
}

/// Render the gain dashboard:
///   - top:    summary stats (interceptions, tokens, savings %, cost USD)
///   - middle: one Gauge per command family, sorted by savings desc
///   - detail: last interception details for selected family (command, diff, or log)
///   - bottom: Sparkline of tokens saved (adaptive width — one column per day)
#[allow(clippy::too_many_arguments)]
pub fn render_gain(
    frame: &mut Frame,
    area: Rect,
    report: &Report,
    items: &[Interception],
    last_updated: Option<&str>,
    gain_mode: GainMode,
    sparkline_mode: SparklineMode,
    selected_family: Option<usize>,
    detail_mode: DetailMode,
    selected_project: Option<usize>,
    project_filter: Option<&str>,
    history_scroll: &mut usize,
) {
    // Outer layout: stats | pool(family+detail) | sparkline
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // stats
            Constraint::Min(7),    // family + detail pool
            Constraint::Length(4), // sparkline
        ])
        .split(area);

    render_stats(frame, outer[0], report, items, last_updated);

    if gain_mode == GainMode::Project {
        let pool = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(outer[1]);
        let project_names = render_projects(frame, pool[0], report, selected_project);
        let sel_proj = selected_project
            .and_then(|i| project_names.get(i))
            .map(String::as_str);
        render_project_log_panel(frame, pool[1], sel_proj, items, history_scroll);
        render_sparkline(frame, outer[2], items, sparkline_mode);
        return;
    }

    // GainMode::Family — pool split family/detail
    let pool = if detail_mode == DetailMode::Diff || detail_mode == DetailMode::Log {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(outer[1])
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(6)])
            .split(outer[1])
    };

    let filtered_items: Vec<Interception>;
    let display_items: &[Interception] = if let Some(proj) = project_filter {
        filtered_items = items
            .iter()
            .filter(|i| matches_project(i, proj))
            .cloned()
            .collect();
        &filtered_items
    } else {
        items
    };
    let family_names = render_families(
        frame,
        pool[0],
        report,
        display_items,
        selected_family,
        project_filter,
    );
    let sel_name = selected_family
        .and_then(|i| family_names.get(i))
        .map(String::as_str);
    render_detail(
        frame,
        pool[1],
        sel_name,
        display_items,
        detail_mode,
        history_scroll,
    );
    render_sparkline(frame, outer[2], items, sparkline_mode);
}

// ── Stats panel ───────────────────────────────────────────────────────────────

fn since_days(report: &Report, items: &[Interception]) -> i64 {
    match report.period.as_str() {
        "today" => 1,
        "week" => 7,
        "month" => 30,
        _ => {
            let now = Utc::now().date_naive();
            let oldest = items
                .iter()
                .filter_map(|item| DateTime::parse_from_rfc3339(&item.timestamp).ok())
                .map(|ts| ts.with_timezone(&Utc).date_naive())
                .min();

            match oldest {
                Some(oldest_date) => (now - oldest_date).num_days().max(0) + 1,
                None => 0,
            }
        }
    }
}

fn render_stats(
    frame: &mut Frame,
    area: Rect,
    report: &Report,
    items: &[Interception],
    last_updated: Option<&str>,
) {
    let saved = report
        .total_tokens_before
        .saturating_sub(report.total_tokens_after);
    let since = since_days(report, items);
    let text = vec![
        Line::from(vec![
            Span::styled("Interceptions: ", Style::default().fg(Color::Cyan)),
            Span::raw(format!("{}   ", report.total_interceptions)),
            Span::styled("Since: ", Style::default().fg(Color::Cyan)),
            Span::raw(format!("{since} days   ")),
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
        Some(ts) => format!(" ecotokens gain - updated {ts} UTC  [q] quit "),
        None => " ecotokens gain  [q] quit ".to_string(),
    };
    let paragraph = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(paragraph, area);
}

// ── Family gauges ─────────────────────────────────────────────────────────────

fn render_families(
    frame: &mut Frame,
    area: Rect,
    report: &Report,
    items: &[Interception],
    selected: Option<usize>,
    project_filter: Option<&str>,
) -> Vec<String> {
    let title = if let Some(proj) = project_filter {
        let basename = project_label(proj);
        format!(" By family  ·  project: {basename}  [j/u] nav  [b] projects ")
    } else {
        " By family  ·  global  [j/u] nav  [b] projects ".to_string()
    };
    let block = Block::default().borders(Borders::ALL).title(title);

    // Build family list depending on filter
    let families_owned: Vec<(String, f32)>;
    let families: Vec<(&str, f32)> = if let Some(proj) = project_filter {
        use std::collections::HashMap;
        let mut map: HashMap<String, (u64, u64)> = HashMap::new();
        for item in items.iter().filter(|i| matches_project(i, proj)) {
            if let Some(family) = serde_json::to_value(&item.command_family)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
            {
                let entry = map.entry(family).or_insert((0, 0));
                entry.0 += item.tokens_before as u64;
                entry.1 += item.tokens_after as u64;
            }
        }
        let mut sorted: Vec<(String, f32)> = map
            .into_iter()
            .map(|(k, (before, after))| {
                let pct = if before == 0 {
                    0.0f32
                } else {
                    ((1.0 - after as f64 / before as f64) * 100.0) as f32
                };
                (k, pct)
            })
            .collect();
        sorted.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        families_owned = sorted;
        families_owned
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect()
    } else {
        if report.by_family.is_empty() {
            let paragraph = Paragraph::new("No data yet.").block(block);
            frame.render_widget(paragraph, area);
            return vec![];
        }
        // Sort families by savings_pct descending
        let mut sorted: Vec<(&String, f32)> = report
            .by_family
            .iter()
            .map(|(k, v)| (k, v.savings_pct))
            .collect();
        sorted.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(b.0))
        });
        families_owned = sorted.into_iter().map(|(k, v)| (k.clone(), v)).collect();
        families_owned
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect()
    };

    if families.is_empty() {
        let paragraph = Paragraph::new("No data yet.").block(block);
        frame.render_widget(paragraph, area);
        return vec![];
    }

    // One row per family
    let n = families.len() as u16;
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let row_height = (inner.height / n).max(1);
    let constraints: Vec<Constraint> = families
        .iter()
        .map(|_| Constraint::Length(row_height))
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (i, (name, pct)) in families.iter().enumerate() {
        if i >= rows.len() {
            break;
        }
        let is_sel = selected == Some(i);
        let color = if is_sel { Color::Green } else { Color::Yellow };
        let modifier = if is_sel {
            Modifier::BOLD
        } else {
            Modifier::empty()
        };
        let prefix = if is_sel { "▶ " } else { "  " };
        let ratio = (*pct as f64 / 100.0).clamp(0.0, 1.0);
        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(color).add_modifier(modifier))
            .label(format!("{pct:.1}%"))
            .ratio(ratio)
            .block(Block::default().title(format!(" {prefix}{name} ")));
        frame.render_widget(gauge, rows[i]);
    }

    families.iter().map(|(name, _)| name.to_string()).collect()
}

// ── Project gauges ────────────────────────────────────────────────────────────

fn render_projects(
    frame: &mut Frame,
    area: Rect,
    report: &Report,
    selected: Option<usize>,
) -> Vec<String> {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" By project  [j/u] nav  [b] families ");

    if report.by_project.is_empty() {
        let paragraph = Paragraph::new("No data yet.").block(block);
        frame.render_widget(paragraph, area);
        return vec![];
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
    projects.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });

    let n = projects.len() as u16;
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let row_height = (inner.height / n).max(1);
    let constraints: Vec<Constraint> = projects
        .iter()
        .map(|_| Constraint::Length(row_height))
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (i, (name, pct)) in projects.iter().enumerate() {
        if i >= rows.len() {
            break;
        }
        let is_sel = selected == Some(i);
        let color = if is_sel { Color::Green } else { Color::Yellow };
        let modifier = if is_sel {
            Modifier::BOLD
        } else {
            Modifier::empty()
        };
        let prefix = if is_sel { "▶ " } else { "  " };
        let label = project_label(name.as_str());
        let ratio = (*pct as f64 / 100.0).clamp(0.0, 1.0);
        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(color).add_modifier(modifier))
            .label(format!("{pct:.1}%"))
            .ratio(ratio)
            .block(Block::default().title(format!(" {prefix}{label}  {name} ")));
        frame.render_widget(gauge, rows[i]);
    }

    projects.iter().map(|(name, _)| name.to_string()).collect()
}

fn render_project_log_panel(
    frame: &mut Frame,
    area: Rect,
    project_name: Option<&str>,
    items: &[Interception],
    history_scroll: &mut usize,
) {
    let Some(name) = project_name else {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Project history ");
        let p = Paragraph::new(Span::styled(
            " j u: select a project",
            Style::default().fg(Color::DarkGray),
        ))
        .block(block);
        frame.render_widget(p, area);
        return;
    };

    let label = project_label(name);

    let history: Vec<&Interception> = items
        .iter()
        .filter(|i| matches_project(i, name))
        .rev()
        .collect();
    let n = history.len();
    // How many rows fit inside the block (subtract 2 for borders).
    let visible = (area.height as usize).saturating_sub(2);
    let max_scroll = n.saturating_sub(visible);
    *history_scroll = (*history_scroll).min(max_scroll);
    let scroll = *history_scroll;
    let history: Vec<&Interception> = history.into_iter().skip(scroll).take(visible).collect();

    let scroll_hint = if n > visible {
        format!("[{}/{}]  [i/k] ", scroll + 1, n)
    } else {
        format!("{} entries ", n)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Project history: {label} · {scroll_hint}"));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = history
        .iter()
        .map(|item| {
            let ts = item.timestamp.get(..16).unwrap_or(&item.timestamp);
            let cmd = truncate_cmd(&item.command, 30);
            let sign = if item.savings_pct >= 0.0 { '-' } else { '+' };
            let abs_pct = item.savings_pct.abs();
            let text = format!(
                "{ts:<16}  {cmd:<30}  {:>6} → {:>6}  {sign}{:.1}%",
                item.tokens_before, item.tokens_after, abs_pct
            );
            Line::from(Span::styled(text, Style::default().fg(Color::Green)))
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

// ── Detail panel ──────────────────────────────────────────────────────────────

fn render_detail(
    frame: &mut Frame,
    area: Rect,
    family_name: Option<&str>,
    items: &[Interception],
    detail_mode: DetailMode,
    history_scroll: &mut usize,
) {
    let Some(name) = family_name else {
        let block = Block::default().borders(Borders::ALL).title(" Detail ");
        let p = Paragraph::new(Span::styled(
            " j u: select a family  [d] diff/log  [b] projects",
            Style::default().fg(Color::DarkGray),
        ))
        .block(block);
        frame.render_widget(p, area);
        return;
    };

    if detail_mode == DetailMode::Log {
        render_log_panel(frame, area, name, items, history_scroll);
        return;
    }

    // Find last interception for this family with actual differences
    let last = items.iter().rev().find(|i| {
        let matches_family = serde_json::to_value(&i.command_family)
            .ok()
            .and_then(|v| v.as_str().map(|s| s == name))
            .unwrap_or(false);
        let has_diff = i.content_before != i.content_after
            && (i.content_before.is_some() || i.content_after.is_some());
        matches_family && has_diff
    });

    let Some(item) = last else {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Detail: {name} "));
        let p = Paragraph::new("No interception with differences for this family.").block(block);
        frame.render_widget(p, area);
        return;
    };

    if detail_mode == DetailMode::Diff {
        render_diff_panel(frame, area, name, item, history_scroll);
    } else {
        render_split_panel(frame, area, name, item, history_scroll);
    }
}

fn is_binary(s: &str) -> bool {
    s.contains('\x00')
}

/// Truncate a command string to `max` chars, showing `…` + tail when longer.
fn truncate_cmd(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        chars.into_iter().collect()
    } else {
        let tail: String = chars[chars.len() - (max - 1)..].iter().collect();
        format!("\u{2026}{tail}")
    }
}

fn extract_outline_path(content_after: &str) -> Option<&str> {
    content_after
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("[ecotokens outline] "))
}

fn wrap_plain_lines(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![];
    }

    let mut wrapped = Vec::new();
    for raw_line in text.lines() {
        let chars: Vec<char> = raw_line.chars().collect();
        if chars.is_empty() {
            wrapped.push(String::new());
            continue;
        }

        for chunk in chars.chunks(width) {
            wrapped.push(chunk.iter().collect());
        }
    }

    if text.is_empty() {
        wrapped.push(String::new());
    }

    wrapped
}

fn render_split_panel(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    item: &Interception,
    history_scroll: &mut usize,
) {
    let ts_short = item.timestamp.get(..16).unwrap_or(&item.timestamp);
    let visible = area.height.saturating_sub(2) as usize;
    let available_width = area.width.saturating_sub(2).max(1) as usize;

    let project = item.git_root.as_deref().map(project_label);
    let mut lines = vec![Line::from(Span::styled(
        "Command",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))];
    lines.extend(
        wrap_plain_lines(item.command.as_str(), available_width)
            .into_iter()
            .map(|line| Line::from(Span::styled(line, Style::default().fg(Color::Yellow)))),
    );
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::styled("Tokens: ", Style::default().fg(Color::Cyan)),
            Span::raw(format!(
                "{} → {}  ({}{:.1}%)",
                item.tokens_before,
                item.tokens_after,
                if item.savings_pct >= 0.0 { '-' } else { '+' },
                item.savings_pct.abs()
            )),
        ]),
    ]);

    if let Some(project) = project {
        lines.push(Line::from(vec![
            Span::styled("Project: ", Style::default().fg(Color::Cyan)),
            Span::raw(project),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("Mode: ", Style::default().fg(Color::Cyan)),
        Span::raw(match item.mode {
            crate::metrics::store::FilterMode::Filtered => "filtered",
            crate::metrics::store::FilterMode::Passthrough => "passthrough",
            crate::metrics::store::FilterMode::Summarized => "summarized",
        }),
        Span::raw(format!("  Duration: {} ms", item.duration_ms)),
    ]));

    let max_scroll = lines.len().saturating_sub(visible);
    *history_scroll = (*history_scroll).min(max_scroll);
    let scroll = *history_scroll;
    let scroll_hint = if lines.len() > visible {
        format!("[{}/{}]  [i/k]  [d] diff ", scroll + 1, lines.len())
    } else {
        format!("{} lines  [i/k] scroll  [d] diff ", lines.len())
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Detail: {name} · {ts_short} · {scroll_hint}"));

    let p = Paragraph::new(lines)
        .block(block)
        .scroll((scroll as u16, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(p, area);
}

fn render_diff_panel(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    item: &Interception,
    history_scroll: &mut usize,
) {
    let cmd_short: String = if name == "native_read" {
        item.content_after
            .as_deref()
            .and_then(extract_outline_path)
            .map(|p| {
                let max = 40usize;
                if p.len() > max {
                    format!("…{}", &p[p.len() - max..])
                } else {
                    p.to_string()
                }
            })
            .unwrap_or_else(|| item.command.chars().take(40).collect())
    } else {
        item.command.chars().take(40).collect()
    };
    let ts_short = item.timestamp.get(..16).unwrap_or(&item.timestamp);
    let block = Block::default().borders(Borders::ALL).title(format!(
        " Diff : {name} · {cmd_short} · {}→{} tok ({}{:.0}%) · {ts_short}  [d] log ",
        item.tokens_before,
        item.tokens_after,
        if item.savings_pct >= 0.0 { '-' } else { '+' },
        item.savings_pct.abs(),
    ));

    let before_text = item.content_before.as_deref().unwrap_or("");
    let after_text = item.content_after.as_deref().unwrap_or("");

    if is_binary(before_text) || is_binary(after_text) {
        let p = Paragraph::new("Binary content — diff not available.")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, area);
        return;
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let diff = TextDiff::from_lines(before_text, after_text);

    let mut lines: Vec<Line> = Vec::new();
    for group in diff.grouped_ops(3) {
        // hunk header
        let old_range = group.first().and_then(|op| diff.iter_changes(op).next());
        let _ = old_range; // just for structure; build @@ line from op bounds
        let first = &group[0];
        let last = &group[group.len() - 1];
        let old_start = first.old_range().start + 1;
        let new_start = first.new_range().start + 1;
        let old_len: usize = group.iter().map(|op| op.old_range().len()).sum();
        let new_len: usize = group.iter().map(|op| op.new_range().len()).sum();
        let _ = last;
        lines.push(Line::from(Span::styled(
            format!("@@ -{old_start},{old_len} +{new_start},{new_len} @@"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));

        for op in &group {
            for change in diff.iter_changes(op) {
                let (prefix, color) = match change.tag() {
                    ChangeTag::Delete => ("-", Color::Red),
                    ChangeTag::Insert => ("+", Color::Green),
                    ChangeTag::Equal => (" ", Color::DarkGray),
                };
                let value = change.value().trim_end_matches('\n');
                lines.push(Line::from(Span::styled(
                    format!("{prefix}{value}"),
                    Style::default().fg(color),
                )));
            }
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "(no differences)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let n = lines.len();
    let visible = inner.height as usize;
    let max_scroll = n.saturating_sub(visible);
    *history_scroll = (*history_scroll).min(max_scroll);
    let scroll = *history_scroll;

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll as u16, 0)),
        inner,
    );
}

fn render_log_panel(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    items: &[Interception],
    history_scroll: &mut usize,
) {
    let family_items: Vec<&Interception> = items
        .iter()
        .filter(|i| {
            serde_json::to_value(&i.command_family)
                .ok()
                .and_then(|v| v.as_str().map(|s| s == name))
                .unwrap_or(false)
        })
        .collect();

    let history: Vec<&Interception> = family_items.iter().rev().copied().collect();
    let n = history.len();
    let visible = (area.height as usize).saturating_sub(2);
    let max_scroll = n.saturating_sub(visible);
    *history_scroll = (*history_scroll).min(max_scroll);
    let scroll = *history_scroll;
    let history: Vec<&Interception> = history.into_iter().skip(scroll).take(visible).collect();

    let scroll_hint = if n > visible {
        format!("[{}/{}]  [i/k]  [d] detail ", scroll + 1, n)
    } else {
        format!("{} entries  [d] detail ", n)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" History: {name} · {scroll_hint}"));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = history
        .iter()
        .map(|item| {
            let ts = item.timestamp.get(..16).unwrap_or(&item.timestamp);
            let cmd = truncate_cmd(&item.command, 30);
            let sign = if item.savings_pct >= 0.0 { '-' } else { '+' };
            let abs_pct = item.savings_pct.abs();
            let text = format!(
                "{ts:<16}  {cmd:<30}  {:>6} → {:>6}  {sign}{:.1}%",
                item.tokens_before, item.tokens_after, abs_pct
            );
            Line::from(Span::styled(text, Style::default().fg(Color::Green)))
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

// ── Sparkline (adaptive width) ──────────────────────────────────────────────

fn render_sparkline(frame: &mut Frame, area: Rect, items: &[Interception], mode: SparklineMode) {
    // Use available width (minus 2 for borders) so every column shows one day.
    let days = (area.width as usize).saturating_sub(2).max(14);
    let raw = build_sparkline_data(items, days);
    let data = match mode {
        SparklineMode::Linear => raw,
        SparklineMode::Log => log_scale(&raw),
        SparklineMode::Capped => cap_scale(&raw),
    };

    let title = format!(" Savings ({days} days) · {} [s] ", mode.label());
    let sparkline = Sparkline::default()
        .block(Block::default().borders(Borders::ALL).title(title))
        .style(Style::default().fg(Color::Green))
        .data(&data);

    frame.render_widget(sparkline, area);
}

fn log_scale(data: &[u64]) -> Vec<u64> {
    let max = data.iter().copied().max().unwrap_or(0);
    if max == 0 {
        return vec![0; data.len()];
    }
    let log_max = ((max + 1) as f64).ln();
    data.iter()
        .map(|&v| (((v + 1) as f64).ln() / log_max * 1000.0) as u64)
        .collect()
}

/// Caps values at P90 of non-zero entries so one outlier does not flatten the rest.
fn cap_scale(data: &[u64]) -> Vec<u64> {
    let mut nonzero: Vec<u64> = data.iter().copied().filter(|&v| v > 0).collect();
    if nonzero.is_empty() {
        return vec![0; data.len()];
    }
    nonzero.sort_unstable();
    let p90_idx = ((nonzero.len() as f64 * 0.9) as usize).min(nonzero.len() - 1);
    let cap = nonzero[p90_idx].max(1);
    data.iter().map(|&v| v.min(cap)).collect()
}

/// Bucket tokens_saved per day over the last `days` days.
fn build_sparkline_data(items: &[Interception], days: usize) -> Vec<u64> {
    let days = days.min(365);
    let mut buckets = vec![0u64; days];
    let now = Utc::now().date_naive();
    let days_i = days as i64;

    for item in items {
        if let Ok(ts) = DateTime::parse_from_rfc3339(&item.timestamp) {
            let date = ts.with_timezone(&Utc).date_naive();
            let diff = (now - date).num_days();
            if (0..days_i).contains(&diff) {
                let idx = (days_i - 1 - diff) as usize; // most recent = last bucket
                let saved = (item.tokens_before as u64).saturating_sub(item.tokens_after as u64);
                buckets[idx] = buckets[idx].saturating_add(saved);
            }
        }
    }

    buckets
}
