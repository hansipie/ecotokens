//! `ecotokens router on|off|status|try|price`.

use std::path::Path;
use std::time::Duration;

use super::{agents, location, outcome, policy, route_with, stats, Decision, Routing};
use crate::config::Settings;

pub const PRIVACY_REMINDER: &str = "While the router is on, every message you send to Claude Code \
is also sent (secrets masked) to TypeSafe, the company that makes Jev, to be sized. Keep it off \
for private work.";

fn has_key() -> bool {
    matches!(std::env::var(crate::jev::API_KEY_ENV), Ok(k) if !k.trim().is_empty())
}

/// Claude Code's hard stop for the hook: the Jev timeout plus one second
/// for process start and the stats write, rounded up.
pub fn hook_timeout_secs(router_timeout_ms: u64) -> u64 {
    router_timeout_ms.div_ceil(1000) + 1
}

pub fn on(settings_path: &Path, agents_dir: &Path, timeout_ms: Option<u64>) -> std::io::Result<()> {
    let mut settings = Settings::load();
    if let Some(ms) = timeout_ms {
        settings.router_timeout_ms = ms;
    }
    let (written, kept) = agents::install_agents(agents_dir, settings.router_max_attempts)?;
    crate::install::install_prompt_hook(
        settings_path,
        hook_timeout_secs(settings.router_timeout_ms),
    )?;
    crate::install::install_router_session_hooks(settings_path)?;
    settings.router_enabled = true;
    settings.save()?;

    println!("router: ON");
    println!(
        "  hook   : UserPromptSubmit → ecotokens hook-prompt ({})",
        settings_path.display()
    );
    println!("  hooks  : SessionStart / SessionEnd → ecotokens hook-router-session");
    for path in &written {
        println!("  agent  : {}", path.display());
    }
    for path in &kept {
        eprintln!(
            "warning: {} exists and was not written by ecotokens; left untouched, so that size \
             will not be delegated correctly",
            path.display()
        );
    }
    if !has_key() {
        eprintln!(
            "warning: {} not set (env or ~/.config/ecotokens/.env); the router will do nothing",
            crate::jev::API_KEY_ENV
        );
    }
    println!(
        "  timeout: Jev {} ms per message (each message waits at most this long)",
        settings.router_timeout_ms
    );
    println!("Restart Claude Code so it loads the hook and the helper agents.");
    println!();
    println!("{PRIVACY_REMINDER}");
    Ok(())
}

pub fn off(settings_path: &Path, agents_dir: &Path) -> std::io::Result<()> {
    let mut settings = Settings::load();
    settings.router_enabled = false;
    settings.save()?;
    crate::install::uninstall_prompt_hook(settings_path)?;
    crate::install::uninstall_router_session_hooks(settings_path)?;
    let removed = agents::remove_agents(agents_dir)?;
    println!("router: OFF");
    println!("  hook   : removed from {}", settings_path.display());
    for path in &removed {
        println!("  agent  : removed {}", path.display());
    }
    Ok(())
}

