//! `ecotokens router on|off|status|try|price`.

use std::path::Path;
use std::time::Duration;

use super::{agents, route, stats, Decision, Routing};
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
    let (written, kept) = agents::install_agents(agents_dir)?;
    crate::install::install_prompt_hook(
        settings_path,
        hook_timeout_secs(settings.router_timeout_ms),
    )?;
    settings.router_enabled = true;
    settings.save()?;

    println!("router: ON");
    println!(
        "  hook   : UserPromptSubmit → ecotokens hook-prompt ({})",
        settings_path.display()
    );
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
    let agents_ok = agents::are_agents_installed(agents_dir);

    if json {
        let v = serde_json::json!({
            "enabled": settings.router_enabled,
            "hook_installed": hook,
            "agents_installed": agents_ok,
            "api_key": has_key(),
            "min_confidence": settings.router_min_confidence,
            "timeout_ms": settings.router_timeout_ms,
            "summary": summary,
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
        yes_no(hook),
        yes_no(agents_ok),
        yes_no(has_key())
    );
    println!(
        "thresholds: delegate when size confidence ≥ {:.2} and follow-up prob < {:.2} · timeout {} ms",
        settings.router_min_confidence, settings.router_followup_min_prob, settings.router_timeout_ms
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

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

pub fn price(input: Option<f64>, output: Option<f64>) -> std::io::Result<()> {
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
        fmt_price(settings.jev_usd_per_mtok_input),
        fmt_price(settings.jev_usd_per_mtok_output)
    );
    Ok(())
}

fn fmt_price(p: Option<f64>) -> String {
    p.map_or_else(|| "unset".into(), |v| format!("${v}"))
}

/// Sizes messages live, without recording and whether or not the router is
/// on. Returns non-zero when there is no key.
pub fn try_messages(messages: &[String], json: bool, timeout_ms: Option<u64>) -> i32 {
    let settings = Settings::load();
    let Some(judge) = super::judge_for_router(&settings, true) else {
        eprintln!(
            "error: Jev unavailable ({} not set, or the jev feature is off)",
            crate::jev::API_KEY_ENV
        );
        return 1;
    };
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(settings.router_timeout_ms));
    let rows: Vec<(String, Routing)> = messages
        .iter()
        .map(|m| {
            #[cfg(feature = "jev")]
            crate::jev::client::reset_breaker();
            (m.clone(), route(m, &settings, judge.as_ref(), timeout))
        })
        .collect();

    if json {
        let v: Vec<_> = rows
            .iter()
            .map(|(m, r)| {
                serde_json::json!({
                    "message": m,
                    "decision": r.decision.as_str(),
                    "size": r.size.map(|s| s.as_str()),
                    "confidence": r.confidence,
                    "needs_conversation": r.followup_prob,
                    "agent": (r.decision == Decision::Delegated)
                        .then(|| r.size.map(|s| s.agent_name())).flatten(),
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
        "{:<48} {:<9} {:>5} {:>6}  {:<15} {:>6}",
        "message", "size", "conf", "follow", "decision", "ms"
    );
    for (m, r) in &rows {
        println!(
            "{:<48} {:<9} {:>5} {:>6}  {:<15} {:>6}",
            clip(m, 48),
            r.size.map_or("-", |s| s.as_str()),
            r.confidence.map_or("-".into(), |c| format!("{c:.2}")),
            r.followup_prob.map_or("-".into(), |p| format!("{p:.2}")),
            r.decision.as_str(),
            r.latency_ms
        );
        if let Some(e) = &r.error {
            println!("  ↳ {e}");
        }
    }
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
