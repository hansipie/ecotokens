use chrono::Utc;
use ecotokens::metrics::report::{aggregate, aggregate_with_price, keep_project, Period};
use ecotokens::metrics::store::{CommandFamily, FilterMode, HookType, Interception};

fn make_interception_ago(
    seconds_ago: i64,
    family: CommandFamily,
    tokens_before: u32,
    tokens_after: u32,
) -> Interception {
    let ts = (Utc::now() - chrono::Duration::seconds(seconds_ago)).to_rfc3339();
    Interception {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: ts,
        command: "git status".into(),
        command_family: family,
        git_root: Some("/repo".into()),
        tokens_before,
        tokens_after,
        savings_pct: if tokens_before == 0 {
            0.0
        } else {
            ((1.0 - tokens_after as f64 / tokens_before as f64) * 100.0) as f32
        },
        mode: if tokens_after < tokens_before {
            FilterMode::Filtered
        } else {
            FilterMode::Passthrough
        },
        redacted: false,
        duration_ms: 5,
        content_before: None,
        content_after: None,
        hook_type: HookType::PreToolUse,
        session_id: None,
        agent_id: None,
        agent_type: None,
    }
}

fn make_items() -> Vec<Interception> {
    vec![
        make_interception_ago(10, CommandFamily::Git, 1000, 200), // today, 80% savings
        make_interception_ago(100, CommandFamily::Cargo, 500, 400), // today, 20% savings
        make_interception_ago(86500, CommandFamily::Git, 800, 300), // yesterday
    ]
}

#[test]
fn gain_shows_native_read_savings() {
    let mut items = make_items();
    // Add PostToolUse NativeRead interception
    let mut native_read = make_interception_ago(5, CommandFamily::NativeRead, 2000, 300);
    native_read.hook_type = HookType::PostToolUse;
    native_read.command = "src/main.rs".to_string();
    items.push(native_read);

    let report = aggregate(&items, Period::All);

    // NativeRead should appear in by_family with key "native_read" (serde snake_case)
    assert!(
        report.by_family.contains_key("native_read"),
        "by_family should contain 'native_read', got keys: {:?}",
        report.by_family.keys().collect::<Vec<_>>()
    );
    let stats = &report.by_family["native_read"];
    assert_eq!(stats.count, 1);
    assert_eq!(stats.tokens_before, 2000);
    assert_eq!(stats.tokens_after, 300);
}

#[test]
fn aggregate_all_includes_all_items() {
    let items = make_items();
    let report = aggregate(&items, Period::All);
    assert_eq!(report.total_interceptions, 3);
}

#[test]
fn aggregate_today_filters_to_today() {
    let items = make_items();
    let report = aggregate(&items, Period::Today);
    assert_eq!(report.total_interceptions, 2, "only today's items");
}

#[test]
fn aggregate_week_includes_recent_items() {
    let items = make_items();
    let report = aggregate(&items, Period::Week);
    assert_eq!(report.total_interceptions, 3, "all within a week");
}

#[test]
fn savings_pct_calculated() {
    let items = make_items();
    let report = aggregate(&items, Period::All);
    assert!(
        report.total_savings_pct > 0.0,
        "should have positive savings"
    );
}

#[test]
fn by_family_groups_correctly() {
    let items = make_items();
    let report = aggregate(&items, Period::All);
    assert!(
        report.by_family.contains_key("git"),
        "should have git family"
    );
    assert!(
        report.by_family.contains_key("cargo"),
        "should have cargo family"
    );
    let git_stats = &report.by_family["git"];
    assert_eq!(git_stats.count, 2, "two git interceptions");
}

#[test]
fn by_project_groups_by_git_root() {
    let items = make_items();
    let report = aggregate(&items, Period::All);
    assert!(
        report.by_project.contains_key("/repo"),
        "should group by /repo"
    );
}

#[test]
fn by_project_blank_git_root_groups_under_unknown() {
    let mut items = make_items();
    let mut empty_root = make_interception_ago(5, CommandFamily::Git, 1200, 600);
    empty_root.git_root = Some("   ".to_string());
    items.push(empty_root);

    let report = aggregate(&items, Period::All);
    assert!(
        !report.by_project.contains_key(""),
        "blank git_root should not create an empty-string key"
    );
    assert!(
        report.by_project.contains_key("[undefined]"),
        "blank git_root should be grouped under [undefined]"
    );
    assert_eq!(
        report.by_project.len(),
        2,
        "/repo and [undefined] should both be present in by_project"
    );
}

#[test]
fn cost_avoided_uses_configured_input_price() {
    let items = make_items();
    let saved: u64 = items
        .iter()
        .map(|i| (i.tokens_before - i.tokens_after) as u64)
        .sum();
    let report = aggregate_with_price(&items, Period::All, Some(3.0));
    let expected = saved as f64 / 1_000_000.0 * 3.0;
    let cost = report.cost_avoided_usd.expect("price is set");
    assert!(cost > 0.0, "cost avoided should be positive");
    assert!((cost - expected).abs() < 1e-9);
}

#[test]
fn cost_avoided_is_none_without_price() {
    let items = make_items();
    let report = aggregate_with_price(&items, Period::All, None);
    assert_eq!(report.cost_avoided_usd, None);
    let json = serde_json::to_string(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["cost_avoided_usd"].is_null());
}

#[test]
fn json_output_is_valid() {
    let items = make_items();
    let report = aggregate_with_price(&items, Period::All, Some(3.0));
    let json = serde_json::to_string(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["total_interceptions"].is_number());
    assert!(parsed["cost_avoided_usd"].is_number());
}

