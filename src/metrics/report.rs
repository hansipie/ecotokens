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
    pub cost_avoided_usd: f64,
    pub model_ref: String,
    pub by_family: HashMap<String, FamilyStats>,
    pub by_project: HashMap<String, ProjectStats>,
    pub by_agent: HashMap<String, FamilyStats>,
    /// Additive token cost of automatic-pipeline rewrite transformations
    /// (`Origin::AutoPipeline`, `FilterMode::Rewritten`). Local `Cli`/`Mcp`
    /// rewrites consume no paid tokens and are excluded entirely from this
    /// report's sums — see `aggregate()` (FR-041, FR-041a, SC-012).
    #[serde(default)]
    pub rewrite_overhead_tokens: u64,
}

fn pricing_usd_per_1m(model: &str, settings: &Settings) -> f64 {
    if let Some(p) = settings.model_pricing.get(model) {
        return p.input_usd_per_1m;
    }
    if let Some(p) = crate::config::models::get_price(model) {
        return p.input_usd_per_1m;
    }
    3.00
}

fn period_start(period: &Period) -> Option<DateTime<Utc>> {
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
    pub model_ref: String,
    pub day: Report,
    pub week: Report,
    pub month: Report,
}

/// Aggregate interceptions for three rolling time windows at once.
pub fn aggregate_history(items: &[Interception], model: &str) -> HistoryReport {
    HistoryReport {
        model_ref: model.to_string(),
        day: aggregate(items, Period::Today, model),
        week: aggregate(items, Period::Week, model),
        month: aggregate(items, Period::Month, model),
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

/// Aggregate interceptions into a Report.
pub fn aggregate(items: &[Interception], period: Period, model: &str) -> Report {
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
    let settings = Settings::load();
    let price_per_1m = pricing_usd_per_1m(model, &settings);
    let cost_avoided_usd = (tokens_saved as f64 / 1_000_000.0) * price_per_1m;

    // by_family
    let mut by_family: HashMap<String, FamilyStats> = HashMap::new();
    for item in &filtered {
        let key = item.command_family.as_str().to_string();
        let entry = by_family.entry(key).or_insert(FamilyStats {
            count: 0,
            tokens_before: 0,
            tokens_after: 0,
            savings_pct: 0.0,
        });
        entry.count += 1;
        entry.tokens_before += item.tokens_before as u64;
        entry.tokens_after += item.tokens_after as u64;
    }
    for stats in by_family.values_mut() {
        stats.savings_pct = if stats.tokens_before == 0 {
            0.0
        } else {
            ((1.0 - stats.tokens_after as f64 / stats.tokens_before as f64) * 100.0) as f32
        };
    }

    // by_agent
    let mut by_agent: HashMap<String, FamilyStats> = HashMap::new();
    for item in &filtered {
        let agent = item.hook_type.agent_label();
        let entry = by_agent.entry(agent.to_string()).or_insert(FamilyStats {
            count: 0,
            tokens_before: 0,
            tokens_after: 0,
            savings_pct: 0.0,
        });
        entry.count += 1;
        entry.tokens_before += item.tokens_before as u64;
        entry.tokens_after += item.tokens_after as u64;
    }
    for stats in by_agent.values_mut() {
        stats.savings_pct = if stats.tokens_before == 0 {
            0.0
        } else {
            ((1.0 - stats.tokens_after as f64 / stats.tokens_before as f64) * 100.0) as f32
        };
    }

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
        model_ref: model.to_string(),
        by_family,
        by_project,
        by_agent,
        rewrite_overhead_tokens,
    }
}
