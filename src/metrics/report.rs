use crate::config::settings::Settings;
use crate::metrics::store::{FilterMode, HookType, Interception};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, clap::ValueEnum, Default)]
pub enum Period {
    #[default]
    All,
    Today,
    Week,
    Month,
}

impl std::fmt::Display for Period {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Period::All => "all",
            Period::Today => "today",
            Period::Week => "week",
            Period::Month => "month",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FamilyStats {
    pub count: u32,
    pub tokens_before: u64,
    pub tokens_after: u64,
    pub savings_pct: f32,
}

/// `by_subagent` key for interceptions made outside any subagent. The brackets
/// keep it apart from a subagent type, which is a plain identifier.
pub const MAIN_SESSION_BUCKET: &str = "[main]";

/// Group `items` by `key` and compute each group's savings percentage.
fn group_stats<'a>(
    items: &[&'a Interception],
    key: impl Fn(&'a Interception) -> &'a str,
) -> HashMap<String, FamilyStats> {
    let mut groups: HashMap<String, FamilyStats> = HashMap::new();
    for item in items {
        let entry = groups.entry(key(item).to_string()).or_insert(FamilyStats {
            count: 0,
            tokens_before: 0,
            tokens_after: 0,
            savings_pct: 0.0,
        });
        entry.count += 1;
        entry.tokens_before += item.tokens_before as u64;
        entry.tokens_after += item.tokens_after as u64;
    }
    for stats in groups.values_mut() {
        stats.savings_pct = if stats.tokens_before == 0 {
            0.0
        } else {
            ((1.0 - stats.tokens_after as f64 / stats.tokens_before as f64) * 100.0) as f32
        };
    }
    groups
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStats {
    pub count: u32,
    pub tokens_before: u64,
    pub tokens_after: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub period: String,
    pub total_interceptions: u32,
    pub total_tokens_before: u64,
    pub total_tokens_after: u64,
    pub total_savings_pct: f32,
    /// `None` until the user sets an input price with `ecotokens gain price`.
    #[serde(default)]
    pub cost_avoided_usd: Option<f64>,
    pub by_family: HashMap<String, FamilyStats>,
    pub by_project: HashMap<String, ProjectStats>,
    /// Grouped by host tool (`claude`, `codex`, `pi`, …), see `HookType::agent_label`.
    pub by_agent: HashMap<String, FamilyStats>,
    /// Grouped by Claude Code subagent type (`agent_type` of the hook input,
    /// e.g. `router-large`). Rows without one — the main session, other hosts
    /// and rows recorded before subagents were tracked — fall in
    /// [`MAIN_SESSION_BUCKET`]. The buckets add up to the report totals.
    #[serde(default)]
    pub by_subagent: HashMap<String, FamilyStats>,
    /// Additive token cost of automatic-pipeline rewrite transformations
    /// (`Origin::AutoPipeline`, `FilterMode::Rewritten`). Local `Cli`/`Mcp`
    /// rewrites consume no paid tokens and are excluded entirely from this
    /// report's sums — see `aggregate()` (FR-041, FR-041a, SC-012).
    #[serde(default)]
    pub rewrite_overhead_tokens: u64,
}

/// Lines of the "By subagent" section of the plain-text `ecotokens gain`
/// output: the main-session bucket first, then each subagent type by name.
/// Empty when no interception came from a subagent, so the section only
/// appears once there is something to compare.
pub fn subagent_lines(report: &Report) -> Vec<String> {
    if report
        .by_subagent
        .keys()
        .all(|k| k.as_str() == MAIN_SESSION_BUCKET)
    {
        return Vec::new();
    }
    let mut rows: Vec<_> = report.by_subagent.iter().collect();
    rows.sort_by(|(a, _), (b, _)| {
        (a.as_str() != MAIN_SESSION_BUCKET, a.as_str())
            .cmp(&(b.as_str() != MAIN_SESSION_BUCKET, b.as_str()))
    });
    let width = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0).max(12);
    let mut lines = vec!["By subagent    :".to_string()];
    for (name, stats) in rows {
        let saved = stats.tokens_before.saturating_sub(stats.tokens_after);
        lines.push(format!(
            "  {name:<width$} {:>6} runs  {saved:>10} tokens saved  {:.1}% savings",
            stats.count, stats.savings_pct
        ));
    }
    lines
}

pub fn period_start(period: &Period) -> Option<DateTime<Utc>> {
    let now = Utc::now();
    match period {
        Period::All => None,
        Period::Today => {
            let today = now.date_naive();
            today.and_hms_opt(0, 0, 0).map(|dt| dt.and_utc())
        }
        Period::Week => Some(now - chrono::Duration::days(7)),
        Period::Month => Some(now - chrono::Duration::days(30)),
    }
}

fn period_label(period: &Period) -> &'static str {
    match period {
        Period::All => "all",
        Period::Today => "today",
        Period::Week => "week",
        Period::Month => "month",
    }
}

/// Filter interceptions by period start time.
fn filter_items_by_period<'a>(
    items: impl Iterator<Item = &'a Interception>,
    start: Option<DateTime<Utc>>,
) -> Vec<&'a Interception> {
    items
        .filter(|item| match start {
            // With a period filter active, only include items whose timestamp
            // parses and falls within the window. An unparseable/corrupt timestamp
            // is excluded rather than leaking into every date-bounded report.
            Some(start_ts) => DateTime::parse_from_rfc3339(&item.timestamp)
                .map(|ts| ts.with_timezone(&Utc) >= start_ts)
                .unwrap_or(false),
            // No period filter → include everything.
            None => true,
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryReport {
    pub day: Report,
    pub week: Report,
    pub month: Report,
}

/// Aggregate interceptions for three rolling time windows at once.
pub fn aggregate_history(items: &[Interception]) -> HistoryReport {
    HistoryReport {
        day: aggregate(items, Period::Today),
        week: aggregate(items, Period::Week),
        month: aggregate(items, Period::Month),
    }
}

/// Filter interceptions by period, reusing the same logic as `aggregate`.
pub fn filter_by_period(items: &[Interception], period: &Period) -> Vec<Interception> {
    let start = period_start(period);
    filter_items_by_period(items.iter(), start)
        .into_iter()
        .cloned()
        .collect()
}

/// Keeps only the interceptions of `project` (a git root, as stored in `git_root`).
pub fn keep_project(items: &mut Vec<Interception>, project: &str) {
    items.retain(|i| i.git_root.as_deref() == Some(project));
}

/// Aggregate interceptions into a Report.
pub fn aggregate(items: &[Interception], period: Period) -> Report {
    aggregate_with_price(items, period, Settings::load().price_input_usd_per_mtok)
}

/// Same as [`aggregate`] with an explicit input price (USD per million tokens)
/// instead of the configured one.
pub fn aggregate_with_price(
    items: &[Interception],
    period: Period,
    input_usd_per_mtok: Option<f64>,
) -> Report {
    let start = period_start(&period);
    let period_items = filter_items_by_period(items.iter(), start);

    // Rewrite rows are transformations, not compressions, and are frequently
    // token-*expanding* — summing them into the sums below would corrupt
    // `total_savings_pct` (research.md §9, FR-040/041). They are excluded
    // entirely from every field below. `AutoPipeline`-origin rewrites (the
    // only ones with a real paid-token cost) have their expansion delta
    // surfaced separately as `rewrite_overhead_tokens` instead (FR-041a,
    // SC-012) rather than silently dropped.
    let rewrite_overhead_tokens: u64 = period_items
        .iter()
        .filter(|i| {
            i.mode == FilterMode::Rewritten && !matches!(i.hook_type, HookType::Cli | HookType::Mcp)
        })
        .map(|i| i.tokens_after.saturating_sub(i.tokens_before) as u64)
        .sum();

    let filtered: Vec<&Interception> = period_items
        .into_iter()
        .filter(|i| i.mode != FilterMode::Rewritten)
        .collect();

    let total_before: u64 = filtered.iter().map(|i| i.tokens_before as u64).sum();
    let total_after: u64 = filtered.iter().map(|i| i.tokens_after as u64).sum();

    let total_savings_pct = if total_before == 0 {
        0.0
    } else {
        ((1.0 - total_after as f64 / total_before as f64) * 100.0) as f32
    };

    let tokens_saved = total_before.saturating_sub(total_after);
    let cost_avoided_usd =
        input_usd_per_mtok.map(|price| (tokens_saved as f64 / 1_000_000.0) * price);

    let by_family = group_stats(&filtered, |i| i.command_family.as_str());
    let by_agent = group_stats(&filtered, |i| i.hook_type.agent_label());
    let by_subagent = group_stats(&filtered, |i| {
        i.agent_type.as_deref().unwrap_or(MAIN_SESSION_BUCKET)
    });

    // by_project
    let mut by_project: HashMap<String, ProjectStats> = HashMap::new();
    for item in &filtered {
        let key = match &item.git_root {
            Some(root) => {
                let root = root.trim();
                if root.is_empty() {
                    "[undefined]"
                } else {
                    root
                }
            }
            None => "[undefined]",
        };
        let entry = by_project.entry(key.to_string()).or_insert(ProjectStats {
            count: 0,
            tokens_before: 0,
            tokens_after: 0,
        });
        entry.count += 1;
        entry.tokens_before += item.tokens_before as u64;
        entry.tokens_after += item.tokens_after as u64;
    }

    Report {
        period: period_label(&period).to_string(),
        total_interceptions: filtered.len() as u32,
        total_tokens_before: total_before,
        total_tokens_after: total_after,
        total_savings_pct,
        cost_avoided_usd,
        by_family,
        by_project,
        by_agent,
        by_subagent,
        rewrite_overhead_tokens,
    }
}