pub fn status(settings_path: &Path, agents_dir: &Path, json: bool) -> std::io::Result<()> {
    let settings = Settings::load();
    let summary = match stats::router_db_path() {
        Some(path) => stats::summarize(&path, &settings)?,
        None => stats::Summary::default(),
    };
    let hook = crate::install::is_prompt_hook_installed(settings_path);
    let session_hooks = crate::install::are_router_session_hooks_installed(settings_path);
    let agents_ok = agents::are_agents_installed(agents_dir);
    let outcomes = match stats::router_db_path() {
        Some(path) => outcome::aggregates(&path, &settings)?,
        None => outcome::Aggregates::default(),
    };
    let policy_version = policy::Policy::load().version().to_string();

    if json {
        let v = serde_json::json!({
            "enabled": settings.router_enabled,
            "hook_installed": hook,
            "agents_installed": agents_ok,
            "api_key": has_key(),
            "min_confidence": settings.router_min_confidence,
            "timeout_ms": settings.router_timeout_ms,
            "summary": summary,
            "private_skipped": decision_count(&summary, Decision::Private),
            "not_allowed_skipped": decision_count(&summary, Decision::NotAllowed),
            "private_locations": settings.router_private.len(),
            "allowed_locations": settings.router_allowed.len(),
            "strict": settings.router_strict,
            "session_hooks_installed": session_hooks,
            "policy_version": policy_version,
            "by_route": outcomes.by_route,
            "by_category": outcomes.by_category,
            "by_policy": summary.by_policy,
            "outcomes": outcomes.outcomes,
        });
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        return Ok(());
    }

    println!(
        "router    : {}",
        if settings.router_enabled { "ON" } else { "OFF" }
    );
    println!(
        "setup     : hook {} · agents {} · key {}",
        yes_no(hook && session_hooks),
        yes_no(agents_ok),
        yes_no(has_key())
    );
    println!(
        "thresholds: delegate when size confidence ≥ {:.2} and follow-up prob < {:.2} · timeout {} ms",
        settings.router_min_confidence, settings.router_followup_min_prob, settings.router_timeout_ms
    );
    println!(
        "locations : {} private · {} allowed · strict {} · skipped {} private, {} not allowed",
        settings.router_private.len(),
        settings.router_allowed.len(),
        if settings.router_strict { "on" } else { "off" },
        decision_count(&summary, Decision::Private),
        decision_count(&summary, Decision::NotAllowed)
    );
    println!();
    if summary.messages == 0 {
        println!("No messages routed yet.");
        return Ok(());
    }
    if let Some(first) = &summary.first {
        println!("Since {first}: {} messages", summary.messages);
    }
    println!();
    println!("{:<10} {:>7} {:>10}  model", "size", "picked", "delegated");
    for size in super::Size::ALL {
        let c = summary
            .by_size
            .get(size.as_str())
            .cloned()
            .unwrap_or_default();
        println!(
            "{:<10} {:>7} {:>10}  {}",
            size.as_str(),
            c.picked,
            c.delegated,
            size.model_label()
        );
    }
    println!();
    for decision in Decision::ALL {
        let n = summary
            .by_decision
            .get(decision.as_str())
            .copied()
            .unwrap_or(0);
        if n > 0 {
            println!("{:<14} {n}", decision.as_str());
        }
    }
    print_outcome_tables(&outcomes, &summary, &policy_version);
    println!();
    println!(
        "Jev: {} requests · {} input + {} output tokens · avg {} ms",
        summary.jev_requests, summary.input_tokens, summary.output_tokens, summary.avg_latency_ms
    );
    match summary.cost_usd {
        Some(cost) => println!("Jev cost: ${cost:.4}"),
        None => println!(
            "Jev cost: no price set (ecotokens router price --input <usd/Mtok> --output <usd/Mtok>)"
        ),
    }
    Ok(())
}

