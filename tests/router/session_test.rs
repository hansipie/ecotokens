#[path = "common.rs"]
mod common;
use common::{
    run_ecotokens, settings_private, settings_strict, stdout_of, temp_config_home, write_config,
};

use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use ecotokens::config::Settings;
use ecotokens::router::session::{end, start, SessionInput};
use ecotokens::router::stats::record_at;
use ecotokens::router::{Decision, Routing};
use rusqlite::Connection;
use tempfile::TempDir;

const FIXTURE: &str = include_str!("fixtures/transcript_subagent.jsonl");

fn at(h: u32, m: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 17, h, m, s).unwrap()
}

fn delegated(route: &str, session: &str) -> Routing {
    let mut r = Routing::without_jev(Decision::Delegated);
    r.route = Some(route.into());
    r.session_id = Some(session.into());
    r.policy_version = Some("2026.1".into());
    r
}

fn other(decision: Decision, session: &str) -> Routing {
    let mut r = Routing::without_jev(decision);
    r.session_id = Some(session.into());
    r
}

struct World {
    _dir: TempDir,
    db: PathBuf,
    transcript: PathBuf,
    cwd: PathBuf,
}

/// Two delegated decisions and one self-handled message in session `sess-1`.
fn world() -> World {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let db = root.join("router.db");
    let transcript = root.join("transcript.jsonl");
    std::fs::write(&transcript, FIXTURE).unwrap();
    let cwd = root.join("work");
    std::fs::create_dir_all(&cwd).unwrap();
    record_at(&db, &delegated("router-large", "sess-1"), at(10, 0, 0)).unwrap();
    record_at(&db, &delegated("router-everyday", "sess-1"), at(10, 5, 0)).unwrap();
    record_at(&db, &other(Decision::SelfUnsure, "sess-1"), at(10, 10, 0)).unwrap();
    World {
        _dir: dir,
        db,
        transcript,
        cwd,
    }
}

fn input(w: &World) -> SessionInput {
    SessionInput {
        session_id: Some("sess-1".into()),
        transcript_path: Some(w.transcript.clone()),
        cwd: Some(w.cwd.clone()),
    }
}

#[derive(Debug, PartialEq)]
struct Row {
    id: i64,
    cost: Option<f64>,
    tokens_in: Option<i64>,
    tokens_out: Option<i64>,
    duration: Option<i64>,
    runs: Option<i64>,
    success: Option<i64>,
    reminded: i64,
}

