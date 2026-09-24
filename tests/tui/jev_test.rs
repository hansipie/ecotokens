use ecotokens::jev::stats::{CallRow, JevSummary, PurposeStats};
use ecotokens::tui::jev::{render_jev, JevStatus};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

mod helpers;
use helpers::buffer_text;

fn draw(summary: &JevSummary, status: &JevStatus) -> String {
    let mut terminal = Terminal::new(TestBackend::new(140, 36)).unwrap();
    terminal
        .draw(|f| {
            render_jev(
                f,
                f.area(),
                summary,
                status,
                "all",
                Some("12:00:00"),
                Some(0),
                0,
            )
        })
        .unwrap();
    buffer_text(&terminal)
}

#[test]
fn empty_data_renders_a_message() {
    let text = draw(&JevSummary::default(), &JevStatus::default());
    assert!(text.contains("No Jev call recorded"), "{text}");
    assert!(text.contains("disabled"));
}

fn populated() -> JevSummary {
    let mut s = JevSummary {
        calls: 3,
        ok: 2,
        fallbacks: 1,
        avg_latency_ms: 120,
        p95_latency_ms: 300,
        input_tokens: 1234,
        output_tokens: 56,
        cost_usd: Some(0.0123),
        timeline: vec![1, 0, 2],
        ..Default::default()
    };
    s.by_purpose.insert(
        "filter_lines".into(),
        PurposeStats {
            calls: 3,
            ok: 2,
            input_tokens: 1234,
            output_tokens: 56,
            avg_latency_ms: 120,
        },
    );
    s.errors.insert("http 503".into(), 1);
    s.recent.push(CallRow {
        timestamp: "2026-09-24T10:11:12+00:00".into(),
        purpose: "filter_lines".into(),
        ok: false,
        error_kind: Some("http".into()),
        http_status: Some(503),
        latency_ms: 42,
        input_tokens: 0,
        output_tokens: 0,
        agent: None,
    });
    s.recent.push(CallRow {
        timestamp: "2026-09-24T10:12:13+00:00".into(),
        purpose: "router".into(),
        ok: true,
        error_kind: None,
        http_status: None,
        latency_ms: 30,
        input_tokens: 0,
        output_tokens: 0,
        agent: Some("router-everyday".into()),
    });
    s
}

#[test]
fn populated_data_shows_stats_failures_and_log() {
    let status = JevStatus {
        enabled: true,
        has_key: true,
        url: None,
    };
    let text = draw(&populated(), &status);
    for needle in [
        "enabled",
        "1,234",
        "$0.0123",
        "p95: 300 ms",
        "filter_lines",
        "http 503",
        "10:11:12",
        "FAIL",
        "-> router-everyday",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
}

#[test]
fn small_terminal_does_not_panic() {
    let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
    terminal
        .draw(|f| {
            render_jev(
                f,
                f.area(),
                &populated(),
                &JevStatus::default(),
                "all",
                None,
                None,
                9,
            )
        })
        .unwrap();
}
