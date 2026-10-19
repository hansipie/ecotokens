//! Measured usage of the router helpers: token totals split into input and
//! output, the measured-job counts, the cost totals, the per-tier prices and
//! the `router price --tier` command.

#[path = "common.rs"]
mod common;
use common::{read_config, run_ecotokens, stderr_of, stdout_of, temp_config_home, write_config};

use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use ecotokens::config::settings::{PriceSource, RouterTierPrices, TierPrice};
use ecotokens::config::Settings;
use ecotokens::router::cli::compact_count;
use ecotokens::router::outcome::{
    aggregates, attach_automatic, record_judgment, Judgment, Measurement,
};
use ecotokens::router::stats::record_at;
use ecotokens::router::{Decision, Routing, Size};
use serde_json::Value;
use tempfile::TempDir;

fn at(m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 18, 9, m, 0).unwrap()
}

fn delegated(route: &str, size: Size) -> Routing {
    let mut r = Routing::without_jev(Decision::Delegated);
    r.size = Some(size);
    r.route = Some(route.into());
    r.policy_version = Some("2026.1".into());
    r.session_id = Some("s".into());
    r
}

fn measured(tokens_in: i64, tokens_out: Option<i64>, cost: Option<f64>) -> Measurement {
    Measurement {
        cost_usd: cost,
        tokens_in: Some(tokens_in),
        tokens_out,
        duration_ms: Some(10_000),
        runs: Some(1),
    }
}

/// Two `large` jobs, one `everyday`, one `tiny`, in `db`.
fn record_jobs(db: &Path) -> Vec<i64> {
    [
        ("router-large", Size::Large),
        ("router-large", Size::Large),
        ("router-everyday", Size::Everyday),
        ("router-tiny", Size::Tiny),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (route, size))| record_at(db, &delegated(route, size), at(i as u32)).unwrap())
    .collect()
}

/// The jobs of [`record_jobs`]: both `large` jobs measured and priced, the
/// `everyday` job measured without a price, the `tiny` job never measured.
fn measure_jobs(db: &Path, ids: &[i64]) {
    attach_automatic(db, ids[0], &measured(30_000, Some(1_500), Some(0.20))).unwrap();
    attach_automatic(db, ids[1], &measured(11_000, Some(500), Some(0.10))).unwrap();
    attach_automatic(db, ids[2], &measured(2_500, Some(500), None)).unwrap();
}

fn db_with_usage() -> (TempDir, PathBuf, Vec<i64>) {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("router.db");
    let ids = record_jobs(&db);
    measure_jobs(&db, &ids);
    (dir, db, ids)
}

fn close(a: Option<f64>, b: f64) -> bool {
    a.is_some_and(|a| (a - b).abs() < 1e-9)
}

// ── Aggregates ───────────────────────────────────────────────────────────

#[test]
fn totals_cover_measured_jobs_and_split_input_and_output() {
    let (_d, db, _ids) = db_with_usage();
    let a = aggregates(&db, &Settings::default()).unwrap();

    let large = &a.by_route["router-large"];
    assert_eq!((large.measured, large.delegated, large.priced), (2, 2, 2));
    assert_eq!(large.tokens_in, Some(41_000));
    assert_eq!(large.tokens_out, Some(2_000));
    assert!(close(large.cost_usd, 0.30), "{:?}", large.cost_usd);
    assert_eq!(large.avg_tokens_in, Some(20_500.0));
    assert_eq!(large.avg_tokens_out, Some(1_000.0));
    assert_eq!(
        large.avg_tokens,
        Some(21_500.0),
        "the combined average keeps its meaning"
    );

    let everyday = &a.by_route["router-everyday"];
    assert_eq!((everyday.measured, everyday.priced), (1, 0));
    assert_eq!(everyday.tokens_in, Some(2_500));
    assert_eq!(everyday.cost_usd, None, "measured but not priced");

    let t = &a.totals;
    assert_eq!((t.delegated, t.measured, t.priced), (4, 3, 2));
    assert_eq!(t.tokens_in, Some(43_500));
    assert_eq!(t.tokens_out, Some(2_500));
    assert!(close(t.cost_usd, 0.30), "{:?}", t.cost_usd);
    assert_eq!(t.avg_tokens_in, Some(14_500.0));
}

