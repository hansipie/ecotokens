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
    Details,
    Diff,
    SplitRaw,
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
            DetailMode::Details => DetailMode::Diff,
            DetailMode::Diff => DetailMode::SplitRaw,
            DetailMode::SplitRaw => DetailMode::Details,
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
/// `"[undefined]"` matches items with a blank or absent `git_root`.
fn matches_project(item: &Interception, project: &str) -> bool {
    if project == "[undefined]" {
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

/// Returns the number of items shown in the active log/history panel.
/// Used by the event loop to clamp the selection index.
pub fn log_item_count(
    items: &[Interception],
    gain_mode: GainMode,
    selected_family: Option<usize>,
    selected_project: Option<usize>,
    project_filter: Option<&str>,
    report: &Report,
    sorted_projects: &[(String, f32)],
) -> usize {
    match gain_mode {
        GainMode::Project => {
            let Some(idx) = selected_project else {
                return 0;
            };
            let Some((name, _)) = sorted_projects.get(idx) else {
                return 0;
            };
            items.iter().filter(|i| matches_project(i, name)).count()
        }
        GainMode::Family => {
            let family_names: Vec<String> = if let Some(proj) = project_filter {
                sorted_family_keys_for_project(items, proj)
            } else {
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
                sorted.into_iter().map(|(k, _)| k.clone()).collect()
            };
            let Some(idx) = selected_family else { return 0 };
            let Some(name) = family_names.get(idx) else {
                return 0;
            };
            items
                .iter()
                .filter(|i| {
                    if let Some(proj) = project_filter {
                        if !matches_project(i, proj) {
                            return false;
                        }
                    }
                    serde_json::to_value(&i.command_family)
                        .ok()
                        .and_then(|v| v.as_str().map(|s| s == name.as_str()))
                        .unwrap_or(false)
                })
                .count()
        }
    }
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
    log_scroll: &mut usize,
    log_selected: Option<usize>,
    gauge_scroll: &mut usize,
    split_raw_after_scroll: &mut usize,
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
            .constraints([Constraint::Min(3), Constraint::Min(5)])
            .split(outer[1]);
        let project_names = render_projects(frame, pool[0], report, selected_project, gauge_scroll);
        let sel_proj = selected_project
            .and_then(|i| project_names.get(i))
            .map(String::as_str);
        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(pool[1]);
        let selected_proj_item =
            render_project_log_panel(frame, bottom[0], sel_proj, items, log_scroll, log_selected);
        render_project_detail(
            frame,
            bottom[1],
            sel_proj,
            items,
            detail_mode,
            history_scroll,
            split_raw_after_scroll,
            selected_proj_item,
        );
        render_sparkline(frame, outer[2], items, sparkline_mode);
        return;
    }

    // GainMode::Family — jauges pleine largeur, puis History | Detail en bas
    let pool = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Min(5)])
        .split(outer[1]);

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
        gauge_scroll,
    );
    let sel_name = selected_family
        .and_then(|i| family_names.get(i))
        .map(String::as_str);

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(pool[1]);

    let selected_log_item = render_log_panel(
        frame,
        bottom[0],
        sel_name,
        display_items,
        log_scroll,
        log_selected,
    );

    render_detail(
        frame,
        bottom[1],
        sel_name,
        display_items,
        detail_mode,
        history_scroll,
        split_raw_after_scroll,
        selected_log_item,
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

const GAUGE_MIN_HEIGHT: u16 = 2;

/// Adjust scroll offset to keep `selected` visible in a window of `visible` items.
fn adjust_gauge_scroll(
    scroll: &mut usize,
    selected: Option<usize>,
    visible: usize,
    max_scroll: usize,
) {
    if let Some(sel) = selected {
        if sel < *scroll {
            *scroll = sel;
        } else if sel >= *scroll + visible {
            *scroll = sel + 1 - visible;
        }
    }
    *scroll = (*scroll).min(max_scroll);
}

fn render_families(
    frame: &mut Frame,
    area: Rect,
    report: &Report,
    items: &[Interception],
    selected: Option<usize>,
    project_filter: Option<&str>,
    gauge_scroll: &mut usize,
) -> Vec<String> {
    let title = if let Some(proj) = project_filter {
        let basename = project_label(proj);
        format!(" By family  ·  project: {basename}  [j/u] nav  [p] projects ")
    } else {
        " By family  ·  global  [j/u] nav  [p] projects ".to_string()
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

    let n = families.len();
    let inner = block.inner(area);
    let visible = ((inner.height / GAUGE_MIN_HEIGHT) as usize).max(1);
    let max_scroll = n.saturating_sub(visible);
    adjust_gauge_scroll(gauge_scroll, selected, visible, max_scroll);

    let scroll = *gauge_scroll;
    let slice = &families[scroll..(scroll + visible).min(n)];

    let scroll_hint = if n > visible {
        format!(" [{}/{}] ", scroll + 1, n)
    } else {
        String::new()
    };
    let block = block.title(scroll_hint);
    frame.render_widget(block, area);

    let constraints: Vec<Constraint> = slice
        .iter()
        .map(|_| Constraint::Length(GAUGE_MIN_HEIGHT))
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (i, (name, pct)) in slice.iter().enumerate() {
        let global_idx = scroll + i;
        let is_sel = selected == Some(global_idx);
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
    gauge_scroll: &mut usize,
) -> Vec<String> {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" By project  [j/u] nav  [f] families ");

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

    let n = projects.len();
    let inner = block.inner(area);
    let visible = ((inner.height / GAUGE_MIN_HEIGHT) as usize).max(1);
    let max_scroll = n.saturating_sub(visible);
    adjust_gauge_scroll(gauge_scroll, selected, visible, max_scroll);

    let scroll = *gauge_scroll;
    let slice = &projects[scroll..(scroll + visible).min(n)];

    let scroll_hint = if n > visible {
        format!(" [{}/{}] ", scroll + 1, n)
    } else {
        String::new()
    };
    let block = block.title(scroll_hint);
    frame.render_widget(block, area);

    let constraints: Vec<Constraint> = slice
        .iter()
        .map(|_| Constraint::Length(GAUGE_MIN_HEIGHT))
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (i, (name, pct)) in slice.iter().enumerate() {
        let global_idx = scroll + i;
        let is_sel = selected == Some(global_idx);
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

#[allow(clippy::too_many_arguments)]
fn render_log_panel_inner<'a>(
    frame: &mut Frame,
    area: Rect,
    name: Option<&str>,
    items: &'a [Interception],
    history_scroll: &mut usize,
    selected: Option<usize>,
    empty_title: &str,
    empty_hint: &str,
    make_title: impl FnOnce(&str) -> String,
    filter: impl Fn(&Interception, &str) -> bool,
) -> Option<&'a Interception> {
    let Some(name) = name else {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(empty_title.to_string());
        let p = Paragraph::new(Span::styled(
            empty_hint,
            Style::default().fg(Color::DarkGray),
        ))
        .block(block);
        frame.render_widget(p, area);
        return None;
    };
    let filtered: Vec<&'a Interception> = items.iter().filter(|i| filter(i, name)).rev().collect();
    let n = filtered.len();
    let selected_item = selected
        .map(|s| s.min(n.saturating_sub(1)))
        .and_then(|s| filtered.get(s).copied());
    render_history_panel(
        frame,
        area,
        &make_title(name),
        filtered,
        history_scroll,
        selected,
        "",
    );
    selected_item
}

fn render_project_log_panel<'a>(
    frame: &mut Frame,
    area: Rect,
    project_name: Option<&str>,
    items: &'a [Interception],
    history_scroll: &mut usize,
    selected: Option<usize>,
) -> Option<&'a Interception> {
    render_log_panel_inner(
        frame,
        area,
        project_name,
        items,
        history_scroll,
        selected,
        " Project history ",
        " j u: select a project",
        |n| format!(" Project history: {}", project_label(n)),
        matches_project,
    )
}

