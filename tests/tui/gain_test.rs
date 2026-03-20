use chrono::Utc;
use ecotokens::metrics::report::{aggregate, aggregate_history, Period};
use ecotokens::metrics::store::{CommandFamily, FilterMode, Interception};
use ecotokens::tui::gain::{render_gain, DetailMode, GainMode};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

mod helpers;
use helpers::buffer_text;

fn draw_gain(
    items: &[Interception],
    width: u16,
    height: u16,
    gain_mode: GainMode,
    selected_family: Option<usize>,
    detail_mode: DetailMode,
    selected_project: Option<usize>,
) -> String {
    let report = aggregate(items, Period::All, "sonnet");
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_gain(
                frame,
                frame.area(),
                &report,
                items,
                None,
                gain_mode,
                Default::default(),
                selected_family,
                detail_mode,
                selected_project,
                None,
                0,
            )
        })
        .unwrap();
    buffer_text(&terminal)
}

fn make_interception(tokens_before: u32, tokens_after: u32, family: CommandFamily) -> Interception {
    Interception::new(
        "git status".to_string(),
        family,
        Some("/home/user/project".to_string()),
        tokens_before,
        tokens_after,
        FilterMode::Filtered,
        false,
        10,
        None,
        None,
    )
}

// ── aggregate_history unit tests ──────────────────────────────────────────────

#[test]
fn aggregate_history_empty_returns_zeros() {
    let report = aggregate_history(&[], "sonnet");
    assert_eq!(report.day.total_interceptions, 0);
    assert_eq!(report.week.total_interceptions, 0);
    assert_eq!(report.month.total_interceptions, 0);
    assert_eq!(report.day.cost_avoided_usd, 0.0);
    assert_eq!(report.model_ref, "sonnet");
}

#[test]
fn aggregate_history_json_has_expected_keys() {
    let report = aggregate_history(&[], "sonnet");
    let json = serde_json::to_string(&report).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(v["day"].is_object(), "should have 'day' key: {json}");
    assert!(v["week"].is_object(), "should have 'week' key: {json}");
    assert!(v["month"].is_object(), "should have 'month' key: {json}");
    assert_eq!(v["model_ref"], "sonnet");
}

#[test]
fn aggregate_history_counts_by_period() {
    let mut item_recent = make_interception(1000, 400, CommandFamily::Git);
    item_recent.timestamp = (Utc::now() - chrono::Duration::days(3)).to_rfc3339();

    let mut item_old = make_interception(2000, 800, CommandFamily::Git);
    item_old.timestamp = (Utc::now() - chrono::Duration::days(20)).to_rfc3339();

    let items = vec![item_recent, item_old];
    let report = aggregate_history(&items, "sonnet");

    assert_eq!(
        report.month.total_interceptions, 2,
        "month should include both items"
    );
    assert_eq!(
        report.week.total_interceptions, 1,
        "week should include only item from 3 days ago"
    );
    assert_eq!(
        report.day.total_interceptions, 0,
        "day should not include items older than midnight"
    );
}

// ── T034et ────────────────────────────────────────────────────────────────────

#[test]
fn gain_renders_savings_label() {
    let items = vec![
        make_interception(1000, 400, CommandFamily::Git),
        make_interception(2000, 800, CommandFamily::Cargo),
    ];
    let content = draw_gain(
        &items,
        100,
        25,
        GainMode::Family,
        None,
        Default::default(),
        None,
    );
    assert!(
        content.contains("Savings"),
        "buffer should contain 'Savings' label: {content:?}"
    );
}

#[test]
fn gain_renders_cost_avoided_label() {
    let items = vec![make_interception(5000, 1000, CommandFamily::Git)];
    let content = draw_gain(
        &items,
        100,
        25,
        GainMode::Family,
        None,
        Default::default(),
        None,
    );
    assert!(
        content.contains("Cost avoided"),
        "buffer should contain 'Cost avoided' label: {content:?}"
    );
}