fn rows(db: &Path) -> Vec<Row> {
    let conn = Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT decision_id, cost_usd, tokens_in, tokens_out, duration_ms, runs, success, reminded
             FROM router_outcomes ORDER BY decision_id",
        )
        .unwrap();
    let v = stmt
        .query_map([], |r| {
            Ok(Row {
                id: r.get(0)?,
                cost: r.get(1)?,
                tokens_in: r.get(2)?,
                tokens_out: r.get(3)?,
                duration: r.get(4)?,
                runs: r.get(5)?,
                success: r.get(6)?,
                reminded: r.get(7)?,
            })
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    v
}

fn priced() -> Settings {
    Settings {
        router_enabled: true,
        price_input_usd_per_mtok: Some(3.0),
        price_output_usd_per_mtok: Some(15.0),
        ..Settings::default()
    }
}

#[test]
fn cost_and_duration_are_summed_over_every_router_run_of_the_job() {
    let w = world();
    let report = end(&w.db, &priced(), &input(&w), true);
    assert!(report.transcript_read);

    let r = rows(&w.db);
    assert_eq!(r.len(), 2, "only delegated decisions get an outcome row");
    let (first, second) = (&r[0], &r[1]);

    // First job: the producer plus the reviewer (retries and reviews included).
    assert_eq!(first.id, 1);
    assert_eq!(first.runs, Some(2));
    assert_eq!(
        first.tokens_in,
        Some(20_000 + 5_000),
        "the reviewer reports a total only"
    );
    assert_eq!(first.tokens_out, Some(1_000));
    assert_eq!(
        first.duration,
        Some(120_000),
        "decision at 10:00:00 to the end of the reviewer at 10:02:00"
    );
    assert_eq!(
        first.cost, None,
        "one run has no input/output split: no invented cost"
    );
    assert_eq!(
        first.success,
        Some(1),
        "one producer, then a reviewer that approves"
    );

    // Second job: one run with a full split, priced.
    assert_eq!(second.id, 2);
    assert_eq!(second.runs, Some(1));
    assert_eq!(second.tokens_in, Some(2_500));
    assert_eq!(second.tokens_out, Some(500));
    assert_eq!(second.duration, Some(40_000));
    let expected = 2_500.0 / 1e6 * 3.0 + 500.0 / 1e6 * 15.0;
    assert!(
        (second.cost.unwrap() - expected).abs() < 1e-9,
        "{:?}",
        second.cost
    );
}

#[test]
fn the_price_of_the_jobs_tier_is_used_and_an_unset_part_falls_back_to_the_global_one() {
    let w = world();
    let mut settings = priced();
    // The second job runs on router-everyday: input from its tier, output global.
    settings.router_tier_prices.everyday.input_usd_per_mtok = Some(1.0);
    // Another tier's price changes nothing for it.
    settings.router_tier_prices.tiny.output_usd_per_mtok = Some(99.0);
    end(&w.db, &settings, &input(&w), true);
    let second = &rows(&w.db)[1];
    let expected = 2_500.0 / 1e6 * 1.0 + 500.0 / 1e6 * 15.0;
    assert!(
        (second.cost.unwrap() - expected).abs() < 1e-9,
        "{:?}",
        second.cost
    );
}

#[test]
fn a_tier_price_alone_prices_the_job_without_a_global_price() {
    let w = world();
    let mut settings = Settings {
        router_enabled: true,
        ..Settings::default()
    };
    settings.router_tier_prices.everyday.input_usd_per_mtok = Some(2.0);
    settings.router_tier_prices.everyday.output_usd_per_mtok = Some(10.0);
    end(&w.db, &settings, &input(&w), true);
    let r = rows(&w.db);
    let expected = 2_500.0 / 1e6 * 2.0 + 500.0 / 1e6 * 10.0;
    assert!(
        (r[1].cost.unwrap() - expected).abs() < 1e-9,
        "{:?}",
        r[1].cost
    );
}

/// Cost of job 1 after `end` over a transcript of `runs` (100 input and 10
/// output tokens each) with `settings`.
fn cost_after(runs: &[(&str, &str)], settings: &Settings) -> Option<f64> {
    let w = world();
    std::fs::write(&w.transcript, transcript_of(runs)).unwrap();
    end(&w.db, settings, &input(&w), true);
    rows(&w.db)[0].cost
}

#[test]
fn each_run_is_priced_at_its_own_tier_and_the_reviewer_runs_on_large() {
    let mut settings = priced();
    settings.router_tier_prices.tiny.input_usd_per_mtok = Some(1.0);
    settings.router_tier_prices.tiny.output_usd_per_mtok = Some(5.0);
    settings.router_tier_prices.large.input_usd_per_mtok = Some(15.0);
    settings.router_tier_prices.large.output_usd_per_mtok = Some(75.0);
    let cost = cost_after(
        &[
            ("router-tiny", "done"),
            ("router-reviewer", "Verdict: approve"),
        ],
        &settings,
    )
    .unwrap();
    let tiny = 100.0 / 1e6 * 1.0 + 10.0 / 1e6 * 5.0;
    let large = 100.0 / 1e6 * 15.0 + 10.0 / 1e6 * 75.0;
    assert!((cost - (tiny + large)).abs() < 1e-12, "{cost}");
}

#[test]
fn a_helper_of_no_known_tier_uses_the_global_price() {
    let mut settings = priced();
    for tier in ["tiny", "everyday", "large", "hardest"] {
        let p = settings.router_tier_prices.get_mut(tier).unwrap();
        p.input_usd_per_mtok = Some(1_000.0);
        p.output_usd_per_mtok = Some(1_000.0);
    }
    let cost = cost_after(&[("router-custom", "done")], &settings).unwrap();
    let global = 100.0 / 1e6 * 3.0 + 10.0 / 1e6 * 15.0;
    assert!((cost - global).abs() < 1e-12, "{cost}");
}

#[test]
fn one_unpriced_run_leaves_the_job_cost_unknown() {
    // No global price; only the tiny tier has one, the reviewer (large) none.
    let mut settings = Settings {
        router_enabled: true,
        ..Settings::default()
    };
    settings.router_tier_prices.tiny.input_usd_per_mtok = Some(1.0);
    assert_eq!(
        cost_after(
            &[
                ("router-tiny", "done"),
                ("router-reviewer", "Verdict: approve")
            ],
            &settings
        ),
        None
    );
    assert!(cost_after(&[("router-tiny", "done")], &settings).is_some());
}

#[test]
fn without_a_price_the_cost_stays_null_but_tokens_are_measured() {
    let w = world();
    let settings = Settings {
        router_enabled: true,
        ..Settings::default()
    };
    end(&w.db, &settings, &input(&w), true);
    let r = rows(&w.db);
    assert_eq!(r[1].cost, None);
    assert_eq!(r[1].tokens_in, Some(2_500));
}

#[test]
fn an_unreadable_transcript_leaves_null_never_zero() {
    let w = world();
    let mut bad = input(&w);
    bad.transcript_path = Some(w.cwd.join("missing.jsonl"));
    let report = end(&w.db, &priced(), &bad, true);
    assert!(!report.transcript_read);
    for row in rows(&w.db) {
        assert_eq!(row.cost, None);
        assert_eq!(row.tokens_in, None);
        assert_eq!(row.tokens_out, None);
        assert_eq!(row.duration, None);
        assert_eq!(row.runs, None);
    }

    // Garbage instead of a transcript.
    let garbage = w.cwd.join("garbage.jsonl");
    std::fs::write(&garbage, "not json\n{\"half\":").unwrap();
    bad.transcript_path = Some(garbage);
    end(&w.db, &priced(), &bad, true);
    for row in rows(&w.db) {
        assert_eq!(row.runs, None, "no run found is unknown, not zero");
    }
}

#[test]
fn a_transcript_without_a_matching_run_leaves_null() {
    let w = world();
    let empty = w.cwd.join("empty.jsonl");
    std::fs::write(
        &empty,
        r#"{"type":"user","timestamp":"2026-10-17T10:00:01.000Z","message":{"role":"user","content":"hi"}}"#,
    )
    .unwrap();
    let mut i = input(&w);
    i.transcript_path = Some(empty);
    end(&w.db, &priced(), &i, true);
    for row in rows(&w.db) {
        assert_eq!((row.runs, row.tokens_in, row.duration), (None, None, None));
    }
}

#[test]
fn a_private_or_not_allowed_folder_is_never_read() {
    for settings in [
        settings_private(&["PLACEHOLDER"]),
        settings_strict(&["/somewhere/else"]),
    ] {
        let w = world();
        let mut settings = settings;
        if !settings.router_private.is_empty() {
            settings.router_private = vec![w.cwd.to_string_lossy().into_owned()];
        }
        let report = end(&w.db, &settings, &input(&w), true);
        assert!(
            !report.transcript_read,
            "the transcript of a private folder is not opened"
        );
        assert!(rows(&w.db).is_empty(), "nothing is attached");
    }
    // An untrusted settings file counts as private too.
    let w = world();
    let report = end(&w.db, &priced(), &input(&w), false);
    assert!(!report.transcript_read);
}

#[test]
fn a_hostile_session_id_matches_nothing() {
    let w = world();
    let mut i = input(&w);
    i.session_id = Some("sess-1' OR '1'='1".into());
    end(&w.db, &priced(), &i, true);
    assert!(rows(&w.db).is_empty());
}

// ── Reminder ─────────────────────────────────────────────────────────────

fn message(out: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(out).expect("JSON output");
    assert_eq!(v.as_object().unwrap().len(), 1, "only systemMessage: {v}");
    v["systemMessage"].as_str().unwrap().to_string()
}

#[test]
fn the_reminder_is_one_line_with_a_count_and_a_command_and_shows_once() {
    let w = world();
    end(&w.db, &priced(), &input(&w), true);
    let out = start(&w.db, &priced(), &input(&w), true).expect("a reminder is due");
    let text = message(&out);
    // The first job was judged by its reviewer, so only the second is due.
    assert!(text.contains("1 delegated job has"), "{text}");
    assert!(
        text.contains("ecotokens router outcome --pending"),
        "{text}"
    );
    assert!(!text.contains('\n'));
    assert!(
        !text.contains("router-large") && !text.contains("work"),
        "no route, no path: {text}"
    );

    // Once per job.
    assert_eq!(start(&w.db, &priced(), &input(&w), true), None);
    assert!(rows(&w.db)
        .iter()
        .all(|r| r.reminded == 1 || r.success.is_some()));
}

#[test]
fn the_reminder_is_silent_when_nothing_is_pending() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("router.db");
    let w = world();
    assert_eq!(
        start(&db, &priced(), &input(&w), true),
        None,
        "no database yet"
    );

    // A judged job is not pending.
    let w = world();
    end(&w.db, &priced(), &input(&w), true);
    let conn = Connection::open(&w.db).unwrap();
    conn.execute("UPDATE router_outcomes SET success = 1", [])
        .unwrap();
    drop(conn);
    assert_eq!(start(&w.db, &priced(), &input(&w), true), None);
}