// ── Detail panel ──────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn render_detail_inner<'a>(
    frame: &mut Frame,
    area: Rect,
    name: Option<&str>,
    items: &'a [Interception],
    detail_mode: DetailMode,
    history_scroll: &mut usize,
    after_scroll: &mut usize,
    selected_item: Option<&'a Interception>,
    empty_hint: &str,
    display_name: impl FnOnce(&str) -> String,
    matches: impl Fn(&Interception, &str) -> bool,
    no_diff_msg: &str,
) {
    let Some(raw_name) = name else {
        let block = Block::default().borders(Borders::ALL).title(" Detail ");
        let p = Paragraph::new(Span::styled(
            empty_hint,
            Style::default().fg(Color::DarkGray),
        ))
        .block(block);
        frame.render_widget(p, area);
        return;
    };

    let label = display_name(raw_name);

    // Use the explicitly selected item if provided, otherwise fall back to the last with differences.
    let item = selected_item.or_else(|| {
        items.iter().rev().find(|i| {
            let has_diff = i.content_before != i.content_after
                && (i.content_before.is_some() || i.content_after.is_some());
            matches(i, raw_name) && has_diff
        })
    });

    let Some(item) = item else {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Detail: {label} "));
        let p = Paragraph::new(no_diff_msg).block(block);
        frame.render_widget(p, area);
        return;
    };

    match detail_mode {
        DetailMode::Diff => render_diff_panel(frame, area, &label, item, history_scroll),
        DetailMode::SplitRaw => {
            render_split_raw_panel(frame, area, &label, item, history_scroll, after_scroll)
        }
        DetailMode::Details => render_details_panel(frame, area, &label, item, history_scroll),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_detail<'a>(
    frame: &mut Frame,
    area: Rect,
    family_name: Option<&str>,
    items: &'a [Interception],
    detail_mode: DetailMode,
    history_scroll: &mut usize,
    after_scroll: &mut usize,
    selected_item: Option<&'a Interception>,
) {
    render_detail_inner(
        frame,
        area,
        family_name,
        items,
        detail_mode,
        history_scroll,
        after_scroll,
        selected_item,
        " j u: select a family  [d] diff/log  [p] projects",
        |n| n.to_string(),
        |i, name| {
            serde_json::to_value(&i.command_family)
                .ok()
                .and_then(|v| v.as_str().map(|s| s == name))
                .unwrap_or(false)
        },
        "No interception with differences for this family.",
    )
}

#[allow(clippy::too_many_arguments)]
fn render_project_detail<'a>(
    frame: &mut Frame,
    area: Rect,
    project_name: Option<&str>,
    items: &'a [Interception],
    detail_mode: DetailMode,
    history_scroll: &mut usize,
    after_scroll: &mut usize,
    selected_item: Option<&'a Interception>,
) {
    render_detail_inner(
        frame,
        area,
        project_name,
        items,
        detail_mode,
        history_scroll,
        after_scroll,
        selected_item,
        " j u: select a project  [d] diff  [f] families",
        project_label,
        matches_project,
        "No interception with differences for this project.",
    )
}