#[test]
fn gain_renders_since_label() {
    let items = vec![make_interception(5000, 1000, CommandFamily::Git)];
    let content = draw_gain(
        &items,
        100,
        25,
        GainMode::Family,
        None,
        Default::default(),
        None,
    );
    assert!(
        content.contains("Since"),
        "buffer should contain 'Since' label: {content:?}"
    );
}

#[test]
fn gain_renders_without_panic_on_empty_data() {
    let report = aggregate(&[], Period::All, "sonnet");
    let backend = TestBackend::new(100, 25);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_gain(
                frame,
                frame.area(),
                &report,
                &[],
                None,
                GainMode::Family,
                Default::default(),
                None,
                Default::default(),
                None,
                None,
                0,
            )
        })
        .unwrap();
    let content = buffer_text(&terminal);
    assert!(
        !content.trim().is_empty(),
        "buffer should not be completely empty"
    );
}

#[test]
fn gain_sparkline_present_adaptive() {
    // One interception per day spread across the last 14 days
    let items: Vec<Interception> = (0i64..14)
        .map(|days_ago| {
            let mut item = make_interception(1000, 600, CommandFamily::Generic);
            item.timestamp = (Utc::now() - chrono::Duration::days(days_ago)).to_rfc3339();
            item
        })
        .collect();
    let report = aggregate(&items, Period::All, "sonnet");
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_gain(
                frame,
                frame.area(),
                &report,
                &items,
                None,
                GainMode::Family,
                Default::default(),
                None,
                Default::default(),
                None,
                None,
                0,
            )
        })
        .unwrap();
    let content = buffer_text(&terminal);
    assert!(
        content.contains("Savings"),
        "sparkline block title should be present: {content:?}"
    );
}

#[test]
fn gain_shows_family_breakdown() {
    let items = vec![
        make_interception(1000, 300, CommandFamily::Git),
        make_interception(2000, 500, CommandFamily::Cargo),
        make_interception(500, 400, CommandFamily::Generic),
    ];
    let report = aggregate(&items, Period::All, "sonnet");
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_gain(
                frame,
                frame.area(),
                &report,
                &items,
                None,
                GainMode::Family,
                Default::default(),
                None,
                Default::default(),
                None,
                None,
                0,
            )
        })
        .unwrap();
    let content = buffer_text(&terminal);
    let lower = content.to_lowercase();
    assert!(
        lower.contains("git") || lower.contains("cargo") || lower.contains("generic"),
        "buffer should contain a family name: {content:?}"
    );
}

#[test]
fn gain_detail_no_content_shows_fallback() {
    let items = vec![make_interception(1000, 400, CommandFamily::Git)];
    let report = aggregate(&items, Period::All, "sonnet");
    let backend = TestBackend::new(120, 35);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_gain(
                frame,
                frame.area(),
                &report,
                &items,
                None,
                GainMode::Family,
                Default::default(),
                Some(0),
                Default::default(),
                None,
                None,
                0,
            )
        })
        .unwrap();
    let content = buffer_text(&terminal);
    assert!(!content.trim().is_empty(), "buffer should not be empty");
    // No panic — fallback message shown for items without content
}

#[test]
fn gain_detail_with_content_renders_text() {
    let mut item = make_interception(1000, 400, CommandFamily::Git);
    item.content_before = Some("diff --git a/foo.rs b/foo.rs".to_string());
    item.content_after = Some("summary: 1 file changed".to_string());
    let items = vec![item];
    let content = draw_gain(
        &items,
        120,
        35,
        GainMode::Family,
        Some(0),
        Default::default(),
        None,
    );
    assert!(
        content.contains("diff") || content.contains("summary") || content.contains("foo"),
        "detail panel should render content text: {content:?}"
    );
}