#[test]
fn the_reminder_is_silent_for_private_and_not_allowed_projects() {
    let w = world();
    end(&w.db, &priced(), &input(&w), true);
    let mut private = priced();
    private.router_private = vec![w.cwd.to_string_lossy().into_owned()];
    assert_eq!(start(&w.db, &private, &input(&w), true), None);
    let strict = Settings {
        router_strict: true,
        ..priced()
    };
    assert_eq!(start(&w.db, &strict, &input(&w), true), None);
    assert_eq!(
        start(&w.db, &priced(), &input(&w), false),
        None,
        "untrusted settings"
    );
    assert!(
        rows(&w.db).iter().all(|r| r.reminded == 0),
        "nothing was consumed"
    );
    // Still due once the gate opens.
    assert!(start(&w.db, &priced(), &input(&w), true).is_some());
}

#[test]
fn the_reminder_needs_the_router_to_be_on() {
    let w = world();
    end(&w.db, &priced(), &input(&w), true);
    let off = Settings::default();
    assert_eq!(start(&w.db, &off, &input(&w), true), None);
}

#[test]
fn a_single_job_is_worded_in_the_singular() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("router.db");
    record_at(&db, &delegated("router-tiny", "s"), at(10, 0, 0)).unwrap();
    let cwd = dir.path().canonicalize().unwrap();
    let i = SessionInput {
        session_id: Some("s".into()),
        transcript_path: None,
        cwd: Some(cwd),
    };
    let text = message(&start(&db, &priced(), &i, true).unwrap());
    assert!(text.contains("1 delegated job has"), "{text}");
}

