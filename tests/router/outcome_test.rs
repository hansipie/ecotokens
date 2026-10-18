#[path = "common.rs"]
mod common;
use common::{run_ecotokens, stderr_of, stdout_of, temp_config_home, ConfigHome};

use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use ecotokens::config::Settings;
use ecotokens::router::card::{Graded, Nature, Risk, TaskCard};
use ecotokens::router::outcome::{
    aggregates, attach_automatic, pending, record_judgment, Correction, Judgment, JudgmentError,
    Measurement,
};
use ecotokens::router::stats::{record_at, record_returning_id, summarize};
use ecotokens::router::{Decision, Routing, Size};
use rusqlite::Connection;
use serde_json::Value;
use tempfile::TempDir;

fn at(m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 17, 10, m, 0).unwrap()
}

fn card(nature: Nature) -> TaskCard {
    TaskCard {
        size: Size::Large,
        size_confidence: 0.9,
        nature: Some(Graded {
            value: nature,
            confidence: 0.9,
        }),
        risk: Some(Graded {
            value: Risk::Reversible,
            confidence: 0.9,
        }),
        ambiguity: Some(0.1),
        verifiability: Some(0.9),
    }
}

fn delegated(route: &str, nature: Nature, policy: &str) -> Routing {
    let mut r = Routing::without_jev(Decision::Delegated);
    r.size = Some(Size::Large);
    r.route = Some(route.into());
    r.rule = Some("bounded-bug".into());
    r.policy_version = Some(policy.into());
    r.session_id = Some("s".into());
    r.card = Some(card(nature));
    r
}

fn db_with_jobs() -> (TempDir, PathBuf, Vec<i64>) {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("router.db");
    let ids = vec![
        record_at(
            &db,
            &delegated("router-large", Nature::Bug, "2026.1"),
            at(0),
        )
        .unwrap(),
        record_at(
            &db,
            &delegated("router-large", Nature::Bug, "2026.1"),
            at(1),
        )
        .unwrap(),
        record_at(
            &db,
            &delegated("router-everyday", Nature::Ui, "2026.1"),
            at(2),
        )
        .unwrap(),
        record_at(
            &db,
            &delegated("router-tiny", Nature::Mechanical, "2026.1+uabcd1234"),
            at(3),
        )
        .unwrap(),
    ];
    (dir, db, ids)
}

fn judgment(
    success: Option<bool>,
    correction: Option<Correction>,
    regressions: Option<i64>,
) -> Judgment {
    Judgment {
        success,
        correction,
        regressions,
    }
}

fn measurement(
    tokens_in: i64,
    tokens_out: Option<i64>,
    cost: Option<f64>,
    ms: i64,
    runs: i64,
) -> Measurement {
    Measurement {
        cost_usd: cost,
        tokens_in: Some(tokens_in),
        tokens_out,
        duration_ms: Some(ms),
        runs: Some(runs),
    }
}

#[test]
fn judgments_are_idempotent_and_later_values_overwrite_earlier_ones() {
    let (_d, db, ids) = db_with_jobs();
    let id = ids[0];
    record_judgment(
        &db,
        id,
        &judgment(Some(false), Some(Correction::Major), Some(3)),
    )
    .unwrap();
    record_judgment(
        &db,
        id,
        &judgment(Some(false), Some(Correction::Major), Some(3)),
    )
    .unwrap();
    let a = aggregates(&db, &Settings::default()).unwrap();
    assert_eq!(
        a.by_route["router-large"].judged, 1,
        "judging twice counts once"
    );

    // A later judgment replaces the earlier one, field by field.
    record_judgment(&db, id, &judgment(Some(true), None, None)).unwrap();
    let a = aggregates(&db, &Settings::default()).unwrap();
    let large = &a.by_route["router-large"];
    assert_eq!(large.success_rate, Some(1.0));
    assert_eq!(
        large.correction.major, 1,
        "a field not given keeps its value"
    );
    assert_eq!(large.regressions, 3);
    record_judgment(&db, id, &judgment(None, Some(Correction::None), Some(0))).unwrap();
    let a = aggregates(&db, &Settings::default()).unwrap();
    assert_eq!(a.by_route["router-large"].correction.none, 1);
    assert_eq!(a.by_route["router-large"].correction.major, 0);
    assert_eq!(a.by_route["router-large"].regressions, 0);
}

