//! `ecotokens hook-router-session start|end`: the SessionStart and SessionEnd
//! hooks of the router.
//!
//! `end` reads the session transcript and attaches cost, tokens and helper
//! time to the session's delegated jobs, and the `router-reviewer` verdict as
//! their (automatic) success. The cost is the measured tokens times the
//! configured price of each run's tier (else the global price); it is
//! computed once, so a later price change never alters a stored cost.
//! `start` prints one line reminding
//! the user of jobs nobody judged, once per job. Both are fail-open and
//! never touch a private or not-allowed folder: its transcript is not even
//! opened.
//!
//! The transcript is an internal Claude Code format, so extraction is a
//! heuristic: a run is a `router-*` subagent call found in the job's time
//! window, and anything that cannot be read stays unknown (NULL), never zero.

use std::collections::HashMap;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use super::agents::AgentKind;
use super::location::{self, GateResult};
use super::outcome::{attach_on, attach_reviewer_verdict, Measurement};
use super::stats;
use crate::config::Settings;

/// What the hook receives on standard input; unknown fields are ignored.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SessionInput {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub transcript_path: Option<PathBuf>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

/// What `end` did, for tests and debugging.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EndReport {
    pub transcript_read: bool,
    pub jobs: usize,
}

/// One `router-*` subagent run found in the transcript.
#[derive(Debug, Clone, Default)]
struct Run {
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    tokens_in: Option<i64>,
    tokens_out: Option<i64>,
    /// Only a combined figure is known.
    tokens_total: Option<i64>,
    duration_ms: Option<i64>,
    /// The `subagent_type` of the call, e.g. `router-large`.
    agent: String,
    /// For a `router-reviewer` run: `Some(true)` approved, `Some(false)`
    /// rejected, `None` when the reply carries no readable verdict.
    verdict: Option<bool>,
}

const REVIEWER: &str = "router-reviewer";

/// The verdict of a reviewer's reply: its first line that starts with
/// `Verdict:` and then approves or rejects. Anything else is no verdict, and
/// the job stays unjudged.
fn parse_verdict(text: &str) -> Option<bool> {
    let line = text.lines().find_map(|l| {
        let l = l.trim_start_matches(|c: char| c.is_whitespace() || "*#>_`".contains(c));
        let head = l.get(.."verdict".len())?;
        head.eq_ignore_ascii_case("verdict")
            .then(|| &l["verdict".len()..])
    })?;
    let word = line
        .trim_start_matches(|c: char| c.is_whitespace() || ":-*_`—".contains(c))
        .to_ascii_lowercase();
    if word.starts_with("approve") {
        Some(true)
    } else if word.starts_with("reject") {
        Some(false)
    } else {
        None
    }
}

fn parse_ts(v: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(v.get("timestamp")?.as_str()?)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

fn int(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))
}

/// `<usage>total_tokens: N … duration_ms: M</usage>`, as Claude Code appends
/// it to a subagent's result text.
fn usage_in_text(text: &str) -> (Option<i64>, Option<i64>) {
    let Some(start) = text.find("<usage>") else {
        return (None, None);
    };
    let block = &text[start..];
    let find = |key: &str| -> Option<i64> {
        let at = block.find(key)? + key.len();
        let digits: String = block[at..]
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().ok()
    };
    (find("total_tokens:"), find("duration_ms:"))
}

fn result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn fill_from_tool_use_result(run: &mut Run, r: &Value) {
    if let Some(usage) = r.get("usage") {
        let get = |k: &str| usage.get(k).and_then(int);
        if let (Some(input), Some(output)) = (get("input_tokens"), get("output_tokens")) {
            run.tokens_in = Some(
                input
                    + get("cache_creation_input_tokens").unwrap_or(0)
                    + get("cache_read_input_tokens").unwrap_or(0),
            );
            run.tokens_out = Some(output);
        }
    }
    if run.tokens_in.is_none() {
        run.tokens_total = r
            .get("totalTokens")
            .or_else(|| r.get("total_tokens"))
            .and_then(int);
    }
    if let Some(ms) = r
        .get("totalDurationMs")
        .or_else(|| r.get("duration_ms"))
        .and_then(int)
    {
        run.duration_ms = Some(ms);
    }
}