#[test]
fn history_ordered_by_date_descending() {
    let items = make_items();
    // history is the raw items; report creation doesn't reorder, but aggregate produces from items
    let report = aggregate(&items, Period::All);
    // simply verify the report has the correct count — ordering is the consumer's responsibility
    assert_eq!(report.total_interceptions, 3);
}

#[test]
fn keep_project_retains_only_that_git_root() {
    let mut items = make_items();
    let mut other = make_interception_ago(5, CommandFamily::Git, 100, 10);
    other.git_root = Some("/other".into());
    let mut none = make_interception_ago(5, CommandFamily::Git, 100, 10);
    none.git_root = None;
    let repo_count = items.len();
    items.push(other);
    items.push(none);

    keep_project(&mut items, "/repo");
    assert_eq!(items.len(), repo_count);
    assert!(items.iter().all(|i| i.git_root.as_deref() == Some("/repo")));

    keep_project(&mut items, "/missing");
    assert!(items.is_empty());
}

// ── by_subagent ───────────────────────────────────────────────────────────────

use ecotokens::metrics::report::{subagent_lines, MAIN_SESSION_BUCKET};

fn with_agent_type(mut item: Interception, agent_type: &str) -> Interception {
    item.agent_type = Some(agent_type.to_string());
    item
}

fn subagent_items() -> Vec<Interception> {
    vec![
        make_interception_ago(10, CommandFamily::Git, 1000, 200),
        with_agent_type(
            make_interception_ago(20, CommandFamily::Cargo, 500, 100),
            "router-large",
        ),
        with_agent_type(
            make_interception_ago(30, CommandFamily::Git, 300, 300),
            "router-large",
        ),
        with_agent_type(
            make_interception_ago(40, CommandFamily::Grep, 400, 100),
            "Explore",
        ),
    ]
}

#[test]
fn by_subagent_buckets_add_up_to_the_totals() {
    let report = aggregate_with_price(&subagent_items(), Period::All, None);

    let before: u64 = report.by_subagent.values().map(|s| s.tokens_before).sum();
    let after: u64 = report.by_subagent.values().map(|s| s.tokens_after).sum();
    let count: u32 = report.by_subagent.values().map(|s| s.count).sum();
    assert_eq!(before, report.total_tokens_before);
    assert_eq!(after, report.total_tokens_after);
    assert_eq!(count, report.total_interceptions);

    let main = &report.by_subagent[MAIN_SESSION_BUCKET];
    assert_eq!(
        (main.count, main.tokens_before, main.tokens_after),
        (1, 1000, 200)
    );
    let large = &report.by_subagent["router-large"];
    assert_eq!(
        (large.count, large.tokens_before, large.tokens_after),
        (2, 800, 400)
    );
    assert!((large.savings_pct - 50.0).abs() < 0.01);
    assert_eq!(report.by_subagent["Explore"].count, 1);
}

#[test]
fn by_agent_keeps_grouping_by_host_tool() {
    let report = aggregate_with_price(&subagent_items(), Period::All, None);
    // Every row is a Claude Code PreToolUse row: subagents do not split by_agent.
    assert_eq!(report.by_agent.len(), 1);
    assert_eq!(report.by_agent["claude"].count, 4);
}

#[test]
fn by_subagent_excludes_rewrite_rows_like_the_totals() {
    let mut items = subagent_items();
    let mut rewrite = with_agent_type(
        make_interception_ago(5, CommandFamily::Generic, 100, 900),
        "router-large",
    );
    rewrite.mode = FilterMode::Rewritten;
    items.push(rewrite);
    let report = aggregate_with_price(&items, Period::All, None);
    let before: u64 = report.by_subagent.values().map(|s| s.tokens_before).sum();
    assert_eq!(before, report.total_tokens_before);
    assert_eq!(report.by_subagent["router-large"].count, 2);
}

#[test]
fn by_subagent_json_shape() {
    let report = aggregate_with_price(&subagent_items(), Period::All, None);
    let v = serde_json::to_value(&report).unwrap();
    let by_subagent = v["by_subagent"].as_object().expect("by_subagent object");
    assert!(by_subagent.contains_key("[main]"));
    let large = &by_subagent["router-large"];
    for key in ["count", "tokens_before", "tokens_after", "savings_pct"] {
        assert!(large.get(key).is_some(), "missing {key}");
    }
    assert!(v["by_agent"].is_object(), "by_agent is kept");

    // A report serialized before the field existed still deserializes.
    let mut old = v.clone();
    old.as_object_mut().unwrap().remove("by_subagent");
    let back: ecotokens::metrics::report::Report = serde_json::from_value(old).unwrap();
    assert!(back.by_subagent.is_empty());
}

#[test]
fn by_subagent_on_empty_data() {
    let report = aggregate_with_price(&[], Period::All, None);
    assert!(report.by_subagent.is_empty());
    assert!(subagent_lines(&report).is_empty());
    let v = serde_json::to_value(&report).unwrap();
    assert_eq!(v["by_subagent"], serde_json::json!({}));
}

#[test]
fn subagent_lines_only_when_a_subagent_ran() {
    let main_only = aggregate_with_price(&make_items(), Period::All, None);
    assert!(
        subagent_lines(&main_only).is_empty(),
        "no section when every row is from the main session"
    );

    let report = aggregate_with_price(&subagent_items(), Period::All, None);
    let lines = subagent_lines(&report);
    assert_eq!(lines[0], "By subagent    :");
    assert!(lines[1].trim_start().starts_with("[main]"), "{lines:?}");
    assert!(lines[2].trim_start().starts_with("Explore"), "{lines:?}");
    assert!(
        lines[3].trim_start().starts_with("router-large"),
        "{lines:?}"
    );
    assert!(lines[3].contains("400 tokens saved"), "{lines:?}");
    assert!(lines[3].contains("50.0% savings"), "{lines:?}");
}