fn is_binary(s: &str) -> bool {
    s.contains('\x00')
}

/// Truncate a command string to `max` chars, showing head + `…` when longer.
fn truncate_cmd(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        chars.into_iter().collect()
    } else {
        let head: String = chars[..max - 1].iter().collect();
        format!("{}\u{2026}", head)
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

fn render_details_panel(
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
    lines.push(Line::from(vec![
        Span::styled("Agent:", Style::default().fg(Color::Cyan)),
        Span::raw(item.hook_type.agent_label()),
    ]));

    let max_scroll = lines.len().saturating_sub(visible);
    *history_scroll = (*history_scroll).min(max_scroll);
    let scroll = *history_scroll;
    let scroll_hint = if lines.len() > visible {
        format!("[{}/{}]  [o/l]  [d] cycle ", scroll + 1, lines.len())
    } else {
        format!("{} lines  [o/l] scroll  [d] cycle ", lines.len())
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

const MAX_HUNK_LINES: usize = 15;
const KEEP_LINES: usize = 5;

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

    let before_text = item.content_before.as_deref().unwrap_or("");
    let after_text = item.content_after.as_deref().unwrap_or("");

    if is_binary(before_text) || is_binary(after_text) {
        let block = Block::default().borders(Borders::ALL).title(format!(
            " Diff : {name} · {cmd_short} · {}→{} tok ({}{:.0}%) · {ts_short}  [d] cycle ",
            item.tokens_before,
            item.tokens_after,
            if item.savings_pct >= 0.0 { '-' } else { '+' },
            item.savings_pct.abs(),
        ));
        let p = Paragraph::new("Binary content — diff not available.")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, area);
        return;
    }

    let available_width = area.width.saturating_sub(2) as usize;

    // ── Étape 1 : en-tête visuel BEFORE / AFTER ──────────────────────────────
    let tb = item.tokens_before;
    let ta = item.tokens_after;
    let pct = if tb > 0 {
        (1.0 - ta as f64 / tb as f64) * 100.0
    } else {
        0.0
    };

    let before_prefix = format!(" BEFORE  {:>8} tokens  ", tb);
    let after_prefix = format!(" AFTER  {:>8} tokens  ", ta);
    let dash_fill = available_width.saturating_sub(before_prefix.len());
    let dashes: String = "─".repeat(dash_fill);

    let bar_width = (area.width as usize).saturating_sub(40).max(5);
    let suffix = format!("  −{:.1} %", pct.max(0.0));
    let bar_avail = available_width
        .saturating_sub(after_prefix.len())
        .saturating_sub(suffix.len());
    let bar_total = bar_width.min(bar_avail);
    let filled = ((pct / 100.0) * bar_total as f64).round() as usize;
    let filled = filled.min(bar_total);
    let bar: String = "▌".repeat(filled) + &"░".repeat(bar_total - filled);

    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(
                " BEFORE  ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:>8} tokens  ", tb),
                Style::default().fg(Color::White),
            ),
            Span::styled(dashes, Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(
                " AFTER  ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:>8} tokens  ", ta),
                Style::default().fg(Color::White),
            ),
            Span::styled(bar, Style::default().fg(Color::Green)),
            Span::styled(
                suffix,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            "─".repeat(available_width),
            Style::default().fg(Color::DarkGray),
        )),
    ];

    // ── Étape 2 & 3 : hunks numérotés + troncature ──────────────────────────
    let diff = TextDiff::from_lines(before_text, after_text);
    let all_groups = diff.grouped_ops(3);
    let total_groups = all_groups.len();

    for (hunk_idx, group) in all_groups.iter().enumerate() {
        let first = &group[0];
        let old_start = first.old_range().start + 1;
        let section_label = format!(
            "─── section {}/{}  l.{old_start} ",
            hunk_idx + 1,
            total_groups
        );
        let section_fill =
            "─".repeat(available_width.saturating_sub(section_label.chars().count()));
        lines.push(Line::from(Span::styled(
            format!("{section_label}{section_fill}"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));

        for op in group {
            let changes: Vec<_> = diff.iter_changes(op).collect();
            let all_delete = changes.iter().all(|c| c.tag() == ChangeTag::Delete);
            let all_insert = changes.iter().all(|c| c.tag() == ChangeTag::Insert);
            let truncate = changes.len() > MAX_HUNK_LINES && (all_delete || all_insert);

            if truncate {
                let omitted = changes.len() - 2 * KEEP_LINES;
                for change in &changes[..KEEP_LINES] {
                    push_diff_line(&mut lines, change);
                }
                lines.push(Line::from(Span::styled(
                    format!("⋯  +{omitted} lignes omises  ⋯"),
                    Style::default().fg(Color::DarkGray),
                )));
                for change in &changes[changes.len() - KEEP_LINES..] {
                    push_diff_line(&mut lines, change);
                }
            } else {
                for change in &changes {
                    push_diff_line(&mut lines, change);
                }
            }
        }
    }

    if lines.len() <= 3 {
        // Only the header was added, no hunks
        lines.push(Line::from(Span::styled(
            "(no differences)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let n = lines.len();
    let tmp_block = Block::default().borders(Borders::ALL);
    let visible = tmp_block.inner(area).height as usize;
    let max_scroll = n.saturating_sub(visible);
    *history_scroll = (*history_scroll).min(max_scroll);
    let scroll = *history_scroll;
    let scroll_hint = if n > visible {
        format!("[{}/{}]  [o/l]  [d] cycle ", scroll + 1, n)
    } else {
        format!("{n} lines  [o/l] scroll  [d] cycle ")
    };
    let block = Block::default().borders(Borders::ALL).title(format!(
        " Diff : {name} · {cmd_short} · {}→{} tok · {ts_short} · {scroll_hint}",
        item.tokens_before, item.tokens_after,
    ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll as u16, 0)),
        inner,
    );
}

fn push_diff_line(lines: &mut Vec<Line>, change: &similar::Change<&str>) {
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

// ── Étape 4 : mode SplitRaw ─────────────────────────────────────────────────
fn render_split_raw_panel(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    item: &Interception,
    history_scroll: &mut usize,
    after_scroll: &mut usize,
) {
    let ts_short = item.timestamp.get(..16).unwrap_or(&item.timestamp);
    let before_text = item.content_before.as_deref().unwrap_or("(vide)");
    let after_text = item.content_after.as_deref().unwrap_or("(vide)");

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let before_lines: Vec<Line> = before_text
        .lines()
        .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(Color::Red))))
        .collect();
    let after_lines: Vec<Line> = after_text
        .lines()
        .map(|l| {
            Line::from(Span::styled(
                l.to_string(),
                Style::default().fg(Color::Green),
            ))
        })
        .collect();

    // BEFORE : scrollable via o/l
    let visible_top = chunks[0].height.saturating_sub(2) as usize;
    let max_before = before_lines.len().saturating_sub(visible_top);
    *history_scroll = (*history_scroll).min(max_before);
    let scroll_before = *history_scroll;

    let before_hint = if before_lines.len() > visible_top {
        format!(
            "[{}/{}]  [o/l]  [d] cycle ",
            scroll_before + 1,
            before_lines.len()
        )
    } else {
        format!("{} lignes  [o/l]  [d] cycle ", before_lines.len())
    };

    // AFTER : scrollable via Maj+O/Maj+L
    let visible_bot = chunks[1].height.saturating_sub(2) as usize;
    let max_after = after_lines.len().saturating_sub(visible_bot);
    *after_scroll = (*after_scroll).min(max_after);
    let scroll_after = *after_scroll;

    let after_hint = if after_lines.len() > visible_bot {
        format!("[{}/{}]  [O/L] ", scroll_after + 1, after_lines.len())
    } else {
        format!("{} lignes  [O/L] ", after_lines.len())
    };

    let top_block = Block::default().borders(Borders::ALL).title(format!(
        " BEFORE · {name} · {} tok · {ts_short} · {before_hint}",
        item.tokens_before
    ));
    let bot_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" AFTER · {} tok · {after_hint}", item.tokens_after));

    frame.render_widget(
        Paragraph::new(before_lines)
            .block(top_block)
            .wrap(Wrap { trim: false })
            .scroll((scroll_before as u16, 0)),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(after_lines)
            .block(bot_block)
            .wrap(Wrap { trim: false })
            .scroll((scroll_after as u16, 0)),
        chunks[1],
    );
}