/// Reads the `router-*` subagent runs of a transcript, in order. `None` when
/// the file cannot be read at all; lines of an unexpected shape are skipped.
fn read_runs(path: &Path) -> Option<Vec<Run>> {
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    let mut ids: Vec<String> = Vec::new();
    let mut runs: HashMap<String, Run> = HashMap::new();
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let ts = parse_ts(&v);

        // A queued task notification carries the usage of a background run.
        if let Some(text) = v.get("content").and_then(Value::as_str) {
            if let Some(id) = text
                .split("<tool-use-id>")
                .nth(1)
                .and_then(|s| s.split("</tool-use-id>").next())
            {
                if let Some(run) = runs.get_mut(id.trim()) {
                    let (total, ms) = usage_in_text(text);
                    run.tokens_total = run.tokens_total.or(total);
                    run.duration_ms = run.duration_ms.or(ms);
                    run.end = ts.or(run.end);
                }
            }
        }

        let Some(content) = v.pointer("/message/content").and_then(Value::as_array) else {
            continue;
        };
        for item in content {
            match item.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    let is_agent = matches!(
                        item.get("name").and_then(Value::as_str),
                        Some("Agent" | "Task")
                    );
                    let agent = item
                        .pointer("/input/subagent_type")
                        .and_then(Value::as_str)
                        .filter(|s| s.starts_with("router-"));
                    if let (true, Some(agent), Some(id)) =
                        (is_agent, agent, item.get("id").and_then(Value::as_str))
                    {
                        ids.push(id.to_string());
                        runs.insert(
                            id.to_string(),
                            Run {
                                start: ts,
                                agent: agent.to_string(),
                                ..Run::default()
                            },
                        );
                    }
                }
                Some("tool_result") => {
                    let Some(run) = item
                        .get("tool_use_id")
                        .and_then(Value::as_str)
                        .and_then(|id| runs.get_mut(id))
                    else {
                        continue;
                    };
                    if let Some(r) = v.get("toolUseResult").filter(|r| r.is_object()) {
                        fill_from_tool_use_result(run, r);
                    }
                    let text = result_text(item.get("content").unwrap_or(&Value::Null));
                    if run.agent == REVIEWER {
                        run.verdict = parse_verdict(&text);
                    }
                    let (total, ms) = usage_in_text(&text);
                    if run.tokens_in.is_none() {
                        run.tokens_total = run.tokens_total.or(total);
                    }
                    run.duration_ms = run.duration_ms.or(ms);
                    run.end = ts.or(run.end);
                }
                _ => {}
            }
        }
    }
    Some(ids.into_iter().filter_map(|id| runs.remove(&id)).collect())
}

/// Time at which a run finished: its result line, else start plus its
/// reported duration.
fn finished(run: &Run) -> Option<DateTime<Utc>> {
    run.end
        .or_else(|| Some(run.start? + chrono::Duration::milliseconds(run.duration_ms?)))
}

/// The router tier whose price applies to a run: the tier of its helper (the
/// analyst and the reviewer run on `large`), `None` for an unknown helper.
fn tier_of(agent: &str) -> Option<&'static str> {
    AgentKind::ALL
        .into_iter()
        .find(|k| k.name() == agent)
        .map(|k| k.tier().as_str())
}

fn measure(runs: &[&Run], from: DateTime<Utc>, settings: &Settings) -> Measurement {
    if runs.is_empty() {
        return Measurement::default();
    }
    let mut tokens_in = 0i64;
    let mut tokens_out = 0i64;
    let mut any_tokens = false;
    let mut any_split = false;
    // Each run is priced at the price of its own tier (else the global one);
    // one run without a split or without any price leaves the cost unknown.
    let mut cost = Some(0.0f64);
    for run in runs {
        match (run.tokens_in, run.tokens_out, run.tokens_total) {
            (Some(i), Some(o), _) => {
                tokens_in += i;
                tokens_out += o;
                any_tokens = true;
                any_split = true;
                let price = settings.effective_tier_price(tier_of(&run.agent));
                cost = cost.filter(|_| price.is_priced()).map(|c| {
                    c + i as f64 / 1e6 * price.input.unwrap_or(0.0)
                        + o as f64 / 1e6 * price.output.unwrap_or(0.0)
                });
            }
            (_, _, Some(total)) => {
                // Only a total: counted as input, and the cost stays unknown.
                tokens_in += total;
                any_tokens = true;
                cost = None;
            }
            _ => cost = None,
        }
    }
    let cost_usd = cost.filter(|_| any_tokens);
    let last_end = runs.iter().filter_map(|r| finished(r)).max();
    Measurement {
        cost_usd,
        tokens_in: any_tokens.then_some(tokens_in),
        tokens_out: any_split.then_some(tokens_out),
        duration_ms: last_end.map(|end| (end - from).num_milliseconds().max(0)),
        runs: Some(runs.len() as i64),
    }
}

/// What the reviewer's verdict says about a job's success. A rejection is a
/// failure. An approval counts as the documented "verified success on the
/// first attempt" only when exactly one helper produced the result: after a
/// retry or a second producer the job stays unjudged. The last readable
/// verdict wins.
fn reviewed_success(runs: &[&Run]) -> Option<bool> {
    let verdict = runs
        .iter()
        .rev()
        .filter(|r| r.agent == REVIEWER)
        .find_map(|r| r.verdict)?;
    let producers = runs
        .iter()
        .filter(|r| r.agent != REVIEWER && r.agent != "router-analyst")
        .count();
    (!verdict || producers == 1).then_some(verdict)
}

fn gate_open(settings: &Settings, input: &SessionInput, trusted: bool) -> bool {
    settings.router_enabled
        && location::gate(input.cwd.as_deref(), settings, trusted) == GateResult::Open
}

