use chrono::Utc;
use ecotokens::metrics::report::{aggregate, Period};
use ecotokens::metrics::store::{CommandFamily, FilterMode, Interception};
use ecotokens::tui::gain::render_gain;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
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
    )
}

// ── T034et ────────────────────────────────────────────────────────────────────

#[test]
fn gain_renders_savings_label() {
    let items = vec![
        make_interception(1000, 400, CommandFamily::Git),
        make_interception(2000, 800, CommandFamily::Cargo),
    ];
    let report = aggregate(&items, Period::All, "sonnet");
    let backend = TestBackend::new(100, 25);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_gain(frame, frame.area(), &report, &items))
        .unwrap();
    let content = buffer_text(&terminal);
    assert!(
        content.contains("Savings"),
        "buffer should contain 'Savings' label: {content:?}"
    );
}

#[test]
fn gain_renders_cost_avoided_label() {
    let items = vec![make_interception(5000, 1000, CommandFamily::Git)];
    let report = aggregate(&items, Period::All, "sonnet");
    let backend = TestBackend::new(100, 25);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_gain(frame, frame.area(), &report, &items))
        .unwrap();
    let content = buffer_text(&terminal);
    assert!(
        content.contains("Cost avoided"),
        "buffer should contain 'Cost avoided' label: {content:?}"
    );
}

#[test]
fn gain_renders_without_panic_on_empty_data() {
    let report = aggregate(&[], Period::All, "sonnet");
    let backend = TestBackend::new(100, 25);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_gain(frame, frame.area(), &report, &[]))
        .unwrap();
    let content = buffer_text(&terminal);
    assert!(!content.trim().is_empty(), "buffer should not be completely empty");
}

#[test]
fn gain_sparkline_present_for_14_days() {
    // One interception per day spread across the last 14 days
    let items: Vec<Interception> = (0i64..14)
        .map(|days_ago| {
            let mut item = make_interception(1000, 600, CommandFamily::Generic);
            item.timestamp =
                (Utc::now() - chrono::Duration::days(days_ago)).to_rfc3339();
            item
        })
        .collect();
    let report = aggregate(&items, Period::All, "sonnet");
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_gain(frame, frame.area(), &report, &items))
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
        .draw(|frame| render_gain(frame, frame.area(), &report, &items))
        .unwrap();
    let content = buffer_text(&terminal);
    let lower = content.to_lowercase();
    assert!(
        lower.contains("git") || lower.contains("cargo") || lower.contains("generic"),
        "buffer should contain a family name: {content:?}"
    );
}