fn render_log_panel<'a>(
    frame: &mut Frame,
    area: Rect,
    name: Option<&str>,
    items: &'a [Interception],
    history_scroll: &mut usize,
    selected: Option<usize>,
) -> Option<&'a Interception> {
    render_log_panel_inner(
        frame,
        area,
        name,
        items,
        history_scroll,
        selected,
        " History ",
        " j u: select a family",
        |n| format!(" History: {n}"),
        |i, name| {
            serde_json::to_value(&i.command_family)
                .ok()
                .and_then(|v| v.as_str().map(|s| s == name))
                .unwrap_or(false)
        },
    )
}

fn render_history_panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    history: Vec<&Interception>,
    history_scroll: &mut usize,
    selected: Option<usize>,
    extra_hint: &str,
) {
    let n = history.len();
    // How many rows fit inside the block (subtract 2 for borders).
    let visible = (area.height as usize).saturating_sub(2);
    let max_scroll = n.saturating_sub(visible);
    // Clamp selected to valid range and use it to auto-scroll the view.
    let clamped_selected = selected.map(|s| s.min(n.saturating_sub(1)));
    adjust_gauge_scroll(history_scroll, clamped_selected, visible, max_scroll);
    let scroll = *history_scroll;
    let history: Vec<&Interception> = history.into_iter().skip(scroll).take(visible).collect();

    let hint = if let Some(sel) = clamped_selected {
        format!("[{}/{}]  [i/k]  {extra_hint}", sel + 1, n)
    } else if n > visible {
        format!("[{}/{}]  [i/k]  {extra_hint}", scroll + 1, n)
    } else {
        format!("{n} entries  {extra_hint}")
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!("{title} · {hint}"));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = history
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let ts = item.timestamp.get(..16).unwrap_or(&item.timestamp);
            let cmd = truncate_cmd(&item.command, 30);
            let sign = if item.savings_pct >= 0.0 { '-' } else { '+' };
            let abs_pct = item.savings_pct.abs();
            let text = format!(
                "{ts:<16}  {cmd:<30}  {:>6} → {:>6}  {sign}{:.1}%",
                item.tokens_before, item.tokens_after, abs_pct
            );
            let is_selected = clamped_selected.is_some_and(|s| s == scroll + idx);
            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Green)
            } else {
                Style::default().fg(Color::Green)
            };
            Line::from(Span::styled(text, style))
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