fn decision_count(summary: &stats::Summary, decision: Decision) -> u64 {
    summary
        .by_decision
        .get(decision.as_str())
        .copied()
        .unwrap_or(0)
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

pub fn price(input: Option<f64>, output: Option<f64>) -> std::io::Result<()> {
    for (flag, value) in [("--input", input), ("--output", output)] {
        crate::config::validate_price(flag, value)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    }
    let mut settings = Settings::load();
    if input.is_some() {
        settings.jev_usd_per_mtok_input = input;
    }
    if output.is_some() {
        settings.jev_usd_per_mtok_output = output;
    }
    settings.save()?;
    println!(
        "Jev price: input {} · output {} (USD per million tokens)",
        crate::config::fmt_price(settings.jev_usd_per_mtok_input),
        crate::config::fmt_price(settings.jev_usd_per_mtok_output)
    );
    Ok(())
}

fn settings_trusted() -> bool {
    Settings::config_path().map_or(true, |p| Settings::config_file_trusted(&p))
}

/// `router try` in a folder the gate closes: say so, ask nobody.
fn report_gate(count: usize, gate: &location::Check, json: bool) -> i32 {
    let decision = if gate.not_allowed {
        Decision::NotAllowed
    } else {
        Decision::Private
    };
    if json {
        let rows: Vec<_> = (0..count)
            .map(|i| {
                serde_json::json!({
                    "message_index": i,
                    "decision": decision.as_str(),
                    "gate": if gate.not_allowed { "not_allowed" } else { "private" },
                    "reason": gate.reason.as_str(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).unwrap_or_default()
        );
        return 0;
    }
    if gate.not_allowed {
        println!(
            "not allowed (strict mode: this folder is not on the allow list): messages here are \
             not sent to Jev, nothing was asked."
        );
    } else {
        println!(
            "private location ({}): messages here are not sent to Jev, nothing was asked.",
            gate.reason.as_str()
        );
    }
    0
}

/// The card, with the reliability of each answer, as JSON.
fn card_json(routing: &Routing, settings: &Settings) -> serde_json::Value {
    let Some(card) = &routing.card else {
        return serde_json::Value::Null;
    };
    let min = settings.card_min_confidence();
    serde_json::json!({
        "nature": card.nature.map(|n| n.value.as_str()),
        "risk": card.risk.map(|r| r.value.as_str()),
        "ambiguity": card.ambiguity,
        "verifiability": card.verifiability,
        "reliability": {
            "size": card.size_reliable(min),
            "nature": card.nature_reliable(min),
            "risk": card.risk_reliable(min),
        },
    })
}

/// Sizes messages live, without recording and whether or not the router is
/// on. Returns non-zero when there is no key.
pub fn try_messages(messages: &[String], json: bool, timeout_ms: Option<u64>) -> i32 {
    let settings = Settings::load();
    let cwd = std::env::current_dir().ok();
    let gate = location::check(cwd.as_deref(), &settings, settings_trusted());
    if gate.private || gate.not_allowed {
        return report_gate(messages.len(), &gate, json);
    }
    let Some(judge) = super::judge_for_router(&settings, true) else {
        eprintln!(
            "error: Jev unavailable ({} not set, or the jev feature is off)",
            crate::jev::API_KEY_ENV
        );
        return 1;
    };
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(settings.router_timeout_ms));
    let policy = super::policy::Policy::load();
    let agents_dir = agents::default_agents_dir();
    let rows: Vec<(String, Routing)> = messages
        .iter()
        .map(|m| {
            #[cfg(feature = "jev")]
            crate::jev::client::reset_breaker();
            let mut routing = route_with(m, &settings, judge.as_ref(), timeout, &policy);
            super::apply_availability(&mut routing, agents_dir.as_deref());
            (m.clone(), routing)
        })
        .collect();

    if json {
        let v: Vec<_> = rows
            .iter()
            .enumerate()
            .map(|(i, (m, r))| {
                serde_json::json!({
                    "message_index": i,
                    "message": m,
                    "decision": r.decision.as_str(),
                    "size": r.size.map(|s| s.as_str()),
                    "confidence": r.confidence,
                    "needs_conversation": r.followup_prob,
                    "card": card_json(r, &settings),
                    "rule": r.rule,
                    "rule_source": r.matched.as_ref().map(|m| source_str(m.source)),
                    "route": r.route,
                    "raised": r.raised,
                    "policy_version": r.policy_version,
                    "agent": (r.decision == Decision::Delegated)
                        .then(|| r.route.clone()).flatten(),
                    "latency_ms": r.latency_ms,
                    "input_tokens": r.usage.map(|u| u.input_tokens),
                    "output_tokens": r.usage.map(|u| u.output_tokens),
                    "error": r.error,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        return 0;
    }

    println!(
        "{:<40} {:<9} {:>5} {:>6} {:<14} {:<11} {:<17} {:<15} {:<15} {:>6}",
        "message", "size", "conf", "follow", "nature", "risk", "rule", "route", "decision", "ms"
    );
    for (m, r) in &rows {
        let card = r.card.as_ref();
        let min = settings.card_min_confidence();
        let flag = |ok: bool| if ok { "" } else { "?" };
        println!(
            "{:<40} {:<9} {:>5} {:>6} {:<14} {:<11} {:<17} {:<15} {:<15} {:>6}",
            clip(m, 40),
            r.size.map_or("-", |s| s.as_str()),
            r.confidence.map_or("-".into(), |c| format!("{c:.2}")),
            r.followup_prob.map_or("-".into(), |p| format!("{p:.2}")),
            card.and_then(|c| c.nature)
                .map_or("-".to_string(), |n| format!(
                    "{}{}",
                    n.value.as_str(),
                    flag(n.confidence >= min)
                )),
            card.and_then(|c| c.risk)
                .map_or("-".to_string(), |x| format!(
                    "{}{}",
                    x.value.as_str(),
                    flag(x.confidence >= min)
                )),
            r.rule.as_deref().unwrap_or("-"),
            format!(
                "{}{}",
                r.route.as_deref().unwrap_or("-"),
                if r.raised { "↑" } else { "" }
            ),
            r.decision.as_str(),
            r.latency_ms
        );
        if let Some(e) = &r.error {
            println!("  ↳ {e}");
        }
    }
    println!();
    println!(
        "policy {} · `?` = answer below the {:.2} reliability threshold · `↑` = raised one tier",
        policy.version(),
        settings.card_min_confidence()
    );
    0
}

fn clip(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= max {
        s
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

// ── Locations: private list, allow list, strict mode ────────────────────

/// Which list a location command edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Private,
    Allowed,
}

impl ListKind {
    fn noun(self) -> &'static str {
        match self {
            ListKind::Private => "private location",
            ListKind::Allowed => "allowed location",
        }
    }

    fn command(self) -> &'static str {
        match self {
            ListKind::Private => "private",
            ListKind::Allowed => "allow",
        }
    }

    fn list(self, settings: &Settings) -> &Vec<String> {
        match self {
            ListKind::Private => &settings.router_private,
            ListKind::Allowed => &settings.router_allowed,
        }
    }

    fn list_mut(self, settings: &mut Settings) -> &mut Vec<String> {
        match self {
            ListKind::Private => &mut settings.router_private,
            ListKind::Allowed => &mut settings.router_allowed,
        }
    }
}

/// Settings for an edit. An unreadable file is never replaced: the router
/// would silently drop whatever the file held.
fn load_for_edit() -> std::io::Result<Settings> {
    if !settings_trusted() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "the settings file cannot be read; fix or remove it first (it was left untouched)",
        ));
    }
    Ok(Settings::load())
}

fn path_or_cwd(path: Option<&str>) -> std::io::Result<String> {
    match path {
        Some(p) => Ok(p.to_string()),
        None => Ok(std::env::current_dir()?.to_string_lossy().into_owned()),
    }
}

pub fn location_add(kind: ListKind, path: Option<&str>, json: bool) -> std::io::Result<()> {
    let raw = path_or_cwd(path)?;
    let mut settings = load_for_edit()?;
    let entry = location::normalize_entry(&raw);
    let changed = location::add_entry(kind.list_mut(&mut settings), &raw);
    if changed {
        settings.save_preserving_unknown()?;
    }
    if json {
        println!(
            "{}",
            serde_json::json!({ "entry": entry, "changed": changed })
        );
        return Ok(());
    }
    match (changed, location::entry_state(&entry)) {
        (false, _) => println!("no change: {entry} is already a {}", kind.noun()),
        (true, location::EntryState::Invalid) => {
            println!("{} added: {entry}", kind.noun());
            eprintln!("warning: `{entry}` is not a valid pattern and is ignored until fixed");
        }
        (true, location::EntryState::Missing) => {
            println!("{} added: {entry}", kind.noun());
            eprintln!("note: {entry} does not exist (yet); the entry is kept");
        }
        (true, location::EntryState::Active) => println!("{} added: {entry}", kind.noun()),
    }
    Ok(())
}

pub fn location_remove(kind: ListKind, path: Option<&str>, json: bool) -> std::io::Result<()> {
    let raw = path_or_cwd(path)?;
    let mut settings = load_for_edit()?;
    let entry = location::normalize_entry(&raw);
    let changed = location::remove_entry(kind.list_mut(&mut settings), &raw);
    if changed {
        settings.save_preserving_unknown()?;
    }
    if json {
        println!(
            "{}",
            serde_json::json!({ "entry": entry, "changed": changed })
        );
        return Ok(());
    }
    if changed {
        println!("{} removed: {entry}", kind.noun());
    } else {
        println!("no change: {entry} is not a {}", kind.noun());
    }
    Ok(())
}

pub fn location_list(kind: ListKind, json: bool) -> std::io::Result<()> {
    let settings = Settings::load();
    let entries: Vec<_> = kind
        .list(&settings)
        .iter()
        .map(|e| (e.clone(), location::entry_state(e)))
        .collect();
    if json {
        let v: Vec<_> = entries
            .iter()
            .map(|(e, st)| serde_json::json!({ "entry": e, "state": st.as_str() }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        return Ok(());
    }
    if entries.is_empty() {
        println!(
            "No {}s. Add one with `ecotokens router {} add [PATH]`.",
            kind.noun(),
            kind.command()
        );
        return Ok(());
    }
    println!("{}s ({}):", kind.noun(), entries.len());
    for (entry, state) in &entries {
        println!("  {:<8} {entry}", state.as_str());
    }
    Ok(())
}

/// `router private check [PATH]`: what the gate would do for a folder.
/// Exit 0 whichever the answer: the answer is data.
pub fn private_check(path: Option<&str>, json: bool) -> std::io::Result<()> {
    let settings = Settings::load();
    let shown = path_or_cwd(path)?;
    let check = location::check(Some(Path::new(&shown)), &settings, settings_trusted());
    if json {
        println!(
            "{}",
            serde_json::json!({
                "path": shown,
                "private": check.private,
                "not_allowed": check.not_allowed,
                "reason": check.reason.as_str(),
                "entry": check.entry,
            })
        );
        return Ok(());
    }
    if check.private {
        match &check.entry {
            Some(entry) => println!(
                "{shown}: private ({}: {entry}); messages here are not sent to Jev",
                check.reason.as_str()
            ),
            None => println!(
                "{shown}: private ({}); messages here are not sent to Jev",
                check.reason.as_str()
            ),
        }
    } else if check.not_allowed {
        println!(
            "{shown}: not private, but not allowed: strict mode is on and the folder is not on \
             the allow list; messages here are not sent to Jev"
        );
    } else {
        println!("{shown}: not private");
    }
    Ok(())
}

pub fn strict_set(on: bool, json: bool) -> std::io::Result<()> {
    let mut settings = load_for_edit()?;
    settings.router_strict = on;
    settings.save_preserving_unknown()?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "strict": on,
                "allowed": settings.router_allowed.len(),
            })
        );
        return Ok(());
    }
    if on {
        println!(
            "strict mode: ON; only the {} allowed location(s) are routed",
            settings.router_allowed.len()
        );
        if settings.router_allowed.is_empty() {
            eprintln!(
                "warning: the allow list is empty, so no message is routed; add one with \
                 `ecotokens router allow add [PATH]`"
            );
        }
    } else {
        println!("strict mode: OFF");
    }
    Ok(())
}

pub fn strict_status(json: bool) -> std::io::Result<()> {
    let settings = Settings::load();
    if json {
        println!(
            "{}",
            serde_json::json!({
                "strict": settings.router_strict,
                "allowed": settings.router_allowed.len(),
            })
        );
    } else {
        println!(
            "strict mode: {} ({} allowed location(s))",
            if settings.router_strict { "ON" } else { "OFF" },
            settings.router_allowed.len()
        );
    }
    Ok(())
}

// ── Rules ───────────────────────────────────────────────────────────────

fn source_str(source: policy::Source) -> &'static str {
    match source {
        policy::Source::User => "user",
        policy::Source::Default => "default",
    }
}