#[test]
fn gain_diff_mode_renders_diff_markers() {
    let mut item = make_interception(1000, 400, CommandFamily::Git);
    item.content_before = Some("line one\nline two\nline three\n".to_string());
    item.content_after = Some("line one\nline TWO\nline three\n".to_string());
    let items = vec![item];
    let report = aggregate(&items, Period::All, "sonnet");
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_gain(
                frame,
                frame.area(),
                &report,
                &items,
                None,
                GainMode::Family,
                Default::default(),
                Some(0),
                DetailMode::Diff,
                None,
                None,
                0,
            )
        })
        .unwrap();
    let content = buffer_text(&terminal);
    // In diff mode the panel title should contain "Diff"
    assert!(
        content.contains("Diff"),
        "diff mode should show 'Diff' in panel title: {content:?}"
    );
}

#[test]
fn gain_log_mode_renders_history() {
    let items: Vec<Interception> = (0..5)
        .map(|_| make_interception(1000, 400, CommandFamily::Git))
        .collect();
    let report = aggregate(&items, Period::All, "sonnet");
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_gain(
                frame,
                frame.area(),
                &report,
                &items,
                None,
                GainMode::Family,
                Default::default(),
                Some(0),
                DetailMode::Log,
                None,
                None,
                0,
            )
        })
        .unwrap();
    let content = buffer_text(&terminal);
    assert!(
        content.contains("History"),
        "log mode should show 'History' in panel title: {content:?}"
    );
}

#[test]
fn gain_selected_ignored_in_by_project_mode() {
    let items = vec![make_interception(1000, 400, CommandFamily::Git)];
    // Must not panic with selected_family=Some(0) in by_project mode
    draw_gain(
        &items,
        120,
        35,
        GainMode::Project,
        Some(0),
        Default::default(),
        None,
    );
}

#[test]
fn gain_project_log_mode_renders_history() {
    let items: Vec<Interception> = (0..5)
        .map(|_| {
            let mut item = make_interception(1000, 400, CommandFamily::Git);
            item.git_root = Some("/home/user/proj".to_string());
            item
        })
        .collect();
    let report = aggregate(&items, Period::All, "sonnet");
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_gain(
                frame,
                frame.area(),
                &report,
                &items,
                None,
                GainMode::Project,
                Default::default(),
                None,
                Default::default(),
                Some(0),
                None,
                0,
            )
        })
        .unwrap();
    let content = buffer_text(&terminal);
    assert!(
        content.contains("history") || content.contains("History"),
        "project log mode should show 'Project history' in panel title: {content:?}"
    );
}

#[test]
fn gain_project_history_panel_refreshes_between_draws() {
    let mut item1 = make_interception(1000, 400, CommandFamily::Git);
    item1.git_root = Some("/home/user/proj-refresh".to_string());
    item1.command = "git status".to_string();

    let mut item2 = make_interception(1000, 300, CommandFamily::Git);
    item2.git_root = Some("/home/user/proj-refresh".to_string());
    item2.command = "git log -n 1".to_string();

    let items_first = vec![item1.clone()];
    let report_first = aggregate(&items_first, Period::All, "sonnet");

    let backend = TestBackend::new(140, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render_gain(
                frame,
                frame.area(),
                &report_first,
                &items_first,
                None,
                GainMode::Project,
                Default::default(),
                None,
                Default::default(),
                Some(0),
                None,
                0,
            )
        })
        .unwrap();

    let first_content = buffer_text(&terminal);
    assert!(
        first_content.contains("git status"),
        "first draw should include first command, got: {first_content:?}"
    );

    let items_second = vec![item1, item2];
    let report_second = aggregate(&items_second, Period::All, "sonnet");
    terminal
        .draw(|frame| {
            render_gain(
                frame,
                frame.area(),
                &report_second,
                &items_second,
                None,
                GainMode::Project,
                Default::default(),
                None,
                Default::default(),
                Some(0),
                None,
                0,
            )
        })
        .unwrap();

    let second_content = buffer_text(&terminal);
    assert!(
        second_content.contains("2 entries"),
        "second draw should show updated entry count, got: {second_content:?}"
    );
    assert!(
        second_content.contains("git log -n 1") || second_content.contains("git log"),
        "second draw should include newest command, got: {second_content:?}"
    );
}
