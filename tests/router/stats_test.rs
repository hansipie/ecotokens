use ecotokens::config::Settings;
use ecotokens::jev::Usage;
use ecotokens::router::stats::{cost_usd, record, summarize};
use ecotokens::router::{Decision, Routing, Size};
use tempfile::TempDir;

fn routing(decision: Decision, size: Option<Size>, tokens: Option<(u64, u64)>) -> Routing {
    Routing {
        decision,
        size,
        confidence: size.map(|_| 0.9),
        followup_prob: size.map(|_| 0.1),
        usage: tokens.map(|(i, o)| Usage {
            input_tokens: i,
            output_tokens: o,
        }),
        latency_ms: if tokens.is_some() { 2000 } else { 0 },
        error: None,
        service_failure: false,
    }
}

#[test]
fn summary_counts_sizes_decisions_and_tokens() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("router.db");
    let rows = [
        routing(Decision::Delegated, Some(Size::Tiny), Some((500, 60))),
        routing(Decision::Delegated, Some(Size::Tiny), Some((500, 60))),
        routing(Decision::SelfUnsure, Some(Size::Large), Some((500, 60))),
        routing(Decision::SelfFollowup, Some(Size::Tiny), Some((500, 60))),
        routing(Decision::Skipped, None, None),
        routing(Decision::Error, None, None),
    ];
    for r in &rows {
        record(&db, r).unwrap();
    }
    let s = summarize(&db, &Settings::default()).unwrap();
    assert_eq!(s.messages, 6);
    assert_eq!(s.by_size["tiny"].picked, 3);
    assert_eq!(s.by_size["tiny"].delegated, 2);
    assert_eq!(s.by_size["large"].picked, 1);
    assert_eq!(s.by_size["large"].delegated, 0);
    assert_eq!(s.by_size["hardest"].picked, 0);
    assert_eq!(s.by_decision["delegated"], 2);
    assert_eq!(s.by_decision["skipped"], 1);
    assert_eq!(s.jev_requests, 5);
    assert_eq!(s.input_tokens, 2000);
    assert_eq!(s.output_tokens, 240);
    assert_eq!(s.cost_usd, None);
}

#[test]
fn empty_store_reads_as_zero() {
    let dir = TempDir::new().unwrap();
    let s = summarize(&dir.path().join("none.db"), &Settings::default()).unwrap();
    assert_eq!(s.messages, 0);
    assert_eq!(s.by_size.len(), 4);
}

#[test]
fn cost_needs_a_configured_price() {
    let mut settings = Settings::default();
    assert_eq!(cost_usd(1_000_000, 1_000_000, &settings), None);
    settings.jev_usd_per_mtok_input = Some(0.5);
    settings.jev_usd_per_mtok_output = Some(2.0);
    let cost = cost_usd(2_000_000, 500_000, &settings).unwrap();
    assert!((cost - 2.0).abs() < 1e-9, "{cost}");
}