/// SessionEnd: attaches cost, tokens and helper time to the session's
/// delegated jobs. Prints nothing.
pub fn end(db: &Path, settings: &Settings, input: &SessionInput, trusted: bool) -> EndReport {
    let mut report = EndReport::default();
    if !gate_open(settings, input, trusted) || !db.exists() {
        return report;
    }
    let Some(session) = input.session_id.as_deref() else {
        return report;
    };
    let Ok(conn) = stats::open(db) else {
        return report;
    };
    let decisions = decisions_of(&conn, session);
    let jobs: Vec<usize> = decisions
        .iter()
        .enumerate()
        .filter(|(_, d)| d.delegated)
        .map(|(i, _)| i)
        .collect();
    report.jobs = jobs.len();
    if jobs.is_empty() {
        return report;
    }
    let runs = input.transcript_path.as_deref().and_then(read_runs);
    report.transcript_read = runs.is_some();
    for i in jobs {
        let d = &decisions[i];
        let until = decisions.get(i + 1).map(|n| n.at);
        let mut reviewed = None;
        let measurement = match (&runs, d.at) {
            (Some(runs), from) => {
                let mine: Vec<&Run> = runs
                    .iter()
                    .filter(|r| {
                        r.start
                            .is_some_and(|s| s >= from && until.map_or(true, |u| s < u))
                    })
                    .collect();
                reviewed = reviewed_success(&mine);
                measure(&mine, from, settings)
            }
            _ => Measurement::default(),
        };
        // An unmeasurable job still gets its (empty) row, so the reminder can
        // track it; unknown parts stay NULL.
        let _ = attach_on(&conn, d.id, &measurement);
        if let Some(approved) = reviewed {
            let _ = attach_reviewer_verdict(&conn, d.id, approved);
        }
    }
    report
}

struct DecisionRow {
    id: i64,
    at: DateTime<Utc>,
    delegated: bool,
}

/// The decisions of one session, oldest first.
fn decisions_of(conn: &rusqlite::Connection, session: &str) -> Vec<DecisionRow> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT id, timestamp, decision FROM router_decisions WHERE session_id = ?1 ORDER BY id",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([session], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    }) else {
        return Vec::new();
    };
    rows.filter_map(Result::ok)
        .filter_map(|(id, ts, decision)| {
            let at = DateTime::parse_from_rfc3339(&ts).ok()?.with_timezone(&Utc);
            Some(DecisionRow {
                id,
                at,
                delegated: decision == "delegated",
            })
        })
        .collect()
}

/// SessionStart: the one-line reminder, or `None` when nothing is due.
/// Each job is reminded of once.
pub fn start(
    db: &Path,
    settings: &Settings,
    input: &SessionInput,
    trusted: bool,
) -> Option<String> {
    if !gate_open(settings, input, trusted) || !db.exists() {
        return None;
    }
    let conn = stats::open(db).ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT d.id FROM router_decisions d
             LEFT JOIN router_outcomes o ON o.decision_id = d.id
             WHERE d.decision = 'delegated' AND o.success IS NULL
               AND COALESCE(o.reminded, 0) = 0
             ORDER BY d.id",
        )
        .ok()?;
    let ids: Vec<i64> = stmt
        .query_map([], |r| r.get(0))
        .ok()?
        .filter_map(Result::ok)
        .collect();
    drop(stmt);
    if ids.is_empty() {
        return None;
    }
    for id in &ids {
        let _ = conn.execute(
            "INSERT INTO router_outcomes (decision_id, reminded) VALUES (?1, 1)
             ON CONFLICT(decision_id) DO UPDATE SET reminded = 1",
            [id],
        );
    }
    let n = ids.len();
    let text = if n == 1 {
        "ecotokens: 1 delegated job has no judgment yet — run `ecotokens router outcome --pending`."
            .to_string()
    } else {
        format!(
            "ecotokens: {n} delegated jobs have no judgment yet — run `ecotokens router outcome --pending`."
        )
    };
    serde_json::to_string(&serde_json::json!({ "systemMessage": text })).ok()
}

/// Entry point of `ecotokens hook-router-session <start|end>`. Fail-open:
/// any error prints nothing and exits 0.
pub fn handle(event: &str) {
    if !matches!(event, "start" | "end") {
        return;
    }
    let settings = Settings::load();
    if !settings.router_enabled {
        return;
    }
    let mut buf = String::new();
    let limit = crate::hook::MAX_STDIN_BYTES as u64;
    if std::io::stdin()
        .take(limit + 1)
        .read_to_string(&mut buf)
        .is_err()
        || buf.len() as u64 > limit
    {
        return;
    }
    let Ok(input) = serde_json::from_str::<SessionInput>(&buf) else {
        return;
    };
    let Some(db) = stats::router_db_path() else {
        return;
    };
    let trusted = Settings::config_path().map_or(true, |p| Settings::config_file_trusted(&p));
    match event {
        "end" => {
            end(&db, &settings, &input, trusted);
        }
        _ => {
            if let Some(out) = start(&db, &settings, &input, trusted) {
                print!("{out}");
            }
        }
    }
}