#[test]
fn unmeasured_jobs_are_left_out_never_counted_as_zero() {
    let (_d, db, ids) = db_with_usage();
    // A judgment creates an outcome row, but measures nothing.
    record_judgment(
        &db,
        ids[3],
        &Judgment {
            success: Some(true),
            ..Judgment::default()
        },
    )
    .unwrap();
    let a = aggregates(&db, &Settings::default()).unwrap();
    let tiny = &a.by_route["router-tiny"];
    assert_eq!((tiny.delegated, tiny.measured, tiny.priced), (1, 0, 0));
    assert_eq!(tiny.tokens_in, None, "unknown, not 0");
    assert_eq!(tiny.tokens_out, None);
    assert_eq!(tiny.cost_usd, None);
    assert_eq!(tiny.avg_tokens_in, None);
    // The averages of all routes divide by the measured jobs only.
    assert_eq!(a.totals.measured, 3);
    assert_eq!(a.totals.avg_tokens_in, Some(43_500.0 / 3.0));
}

#[test]
fn a_job_with_only_a_total_has_no_output_figure() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("router.db");
    let id = record_at(&db, &delegated("router-large", Size::Large), at(0)).unwrap();
    attach_automatic(&db, id, &measured(5_000, None, None)).unwrap();
    let a = aggregates(&db, &Settings::default()).unwrap();
    let large = &a.by_route["router-large"];
    assert_eq!(large.measured, 1);
    assert_eq!(large.tokens_in, Some(5_000));
    assert_eq!(large.tokens_out, None);
    assert_eq!(large.avg_tokens_out, None);
}

#[test]
fn an_empty_store_has_zero_counts_and_no_figures() {
    let dir = TempDir::new().unwrap();
    let a = aggregates(&dir.path().join("missing.db"), &Settings::default()).unwrap();
    assert_eq!((a.totals.delegated, a.totals.measured), (0, 0));
    assert_eq!(a.totals.tokens_in, None);
    assert_eq!(a.totals.cost_usd, None);
}

#[test]
fn compact_counts_are_short() {
    assert_eq!(compact_count(0.0), "0");
    assert_eq!(compact_count(950.0), "950");
    assert_eq!(compact_count(41_000.0), "41.0k");
    assert_eq!(compact_count(2_000.0), "2.0k");
    assert_eq!(compact_count(999_999.0), "1.0M");
    assert_eq!(compact_count(1_234_567.0), "1.2M");
}

// ── Prices ───────────────────────────────────────────────────────────────

#[test]
fn tier_prices_are_unset_by_default_and_fall_back_part_by_part() {
    let defaults = Settings::default();
    assert_eq!(defaults.router_tier_prices, RouterTierPrices::default());
    let p = defaults.effective_tier_price(Some("large"));
    assert_eq!((p.input, p.output), (None, None));
    assert_eq!(p.input_source, PriceSource::Unset);
    assert!(!p.is_priced());

    let mut s = Settings {
        price_input_usd_per_mtok: Some(3.0),
        price_output_usd_per_mtok: Some(15.0),
        ..Settings::default()
    };
    s.router_tier_prices.large = TierPrice {
        input_usd_per_mtok: Some(15.0),
        output_usd_per_mtok: None,
    };
    let p = s.effective_tier_price(Some("large"));
    assert_eq!((p.input, p.output), (Some(15.0), Some(15.0)));
    assert_eq!(
        (p.input_source, p.output_source),
        (PriceSource::Tier, PriceSource::Global)
    );
    let p = s.effective_tier_price(Some("tiny"));
    assert_eq!((p.input, p.output), (Some(3.0), Some(15.0)));
    assert_eq!(p.input_source, PriceSource::Global);
    // A helper of no known tier uses the global price.
    let p = s.effective_tier_price(None);
    assert_eq!((p.input, p.output), (Some(3.0), Some(15.0)));
}

#[test]
fn tier_prices_round_trip_through_the_settings_file() {
    let json = serde_json::json!({
        "router_tier_prices": { "tiny": { "input_usd_per_mtok": 1.0, "output_usd_per_mtok": 5.0 } }
    });
    let s: Settings = serde_json::from_value(json).unwrap();
    assert_eq!(s.router_tier_prices.tiny.input_usd_per_mtok, Some(1.0));
    assert_eq!(s.router_tier_prices.large, TierPrice::default());
    let back = serde_json::to_value(&s).unwrap();
    assert_eq!(
        back["router_tier_prices"]["tiny"]["output_usd_per_mtok"],
        5.0
    );
    assert_eq!(
        back["router_tier_prices"]["large"],
        serde_json::json!({}),
        "an unset tier writes no price"
    );
}

// ── CLI ──────────────────────────────────────────────────────────────────

fn run(home: &common::ConfigHome, args: &[&str]) -> std::process::Output {
    run_ecotokens(home, args, None, None)
}

fn json_of(out: &std::process::Output) -> Value {
    serde_json::from_str(&stdout_of(out)).unwrap_or_else(|e| panic!("{e}: {}", stdout_of(out)))
}

