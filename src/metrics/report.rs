use crate::metrics::store::Interception;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Period {
    All,
    Today,
    Week,
    Month,
}

impl Period {
    pub fn parse(s: &str) -> Self {
        match s {
            "today" => Period::Today,
            "week" => Period::Week,
            "month" => Period::Month,
            _ => Period::All,
        }
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
}

fn default_pricing_usd_per_1m(model: &str) -> f64 {
    match model {
        "claude-haiku-4-5" => 0.80,
        "claude-opus-4-6" => 15.00,
        "github-copilot" => 0.0,
        _ => 3.00, // sonnet default
    }
}

fn period_start(period: &Period) -> Option<DateTime<Utc>> {
    let now = Utc::now();
    match period {
        Period::All => None,
        Period::Today => {
            let today = now.date_naive();
            Some(today.and_hms_opt(0, 0, 0).unwrap().and_utc())
        }
        Period::Week => Some(now - chrono::Duration::days(7)),
        Period::Month => Some(now - chrono::Duration::days(30)),
    }
}

/// Filter interceptions by period start time.
fn filter_items_by_period<'a>(
    items: impl Iterator<Item = &'a Interception>,
    start: Option<DateTime<Utc>>,
) -> Vec<&'a Interception> {
    items
        .filter(|item| {
            if let Some(start_ts) = start {
                if let Ok(ts) = DateTime::parse_from_rfc3339(&item.timestamp) {
                    return ts.with_timezone(&Utc) >= start_ts;
                }
            }
            true
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
    let filtered = filter_items_by_period(items.iter(), start);

    let total_before: u64 = filtered.iter().map(|i| i.tokens_before as u64).sum();
    let total_after: u64 = filtered.iter().map(|i| i.tokens_after as u64).sum();

    let total_savings_pct = if total_before == 0 {
        0.0
    } else {
        ((1.0 - total_after as f64 / total_before as f64) * 100.0) as f32
    };

    let tokens_saved = total_before.saturating_sub(total_after);
    let price_per_1m = default_pricing_usd_per_1m(model);
    let cost_avoided_usd = (tokens_saved as f64 / 1_000_000.0) * price_per_1m;

    // by_family
    let mut by_family: HashMap<String, FamilyStats> = HashMap::new();
    for item in &filtered {
        let key = serde_json::to_value(&item.command_family)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("{:?}", item.command_family).to_lowercase());
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

    // by_project
    let mut by_project: HashMap<String, ProjectStats> = HashMap::new();
    for item in &filtered {
        if let Some(root) = &item.git_root {
            let root = root.trim();
            let key = if root.is_empty() { "(unknown)" } else { root };
            let entry = by_project.entry(key.to_string()).or_insert(ProjectStats {
                count: 0,
                tokens_before: 0,
                tokens_after: 0,
            });
            entry.count += 1;
            entry.tokens_before += item.tokens_before as u64;
            entry.tokens_after += item.tokens_after as u64;
        }
    }

    Report {
        period: format!("{period:?}").to_lowercase(),
        total_interceptions: filtered.len() as u32,
        total_tokens_before: total_before,
        total_tokens_after: total_after,
        total_savings_pct,
        cost_avoided_usd,
        model_ref: model.to_string(),
        by_family,
        by_project,
    }
}