type Row = (
    Option<f64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
);

#[test]
fn automatic_columns_and_manual_columns_never_overwrite_each_other() {
    let (_d, db, ids) = db_with_jobs();
    let id = ids[0];
    attach_automatic(&db, id, &measurement(1000, Some(200), Some(0.5), 5000, 2)).unwrap();
    record_judgment(
        &db,
        id,
        &judgment(Some(true), Some(Correction::Minor), Some(1)),
    )
    .unwrap();
    let conn = Connection::open(&db).unwrap();
    let row = |conn: &Connection| -> Row {
        conn.query_row(
            "SELECT cost_usd, tokens_in, duration_ms, runs, success FROM router_outcomes WHERE decision_id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap()
    };
    assert_eq!(
        row(&conn),
        (Some(0.5), Some(1000), Some(5000), Some(2), Some(1))
    );

    // A judgment leaves the measurements alone…
    record_judgment(&db, id, &judgment(Some(false), None, None)).unwrap();
    assert_eq!(
        row(&conn),
        (Some(0.5), Some(1000), Some(5000), Some(2), Some(0))
    );
    // …and a new measurement leaves the judgment alone.
    attach_automatic(&db, id, &measurement(2000, Some(300), Some(0.9), 9000, 3)).unwrap();
    assert_eq!(
        row(&conn),
        (Some(0.9), Some(2000), Some(9000), Some(3), Some(0))
    );
}

#[test]
fn an_unknown_id_is_an_actionable_error() {
    let (_d, db, ids) = db_with_jobs();
    let err = record_judgment(&db, 9999, &judgment(Some(true), None, None)).unwrap_err();
    assert!(matches!(err, JudgmentError::UnknownJob(9999)), "{err:?}");
    // A decision that was not delegated cannot be judged either.
    let skipped = record_returning_id(&db, &Routing::without_jev(Decision::SelfUnsure)).unwrap();
    assert!(matches!(
        record_judgment(&db, skipped, &judgment(Some(true), None, None)),
        Err(JudgmentError::UnknownJob(_))
    ));
    let _ = ids;
}

#[test]
fn pending_lists_what_the_reminder_promises_and_no_message_text() {
    let (_d, db, ids) = db_with_jobs();
    attach_automatic(
        &db,
        ids[0],
        &measurement(1000, Some(200), Some(0.5), 5000, 2),
    )
    .unwrap();
    record_judgment(&db, ids[1], &judgment(Some(true), None, None)).unwrap();
    let list = pending(&db).unwrap();
    let listed: Vec<i64> = list.iter().map(|p| p.id).collect();
    assert_eq!(listed, [ids[0], ids[2], ids[3]], "the judged job is gone");
    let first = &list[0];
    assert_eq!(first.route, "router-large");
    assert_eq!(first.category.as_deref(), Some("bug"));
    assert_eq!(first.cost_usd, Some(0.5));
    assert_eq!(first.duration_ms, Some(5000));
    assert!(first.date.starts_with("2026-10-17"));
    assert_eq!(list[1].cost_usd, None, "unmeasured jobs show unknown");
}

#[test]
fn success_rates_leave_out_unknown_outcomes() {
    let (_d, db, ids) = db_with_jobs();
    let a = aggregates(&db, &Settings::default()).unwrap();
    assert_eq!(
        a.by_route["router-large"].success_rate, None,
        "nothing judged yet: null, not 0"
    );
    assert_eq!((a.outcomes.unknown, a.outcomes.judged), (4, 0));

    record_judgment(&db, ids[0], &judgment(Some(true), None, None)).unwrap();
    record_judgment(&db, ids[1], &judgment(Some(false), None, None)).unwrap();
    let a = aggregates(&db, &Settings::default()).unwrap();
    assert_eq!(a.by_route["router-large"].success_rate, Some(0.5));
    assert_eq!(a.by_route["router-large"].delegated, 2);
    assert_eq!(a.by_route["router-large"].judged, 2);
    assert_eq!(
        a.by_route["router-everyday"].success_rate, None,
        "unknown stays out of the rate"
    );
    assert_eq!((a.outcomes.unknown, a.outcomes.judged), (2, 2));

    // A judgment with only a correction is not a verdict: still unknown.
    record_judgment(&db, ids[2], &judgment(None, Some(Correction::Minor), None)).unwrap();
    let a = aggregates(&db, &Settings::default()).unwrap();
    assert_eq!(a.by_route["router-everyday"].success_rate, None);
    assert_eq!(a.by_route["router-everyday"].correction.minor, 1);
    assert_eq!((a.outcomes.unknown, a.outcomes.judged), (2, 2));
}

#[test]
fn aggregates_group_by_route_category_and_measure_cost_and_time() {
    let (_d, db, ids) = db_with_jobs();
    attach_automatic(
        &db,
        ids[0],
        &measurement(1000, Some(200), Some(0.5), 4000, 2),
    )
    .unwrap();
    attach_automatic(
        &db,
        ids[1],
        &measurement(3000, Some(400), Some(1.5), 8000, 4),
    )
    .unwrap();
    let a = aggregates(&db, &Settings::default()).unwrap();

    let large = &a.by_route["router-large"];
    assert_eq!(large.avg_tokens, Some(2300.0), "(1200 + 3400) / 2");
    assert_eq!(large.avg_cost_usd, Some(1.0));
    assert_eq!(large.avg_duration_ms, Some(6000.0));
    assert_eq!(large.avg_runs, Some(3.0));
    assert_eq!(a.by_route["router-tiny"].avg_tokens, None, "not measured");

    assert_eq!(a.by_category["bug"].delegated, 2);
    assert_eq!(a.by_category["ui"].delegated, 1);
    assert_eq!(a.by_category["mechanical"].delegated, 1);
    assert_eq!(a.by_category["bug"].avg_cost_usd, Some(1.0));

    let s = summarize(&db, &Settings::default()).unwrap();
    assert_eq!(s.by_policy["2026.1"].messages, 3);
    assert_eq!(s.by_policy["2026.1+uabcd1234"].messages, 1);
}

#[test]
fn legacy_rows_are_shown_as_legacy_size() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("router.db");
    let mut legacy = delegated("router-large", Nature::Bug, "x");
    legacy.policy_version = None;
    legacy.card = None;
    legacy.rule = None;
    record_at(&db, &legacy, at(0)).unwrap();
    let s = summarize(&db, &Settings::default()).unwrap();
    assert_eq!(s.by_policy["legacy-size"].messages, 1);
    let a = aggregates(&db, &Settings::default()).unwrap();
    assert_eq!(
        a.by_category["unknown"].delegated, 1,
        "no card, unknown category"
    );
}