fn seeded_home() -> common::ConfigHome {
    let home = temp_config_home();
    let db = home.router_db();
    let ids = record_jobs(&db);
    measure_jobs(&db, &ids);
    home
}

#[test]
fn status_json_adds_the_usage_figures_and_keeps_the_old_ones() {
    let home = seeded_home();
    let out = run(&home, &["router", "status", "--json"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let v = json_of(&out);

    let large = &v["by_route"]["router-large"];
    for key in [
        // Existing keys, unchanged.
        "delegated",
        "judged",
        "judged_by_reviewer",
        "success_rate",
        "success_rate_manual",
        "avg_tokens",
        "avg_cost_usd",
        "avg_duration_ms",
        "avg_runs",
        "correction",
        "regressions",
        // New keys.
        "measured",
        "priced",
        "tokens_in",
        "tokens_out",
        "cost_usd",
        "avg_tokens_in",
        "avg_tokens_out",
    ] {
        assert!(large.get(key).is_some(), "by_route key `{key}` missing");
    }
    assert_eq!(large["avg_tokens"], 21_500.0);
    assert_eq!(large["tokens_in"], 41_000);
    assert_eq!(large["tokens_out"], 2_000);
    assert_eq!(large["measured"], 2);
    assert_eq!(large["delegated"], 2);
    assert!(close(large["cost_usd"].as_f64(), 0.30));

    let tiny = &v["by_route"]["router-tiny"];
    assert_eq!(tiny["measured"], 0);
    assert_eq!(tiny["tokens_in"], Value::Null);
    assert_eq!(tiny["cost_usd"], Value::Null);

    let totals = &v["totals"];
    assert_eq!(totals["delegated"], 4);
    assert_eq!(totals["measured"], 3);
    assert_eq!(totals["priced"], 2);
    assert_eq!(totals["tokens_in"], 43_500);
    assert_eq!(totals["tokens_out"], 2_500);
    assert!(close(totals["cost_usd"].as_f64(), 0.30));
    // The category figures carry the same fields.
    assert_eq!(v["by_category"]["unknown"]["tokens_in"], 43_500);

    // Prices: nothing set, nothing invented.
    let prices = &v["prices"];
    assert_eq!(prices["global"]["input_usd_per_mtok"], Value::Null);
    for tier in ["tiny", "everyday", "large", "hardest"] {
        assert_eq!(prices["tiers"][tier]["input_usd_per_mtok"], Value::Null);
        assert_eq!(prices["tiers"][tier]["input_source"], "unset");
    }
}

#[test]
fn status_text_shows_totals_measured_counts_and_the_in_out_split() {
    let home = seeded_home();
    let out = run(&home, &["router", "status"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let text = stdout_of(&out);
    assert!(text.contains("avg in→out"), "{text}");
    assert!(text.contains("20.5k→1.0k"), "{text}");
    assert!(text.contains("usage (totals)"), "{text}");
    let line = |prefix: &str| {
        text.lines()
            .rev()
            .find(|l| l.starts_with(prefix))
            .unwrap_or_else(|| panic!("no `{prefix}` line: {text}"))
            .to_string()
    };
    let large = line("router-large ");
    assert!(
        large.contains("2/2") && large.contains("41.0k") && large.contains("2.0k"),
        "{large}"
    );
    assert!(large.contains("$0.3000"), "{large}");
    let tiny = line("router-tiny ");
    assert!(tiny.contains("0/1") && tiny.contains('-'), "{tiny}");
    let total = line("total ");
    assert!(
        total.contains("3/4") && total.contains("43.5k") && total.contains("2.5k"),
        "{total}"
    );
    assert!(
        total.contains("$0.3000*"),
        "fewer priced than measured: {total}"
    );
    assert!(
        text.contains("measured from the session transcripts"),
        "{text}"
    );
    assert!(text.contains("No saving against a bigger model"), "{text}");
}

#[test]
fn status_on_an_empty_store_has_the_usage_keys() {
    let home = temp_config_home();
    let out = run(&home, &["router", "status", "--json"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let v = json_of(&out);
    assert_eq!(v["totals"]["delegated"], 0);
    assert_eq!(v["totals"]["measured"], 0);
    assert_eq!(v["totals"]["tokens_in"], Value::Null);
    assert_eq!(v["totals"]["cost_usd"], Value::Null);
    assert_eq!(v["prices"]["tiers"]["large"]["output_source"], "unset");
    let out = run(&home, &["router", "status"]);
    assert!(stdout_of(&out).contains("No messages routed yet"));
}

#[test]
fn price_with_a_tier_sets_that_tier_and_leaves_the_jev_price_alone() {
    let home = temp_config_home();
    write_config(
        &home,
        &serde_json::json!({
            "jev_usd_per_mtok_input": 0.5,
            "price_input_usd_per_mtok": 3.0,
            "price_output_usd_per_mtok": 15.0,
        }),
    );
    let out = run(
        &home,
        &[
            "router", "price", "--tier", "large", "--input", "15", "--output", "75",
        ],
    );
    assert!(out.status.success(), "{}", stderr_of(&out));
    let text = stdout_of(&out);
    assert!(text.contains("large helper price"), "{text}");
    assert!(text.contains("$15") && text.contains("$75"), "{text}");
    assert!(text.contains("not recomputed"), "{text}");
    let cfg = read_config(&home);
    assert_eq!(
        cfg["router_tier_prices"]["large"]["input_usd_per_mtok"],
        15.0
    );
    assert_eq!(
        cfg["router_tier_prices"]["large"]["output_usd_per_mtok"],
        75.0
    );
    assert_eq!(
        cfg["jev_usd_per_mtok_input"], 0.5,
        "the Jev price is untouched"
    );
    assert_eq!(cfg["price_input_usd_per_mtok"], 3.0);

    // One part at a time; the other keeps its value.
    let out = run(
        &home,
        &["router", "price", "--tier", "large", "--output", "80"],
    );
    assert!(out.status.success(), "{}", stderr_of(&out));
    let cfg = read_config(&home);
    assert_eq!(
        cfg["router_tier_prices"]["large"]["input_usd_per_mtok"],
        15.0
    );
    assert_eq!(
        cfg["router_tier_prices"]["large"]["output_usd_per_mtok"],
        80.0
    );

    // Showing a tier without a price of its own names the global fallback.
    let out = run(&home, &["router", "price", "--tier", "tiny"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let text = stdout_of(&out);
    assert!(
        text.contains("$3 (global)") && text.contains("$15 (global)"),
        "{text}"
    );
    assert!(!text.contains("not recomputed"), "nothing changed: {text}");

    // status --json reports the applied prices and their source.
    let v = json_of(&run(&home, &["router", "status", "--json"]));
    let large = &v["prices"]["tiers"]["large"];
    assert_eq!(large["input_usd_per_mtok"], 15.0);
    assert_eq!(large["input_source"], "tier");
    let tiny = &v["prices"]["tiers"]["tiny"];
    assert_eq!(tiny["output_usd_per_mtok"], 15.0);
    assert_eq!(tiny["output_source"], "global");

    // --clear unsets the tier: the global price applies again.
    let out = run(&home, &["router", "price", "--tier", "large", "--clear"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("$3 (global)"));
    let cfg = read_config(&home);
    assert_eq!(cfg["router_tier_prices"]["large"], serde_json::json!({}));
}

#[test]
fn price_without_a_tier_still_sets_the_jev_price() {
    let home = temp_config_home();
    let out = run(&home, &["router", "price", "--input", "1", "--output", "2"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Jev price: input $1 · output $2"));
    let cfg = read_config(&home);
    assert_eq!(cfg["jev_usd_per_mtok_input"], 1.0);
    assert_eq!(cfg["jev_usd_per_mtok_output"], 2.0);
    assert_eq!(cfg["router_tier_prices"]["large"], serde_json::json!({}));
}

#[test]
fn price_rejects_bad_tiers_and_values() {
    let home = temp_config_home();
    for args in [
        vec!["router", "price", "--tier", "huge", "--input", "1"],
        vec!["router", "price", "--tier", "large", "--input=-1"],
        vec!["router", "price", "--tier", "large", "--output", "NaN"],
        vec!["router", "price", "--clear"],
        vec![
            "router", "price", "--tier", "large", "--clear", "--input", "1",
        ],
    ] {
        let out = run(&home, &args);
        assert!(!out.status.success(), "{args:?} should fail");
    }
    assert!(
        !home.ecotokens_dir().join("config.json").exists()
            || read_config(&home)["router_tier_prices"]["large"] == serde_json::json!({}),
        "nothing was written"
    );
}

#[test]
fn a_price_change_does_not_recompute_stored_costs() {
    let home = seeded_home();
    let before = json_of(&run(&home, &["router", "status", "--json"]));
    let out = run(
        &home,
        &[
            "router", "price", "--tier", "large", "--input", "100", "--output", "500",
        ],
    );
    assert!(out.status.success(), "{}", stderr_of(&out));
    let after = json_of(&run(&home, &["router", "status", "--json"]));
    assert_eq!(
        before["by_route"]["router-large"]["cost_usd"],
        after["by_route"]["router-large"]["cost_usd"]
    );
    assert_eq!(
        after["by_route"]["router-everyday"]["cost_usd"],
        Value::Null,
        "a job stored without a cost stays without one"
    );
}
