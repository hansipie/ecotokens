//! Reference set for the task card, against the real Jev service.
//!
//! Needs `TYPESAFE_API_KEY` and the network, so it is `#[ignore]`d:
//! `cargo test --test router_live_card_test -- --ignored --nocapture`.

use std::time::Duration;

use ecotokens::config::Settings;
use ecotokens::router::card::{Nature, Risk};
use ecotokens::router::{judge_for_router, route};
use serde::Deserialize;

#[derive(Deserialize)]
struct Labelled {
    message: String,
    nature: String,
    risk: String,
}

#[test]
#[ignore = "network: needs TYPESAFE_API_KEY"]
fn the_reference_set_matches_the_labels() {
    let labelled: Vec<Labelled> =
        serde_json::from_str(include_str!("fixtures/card_reference.json")).unwrap();
    assert!(
        labelled.len() >= 40,
        "the reference set needs at least 40 messages"
    );

    let settings = Settings {
        router_enabled: true,
        ..Settings::default()
    };
    let judge = judge_for_router(&settings, true).expect("TYPESAFE_API_KEY must be set");
    let timeout = Duration::from_secs(5);

    let (mut nature_ok, mut risk_ok, mut answered) = (0, 0, 0);
    let (mut sensitive_total, mut sensitive_advanced, mut sensitive_kept, mut sensitive_cheap) =
        (0, 0, 0, 0);
    for item in &labelled {
        #[cfg(feature = "jev")]
        ecotokens::jev::client::reset_breaker();
        let routing = route(&item.message, &settings, judge.as_ref(), timeout);
        let Some(card) = routing.card else {
            eprintln!("no card for: {}", item.message);
            continue;
        };
        answered += 1;
        let nature = card.nature.map(|n| n.value);
        let risk = card.risk.map(|r| r.value);
        nature_ok += (nature == Nature::parse(&item.nature)) as usize;
        risk_ok += (risk == Risk::parse(&item.risk)) as usize;

        let labelled_sensitive = item.nature == "sensitive" || item.risk != "reversible";
        if labelled_sensitive {
            sensitive_total += 1;
            match routing.route.as_deref() {
                Some("router-large" | "router-hardest" | "router-analyst") => {
                    sensitive_advanced += 1
                }
                Some(cheap) => {
                    sensitive_cheap += 1;
                    eprintln!("sensitive message sent to {cheap}: {}", item.message);
                }
                // The size answer was not reliable (FR-005): the main session,
                // not a cheaper model, keeps the message.
                None => {
                    sensitive_kept += 1;
                    eprintln!(
                        "sensitive message kept by the main session ({}): {}",
                        routing.decision.as_str(),
                        item.message
                    );
                }
            }
        }
    }
    eprintln!(
        "nature {nature_ok}/{answered}, risk {risk_ok}/{answered}; sensitive messages: \
         {sensitive_advanced} to an advanced helper, {sensitive_kept} kept by the main session, \
         {sensitive_cheap} to a cheap helper (of {sensitive_total})"
    );
    assert!(
        answered * 100 >= labelled.len() * 90,
        "too many unanswered cards"
    );
    assert!(
        nature_ok * 100 >= answered * 85,
        "nature agreement under 85%"
    );
    assert!(risk_ok * 100 >= answered * 85, "risk agreement under 85%");
    // SC-001: a sensitive job must never end up on a cheaper model. When the
    // size answer is unreliable the main session keeps it, which is safe too.
    assert_eq!(
        sensitive_cheap, 0,
        "a message labelled sensitive was sent to a tiny or everyday helper"
    );
}