fn rule_json(rule: &policy::Rule) -> serde_json::Value {
    serde_json::json!({
        "name": rule.name,
        "when": rule.when.to_json(),
        "route": rule.route.as_str(),
        "bound": rule.bound.as_str(),
        "role": if rule.route == policy::Route::Analyst { "analyst" } else { "producer" },
        "review": rule.review.as_str(),
        "approval": rule.approval.as_str(),
        "hints": rule.hints,
        "source": source_str(rule.source),
        "valid": true,
    })
}

/// The `when` of a rule as `key=a,b key=c`, or `any` when it has no condition.
fn when_text(when: &policy::When) -> String {
    let serde_json::Value::Object(map) = when.to_json() else {
        return "any".into();
    };
    if map.is_empty() {
        return "any".into();
    }
    map.iter()
        .map(|(k, v)| match v {
            serde_json::Value::Array(items) => format!(
                "{k}={}",
                items
                    .iter()
                    .filter_map(|i| i.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            other => format!("{k}={}", other.as_str().unwrap_or("?")),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `ecotokens router rules [--json]`: the rules in evaluation order. Warnings
/// about ignored rules go to stderr only.
pub fn rules(json: bool) -> std::io::Result<()> {
    let policy = policy::Policy::load();
    for warning in policy.all_warnings() {
        eprintln!("warning: {warning}");
    }
    let user_count = policy
        .rules()
        .iter()
        .filter(|r| r.source == policy::Source::User)
        .count();
    if json {
        let mut rows: Vec<serde_json::Value> = policy
            .rules()
            .iter()
            .filter(|r| r.source == policy::Source::User)
            .map(rule_json)
            .collect();
        rows.extend(policy.invalid().iter().map(|i| {
            serde_json::json!({
                "name": i.name,
                "source": "user",
                "valid": false,
                "reason": i.reason,
            })
        }));
        rows.extend(
            policy
                .rules()
                .iter()
                .filter(|r| r.source == policy::Source::Default)
                .map(rule_json),
        );
        let v = serde_json::json!({
            "policy_version": policy.version(),
            "user_file": policy.has_user_file(),
            "rules": rows,
        });
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        return Ok(());
    }

    let default_count = policy.rules().len() - user_count;
    println!(
        "Policy {} · {} user rule(s), {} invalid · {} default rule(s)",
        policy.version(),
        user_count,
        policy.invalid().len(),
        default_count
    );
    if !policy.has_user_file() {
        match policy::Policy::user_rules_path() {
            Some(path) => println!(
                "No custom rules: the default rules apply. Create {} to add your own \
                 (user rules are evaluated first).",
                path.display()
            ),
            None => println!("No custom rules: the default rules apply."),
        }
    }
    println!();
    println!(
        "{:>2}  {:<22} {:<8} {:<10} {:<42} extras",
        "#", "name", "source", "route", "when"
    );
    let mut n = 0;
    let print_rule = |n: usize, r: &policy::Rule| {
        let mut extras = Vec::new();
        if r.bound == policy::Bound::AtLeast {
            extras.push("at-least".to_string());
        }
        if r.review != policy::Review::None {
            extras.push(format!("review={}", r.review.as_str()));
        }
        if r.approval != policy::Approval::None {
            extras.push(format!("approval={}", r.approval.as_str()));
        }
        println!(
            "{:>2}  {:<22} {:<8} {:<10} {:<42} {}",
            n,
            r.name,
            source_str(r.source),
            r.route.as_str(),
            when_text(&r.when),
            extras.join(" ")
        );
    };
    for r in policy
        .rules()
        .iter()
        .filter(|r| r.source == policy::Source::User)
    {
        n += 1;
        print_rule(n, r);
    }
    for i in policy.invalid() {
        println!(
            "{:>2}  {:<22} {:<8} invalid: {}",
            "-",
            i.name.as_deref().unwrap_or("(unnamed)"),
            "user",
            i.reason
        );
    }
    for r in policy
        .rules()
        .iter()
        .filter(|r| r.source == policy::Source::Default)
    {
        n += 1;
        print_rule(n, r);
    }
    Ok(())
}

// ── Dossier ─────────────────────────────────────────────────────────────

/// `ecotokens router dossier [--json]`: writes the six-item dossier of a
/// helper that stops, from the JSON object on standard input.
pub fn dossier(json: bool) -> std::io::Result<()> {
    use super::dossier as d;
    let invalid = |e: String| std::io::Error::new(std::io::ErrorKind::InvalidInput, e);
    let text = d::read_stdin()?;
    let input = d::parse_input(&text).map_err(invalid)?;
    let cwd = std::env::current_dir()?;
    let diff = d::collect_diff(&cwd);
    let mut handoff = d::build(&input, diff, &cwd.to_string_lossy(), chrono::Utc::now());
    handoff.branch = d::branch_of(&cwd);
    let dir = crate::handoff::store::default_dir()
        .ok_or_else(|| invalid("cannot locate the configuration directory".into()))?;
    let written = d::write(&dir, handoff)?;
    if json {
        println!(
            "{}",
            serde_json::json!({ "path": written.path, "chars": written.chars })
        );
    } else {
        print!("{}", written.markdown);
    }
    Ok(())
}

// ── Outcomes ────────────────────────────────────────────────────────────

fn opt(v: Option<f64>, f: impl Fn(f64) -> String) -> String {
    v.map_or("-".to_string(), f)
}

fn print_group_table(
    title: &str,
    groups: &std::collections::BTreeMap<String, outcome::GroupStats>,
) {
    if groups.is_empty() {
        return;
    }
    println!();
    println!(
        "{:<18} {:>9} {:>6} {:>8} {:>8} {:>8} {:>10} {:>9} {:>8} {:>5}  corrections n/m/M · regressions",
        title,
        "delegated",
        "judged",
        "by rev.",
        "success",
        "manual",
        "avg tokens",
        "avg cost",
        "avg time",
        "runs"
    );
    for (name, g) in groups {
        println!(
            "{:<18} {:>9} {:>6} {:>8} {:>8} {:>8} {:>10} {:>9} {:>8} {:>5}  {}/{}/{} · {}",
            name,
            g.delegated,
            g.judged,
            g.judged_by_reviewer,
            opt(g.success_rate, |r| format!("{:.0}%", r * 100.0)),
            opt(g.success_rate_manual, |r| format!("{:.0}%", r * 100.0)),
            opt(g.avg_tokens, |t| format!("{t:.0}")),
            opt(g.avg_cost_usd, |c| format!("${c:.4}")),
            opt(g.avg_duration_ms, |ms| format!("{:.1}s", ms / 1000.0)),
            opt(g.avg_runs, |r| format!("{r:.1}")),
            g.correction.none,
            g.correction.minor,
            g.correction.major,
            g.regressions
        );
    }
}

fn print_outcome_tables(
    outcomes: &outcome::Aggregates,
    summary: &stats::Summary,
    policy_version: &str,
) {
    print_group_table("route", &outcomes.by_route);
    print_group_table("category", &outcomes.by_category);
    if !summary.by_policy.is_empty() {
        println!();
        println!("policy (current: {policy_version})");
        for (version, c) in &summary.by_policy {
            println!("  {:<22} {:>6} messages", version, c.messages);
        }
    }
    let o = &outcomes.outcomes;
    if o.judged + o.unknown > 0 {
        println!();
        println!(
            "Outcomes: {} judged · {} unknown (excluded from success rates). \
             `by rev.` counts verdicts taken from router-reviewer; `manual` is the success \
             rate of your own judgments only. Judge a job with \
             `ecotokens router outcome --pending`.",
            o.judged, o.unknown
        );
    }
}

/// `ecotokens router outcome [ID] [--pending] [--success yes|no]
/// [--correction none|minor|major] [--regressions N] [--json]`.
pub fn outcome(
    id: Option<i64>,
    pending: bool,
    success: Option<String>,
    correction: Option<String>,
    regressions: Option<i64>,
    json: bool,
) -> std::io::Result<()> {
    let invalid = |m: String| std::io::Error::new(std::io::ErrorKind::InvalidInput, m);
    let Some(db) = stats::router_db_path() else {
        return Err(invalid("cannot locate the configuration directory".into()));
    };
    let judgment = outcome::Judgment {
        success: success.as_deref().map(|s| s == "yes"),
        correction: correction.as_deref().and_then(outcome::Correction::parse),
        regressions,
    };

    if let Some(id) = id {
        if judgment.is_empty() {
            return Err(invalid(format!(
                "nothing to record for job {id}: give --success, --correction or --regressions"
            )));
        }
        outcome::record_judgment(&db, id, &judgment)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "id": id,
                    "success": judgment.success,
                    "correction": judgment.correction.map(|c| c.as_str()),
                    "regressions": judgment.regressions,
                })
            );
        } else {
            println!("recorded the judgment of job {id}");
        }
        return Ok(());
    }

    let jobs = outcome::pending(&db)?;
    if pending {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&jobs).unwrap_or_default()
            );
        } else if jobs.is_empty() {
            println!("No delegated job awaits a judgment.");
        } else {
            println!(
                "{:>5}  {:<25} {:<16} {:<14} {:>9} {:>8}",
                "id", "date", "route", "category", "cost", "time"
            );
            for j in &jobs {
                println!(
                    "{:>5}  {:<25} {:<16} {:<14} {:>9} {:>8}",
                    j.id,
                    j.date,
                    j.route,
                    j.category.as_deref().unwrap_or("-"),
                    opt(j.cost_usd, |c| format!("${c:.4}")),
                    opt(j.duration_ms.map(|m| m as f64), |ms| format!(
                        "{:.1}s",
                        ms / 1000.0
                    )),
                );
            }
            println!();
            println!(
                "Judge one with: ecotokens router outcome <id> --success yes|no \
                 [--correction none|minor|major] [--regressions N]"
            );
        }
        return Ok(());
    }

    if json {
        println!("{}", serde_json::json!({ "pending": jobs.len() }));
    } else {
        println!(
            "Usage: ecotokens router outcome --pending\n       \
             ecotokens router outcome <id> --success yes|no [--correction none|minor|major] \
             [--regressions N]\n\n{} delegated job(s) await a judgment.",
            jobs.len()
        );
    }
    Ok(())
}
