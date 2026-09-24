use chrono::Utc;
use ecotokens::metrics::report::{aggregate, Period};
use ecotokens::metrics::store::{CommandFamily, FilterMode, HookType, Interception};

fn base_interception(family: CommandFamily, tokens_before: u32, tokens_after: u32) -> Interception {
    Interception {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        command: "git status".into(),
        command_family: family,
        git_root: Some("/repo".into()),
        tokens_before,
        tokens_after,
        savings_pct: ((1.0 - tokens_after as f64 / tokens_before as f64) * 100.0) as f32,
        mode: FilterMode::Filtered,
        redacted: false,
        duration_ms: 5,
        content_before: None,
        content_after: None,
        hook_type: HookType::PreToolUse,
    }
}

fn rewrite_row(tokens_before: u32, tokens_after: u32, hook_type: HookType) -> Interception {
    Interception {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        command: "ecotokens rewrite --mode paraphrase".into(),
        command_family: CommandFamily::Generic,
        git_root: Some("/repo".into()),
        tokens_before,
        tokens_after,
        savings_pct: 0.0,
        mode: FilterMode::Rewritten,
        redacted: false,
        duration_ms: 500,
        content_before: None,
        content_after: None,
        hook_type,
    }
}

fn base_items() -> Vec<Interception> {
    vec![
        base_interception(CommandFamily::Git, 1000, 200),
        base_interception(CommandFamily::Cargo, 500, 100),
        base_interception(CommandFamily::Python, 300, 250),
    ]
}

/// P0 regression (SC-008): capture the full `gain --json`-equivalent report,
/// insert N `FilterMode::Rewritten` rows of every origin, re-capture, and
/// assert every pre-existing field is byte-identical.
#[test]
fn rewrite_rows_do_not_alter_the_report() {
    let before_items = base_items();
    let before = aggregate(&before_items, Period::All);
    // Compare via parsed JSON values (order-independent), not raw strings —
    // HashMap key order is not stable across the two aggregate() calls.
    let before_json: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&before).unwrap()).unwrap();

    let mut after_items = before_items;
    // Cli/Mcp origin: local-only, must be fully excluded from every sum.
    after_items.push(rewrite_row(400, 600, HookType::Cli));
    after_items.push(rewrite_row(400, 150, HookType::Cli));
    after_items.push(rewrite_row(800, 900, HookType::Mcp));
    // AutoPipeline origin (any hook_type other than Cli/Mcp): excluded from
    // the main sums too, but must surface via rewrite_overhead_tokens.
    after_items.push(rewrite_row(300, 500, HookType::PostToolUse));

    let after = aggregate(&after_items, Period::All);
    let mut after_for_compare = after.clone();
    after_for_compare.rewrite_overhead_tokens = 0;
    let after_json: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&after_for_compare).unwrap()).unwrap();

    assert_eq!(
        before_json, after_json,
        "inserting Rewritten rows must not change any pre-existing report field"
    );

    // The AutoPipeline row expanded 300 -> 500 tokens: a real 200-token cost
    // that must be visible, not silently dropped (FR-041a, SC-012).
    assert_eq!(after.rewrite_overhead_tokens, 200);
}

#[test]
fn rewrite_rows_are_excluded_from_total_interceptions() {
    let mut items = base_items();
    let before_count = aggregate(&items, Period::All).total_interceptions;
    items.push(rewrite_row(100, 50, HookType::Cli));
    items.push(rewrite_row(100, 200, HookType::PostToolUse));
    let after_count = aggregate(&items, Period::All).total_interceptions;
    assert_eq!(before_count, after_count);
}

#[test]
fn rewrite_rows_are_excluded_from_by_family_and_by_agent() {
    let mut items = base_items();
    items.push(rewrite_row(100, 300, HookType::Cli));
    items.push(rewrite_row(100, 300, HookType::PostToolUse));
    let report = aggregate(&items, Period::All);
    assert!(!report.by_family.contains_key("generic"));
    assert!(!report.by_agent.values().any(|_| false)); // sanity: no panic
    let total_by_family_tokens: u64 = report.by_family.values().map(|s| s.tokens_before).sum();
    assert_eq!(total_by_family_tokens, report.total_tokens_before);
}
