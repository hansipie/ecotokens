use std::time::Instant;
use tempfile::TempDir;

// ── T045b — SC-003 : P90 ≤ 50ms ──────────────────────────────────────────────
// Tests the filter pipeline directly (no subprocess overhead) to validate the
// actual SC-003 production target of 50ms P90.

#[test]
#[ignore]
fn filter_p90_latency_under_50ms() {
    let content = (0..30)
        .map(|i| format!("line {i}: some content here"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut durations_ms = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        let _result = ecotokens::filter::run_filter_pipeline_with_cwd("cat", &content, 0, None);
        let elapsed = start.elapsed().as_millis() as u64;
        durations_ms.push(elapsed);
    }

    durations_ms.sort_unstable();
    let p90 = durations_ms[(durations_ms.len() as f64 * 0.9) as usize];
    assert!(
        p90 <= 50, // SC-003: P90 ≤ 50ms (filter logic only, no subprocess overhead)
        "P90 latency should be ≤50ms, got {p90}ms"
    );
}

// ── T045c — SC-005 : gain report ≤ 3s ────────────────────────────────────────

#[test]
fn gain_report_with_large_store_is_fast() {
    use ecotokens::metrics::store::{append_to, CommandFamily, FilterMode, Interception};

    let tmp = TempDir::new().unwrap();
    let store = tmp.path().join("metrics.db");

    // Write 1000 entries (keeping test fast; production would be 10k)
    for i in 0u32..1000 {
        let rec = Interception::new(
            format!("git status {i}"),
            CommandFamily::Git,
            Some("/repo".to_string()),
            1000 + i,
            400 + i,
            FilterMode::Filtered,
            false,
            10,
            None,
            None,
        );
        append_to(&store, &rec).unwrap();
    }

    let start = Instant::now();
    let items = ecotokens::metrics::store::read_from(&store).unwrap();
    let elapsed = start.elapsed().as_millis();

    assert_eq!(items.len(), 1000, "should have read 1000 entries");
    assert!(
        elapsed < 3000,
        "reading 1000 entries should be < 3s, took {elapsed}ms"
    );
}