// ── CLI ──────────────────────────────────────────────────────────────────

fn seeded_home() -> (ConfigHome, Vec<i64>) {
    let home = temp_config_home();
    let db = home.router_db();
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let ids = vec![
        record_at(
            &db,
            &delegated("router-large", Nature::Bug, "2026.1"),
            at(0),
        )
        .unwrap(),
        record_at(
            &db,
            &delegated("router-everyday", Nature::Ui, "2026.1"),
            at(1),
        )
        .unwrap(),
    ];
    (home, ids)
}

fn run(home: &ConfigHome, args: &[&str]) -> std::process::Output {
    run_ecotokens(home, args, None, None)
}

fn json_of(out: &std::process::Output) -> Value {
    serde_json::from_str(&stdout_of(out)).unwrap_or_else(|e| panic!("{e}: {}", stdout_of(out)))
}

#[test]
fn outcome_pending_lists_jobs_without_text() {
    let (home, ids) = seeded_home();
    let out = run(&home, &["router", "outcome", "--pending", "--json"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let v = json_of(&out);
    let jobs = v.as_array().unwrap();
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0]["id"], ids[0]);
    assert_eq!(jobs[0]["route"], "router-large");
    assert_eq!(jobs[0]["category"], "bug");
    for key in ["date", "cost_usd", "duration_ms"] {
        assert!(jobs[0].get(key).is_some(), "{key}");
    }
    let keys: Vec<_> = jobs[0].as_object().unwrap().keys().cloned().collect();
    assert!(!keys
        .iter()
        .any(|k| k.contains("message") || k.contains("prompt") || k.contains("text")));

    let out = run(&home, &["router", "outcome", "--pending"]);
    let text = stdout_of(&out);
    assert!(
        text.contains("router-large") && text.contains("bug"),
        "{text}"
    );
}