// ── Binary level ─────────────────────────────────────────────────────────

fn hook_json(w: &World, event: &str) -> String {
    serde_json::json!({
        "session_id": "sess-1",
        "transcript_path": w.transcript,
        "cwd": w.cwd,
        "hook_event_name": event,
        "source": "startup",
    })
    .to_string()
}

#[test]
fn the_session_end_hook_prints_nothing_and_the_start_hook_prints_one_message() {
    let home = temp_config_home();
    write_config(&home, &serde_json::json!({ "router_enabled": true }));
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let transcript = root.join("t.jsonl");
    std::fs::write(&transcript, FIXTURE).unwrap();
    let cwd = root.join("work");
    std::fs::create_dir_all(&cwd).unwrap();
    let db = home.router_db();
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    record_at(&db, &delegated("router-large", "sess-1"), at(10, 0, 0)).unwrap();
    let w = World {
        _dir: dir,
        db: db.clone(),
        transcript,
        cwd,
    };

    let out = run_ecotokens(
        &home,
        &["hook-router-session", "end"],
        None,
        Some(&hook_json(&w, "SessionEnd")),
    );
    assert!(out.status.success());
    assert!(stdout_of(&out).is_empty(), "SessionEnd prints nothing");
    assert_eq!(rows(&db).len(), 1);

    let out = run_ecotokens(
        &home,
        &["hook-router-session", "start"],
        None,
        Some(&hook_json(&w, "SessionStart")),
    );
    assert!(out.status.success());
    assert!(message(&stdout_of(&out)).contains("1 delegated job"));
    let out = run_ecotokens(
        &home,
        &["hook-router-session", "start"],
        None,
        Some(&hook_json(&w, "SessionStart")),
    );
    assert!(stdout_of(&out).is_empty(), "once per job");
}

#[test]
fn the_session_hooks_are_fail_open() {
    let home = temp_config_home();
    write_config(&home, &serde_json::json!({ "router_enabled": true }));
    for event in ["start", "end", "bogus"] {
        for input in ["", "not json", "[1]", "{\"cwd\": 5}"] {
            let out = run_ecotokens(&home, &["hook-router-session", event], None, Some(input));
            assert!(out.status.success(), "{event} {input:?}: {:?}", out.status);
            assert!(stdout_of(&out).is_empty(), "{event} {input:?}");
        }
    }
}