#[test]
fn outcome_records_a_judgment_and_status_reports_it() {
    let (home, ids) = seeded_home();
    let id = ids[0].to_string();
    let out = run(
        &home,
        &[
            "router",
            "outcome",
            &id,
            "--success",
            "yes",
            "--correction",
            "minor",
            "--regressions",
            "1",
            "--json",
        ],
    );
    assert!(out.status.success(), "{}", stderr_of(&out));
    let v = json_of(&out);
    assert_eq!(v["id"], ids[0]);
    assert_eq!(v["success"], true);
    assert_eq!(v["correction"], "minor");
    assert_eq!(v["regressions"], 1);

    // Idempotent.
    let again = run(&home, &["router", "outcome", &id, "--success", "yes"]);
    assert!(again.status.success());

    let out = run(&home, &["router", "status", "--json"]);
    let status = json_of(&out);
    let large = &status["by_route"]["router-large"];
    assert_eq!(large["success_rate"], 1.0);
    assert_eq!(large["judged"], 1);
    assert_eq!(large["correction"]["minor"], 1);
    assert_eq!(status["by_category"]["bug"]["judged"], 1);
    assert_eq!(
        status["by_route"]["router-everyday"]["success_rate"],
        Value::Null
    );
    assert_eq!(status["outcomes"]["judged"], 1);
    assert_eq!(status["outcomes"]["unknown"], 1);
    assert_eq!(status["by_policy"]["2026.1"]["messages"], 2);
    assert_eq!(status["policy_version"], "2026.1");

    let out = run(&home, &["router", "outcome", "--pending", "--json"]);
    assert_eq!(json_of(&out).as_array().unwrap().len(), 1);
}

#[test]
fn outcome_with_an_unknown_id_fails_with_a_hint() {
    let (home, _) = seeded_home();
    let out = run(&home, &["router", "outcome", "424242", "--success", "no"]);
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(err.contains("424242"), "{err}");
    assert!(err.contains("router outcome --pending"), "{err}");
}

#[test]
fn outcome_without_flags_explains_itself() {
    let (home, ids) = seeded_home();
    let out = run(&home, &["router", "outcome"]);
    assert!(out.status.success());
    let text = stdout_of(&out);
    assert!(
        text.contains("--pending") && text.contains("--success"),
        "{text}"
    );
    assert!(text.contains('2'), "the pending count: {text}");

    let out = run(&home, &["router", "outcome", "--json"]);
    assert_eq!(json_of(&out)["pending"], 2);

    // An id with nothing to record is a usage error, not a silent no-op.
    let out = run(&home, &["router", "outcome", &ids[0].to_string()]);
    assert!(!out.status.success());
}

#[test]
fn the_existing_status_keys_keep_their_meaning() {
    let (home, _) = seeded_home();
    let status = json_of(&run(&home, &["router", "status", "--json"]));
    for key in [
        "enabled",
        "hook_installed",
        "agents_installed",
        "api_key",
        "min_confidence",
        "timeout_ms",
        "summary",
    ] {
        assert!(status.get(key).is_some(), "existing key `{key}` is gone");
    }
    let summary = &status["summary"];
    for key in [
        "messages",
        "by_decision",
        "by_size",
        "jev_requests",
        "input_tokens",
        "output_tokens",
        "cost_usd",
        "avg_latency_ms",
        "first",
    ] {
        assert!(
            summary.get(key).is_some(),
            "existing summary key `{key}` is gone"
        );
    }
    assert_eq!(summary["messages"], 2);
    assert_eq!(summary["by_size"]["large"]["delegated"], 2);
    for key in [
        "private_skipped",
        "not_allowed_skipped",
        "private_locations",
        "allowed_locations",
        "strict",
        "policy_version",
        "by_route",
        "by_category",
        "by_policy",
        "outcomes",
    ] {
        assert!(status.get(key).is_some(), "new key `{key}` missing");
    }
}

#[test]
fn status_on_an_empty_store_has_the_new_keys_with_empty_values() {
    let home = temp_config_home();
    let status = json_of(&run(&home, &["router", "status", "--json"]));
    assert_eq!(status["by_route"], serde_json::json!({}));
    assert_eq!(status["outcomes"]["judged"], 0);
    assert_eq!(status["private_skipped"], 0);
    let out = run(&home, &["router", "status"]);
    assert!(stdout_of(&out).contains("No messages routed yet"));
    let _ = (Path::new(""), Size::Tiny);
}