// ── Reviewer verdicts ────────────────────────────────────────────────────

/// A transcript of one session with the given `(subagent_type, reply)` runs,
/// one minute apart from 10:00:30, each with a full token split.
fn transcript_of(runs: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for (i, (agent, reply)) in runs.iter().enumerate() {
        let id = format!("tu{i}");
        let (start, end) = (30 + i * 60, 50 + i * 60);
        let ts = |s: usize| format!("2026-10-17T10:{:02}:{:02}.000Z", s / 60, s % 60);
        out.push_str(&format!(
            "{}\n",
            serde_json::json!({"type": "assistant", "timestamp": ts(start),
                "message": {"role": "assistant", "content": [{"type": "tool_use", "id": id,
                    "name": "Agent", "input": {"subagent_type": agent, "prompt": "p"}}]}})
        ));
        out.push_str(&format!(
            "{}\n",
            serde_json::json!({"type": "user", "timestamp": ts(end),
                "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": id,
                    "content": [{"type": "text", "text": reply}]}]},
                "toolUseResult": {"usage": {"input_tokens": 100, "output_tokens": 10}}})
        ));
    }
    out
}

/// The success of job 1 after `end` over a transcript made of `runs`.
fn success_after(runs: &[(&str, &str)]) -> Option<i64> {
    let w = world();
    std::fs::write(&w.transcript, transcript_of(runs)).unwrap();
    end(&w.db, &priced(), &input(&w), true);
    rows(&w.db)[0].success
}

#[test]
fn an_approval_after_a_single_producer_is_a_success() {
    let reply = "Verdict: approve\nthe diff matches the goal";
    assert_eq!(
        success_after(&[("router-large", "done"), ("router-reviewer", reply)]),
        Some(1)
    );
}

#[test]
fn a_rejection_is_a_failure_even_after_a_retry() {
    let reply = "**Verdict: reject** — the test fails";
    assert_eq!(
        success_after(&[
            ("router-large", "done"),
            ("router-large", "done again"),
            ("router-reviewer", reply),
        ]),
        Some(0)
    );
}

#[test]
fn an_approval_after_a_retry_is_not_a_first_attempt_success() {
    let reply = "Verdict: approve";
    assert_eq!(
        success_after(&[
            ("router-large", "done"),
            ("router-large", "done again"),
            ("router-reviewer", reply),
        ]),
        None
    );
}

#[test]
fn a_reply_without_a_readable_verdict_leaves_the_job_unjudged() {
    for reply in [
        "Looks fine to me",
        "Verdict: unclear, the test output is missing",
        "I would not approve this. Verdict:",
    ] {
        assert_eq!(
            success_after(&[("router-large", "done"), ("router-reviewer", reply)]),
            None,
            "{reply}"
        );
    }
}

#[test]
fn no_review_means_no_verdict() {
    assert_eq!(success_after(&[("router-large", "Verdict: approve")]), None);
}

#[test]
fn the_user_judgment_is_never_overwritten_by_the_reviewer() {
    let w = world();
    std::fs::write(
        &w.transcript,
        transcript_of(&[
            ("router-large", "done"),
            ("router-reviewer", "Verdict: approve"),
        ]),
    )
    .unwrap();
    // The user judged job 1 by hand (a row with no `judged_by`, as written by
    // older versions, counts as theirs too).
    let conn = Connection::open(&w.db).unwrap();
    conn.execute(
        "INSERT INTO router_outcomes (decision_id, success) VALUES (1, 0)",
        [],
    )
    .unwrap();
    drop(conn);
    end(&w.db, &priced(), &input(&w), true);
    assert_eq!(rows(&w.db)[0].success, Some(0));
}

#[test]
fn a_later_review_replaces_an_earlier_automatic_verdict() {
    let w = world();
    for verdict in ["Verdict: reject", "Verdict: approve"] {
        std::fs::write(
            &w.transcript,
            transcript_of(&[("router-large", "done"), ("router-reviewer", verdict)]),
        )
        .unwrap();
        end(&w.db, &priced(), &input(&w), true);
    }
    assert_eq!(rows(&w.db)[0].success, Some(1));
}
