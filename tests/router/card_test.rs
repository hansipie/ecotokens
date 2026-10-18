#[path = "common.rs"]
mod common;
use common::{answers_card, answers_card_with, router_settings, Confidences};

use ecotokens::config::Settings;
use ecotokens::jev::Answers;
use ecotokens::router::card::{Fallback, Nature, Risk, TaskCard};
use ecotokens::router::policy::{Approval, Policy, Resolution, Review, Role};
use ecotokens::router::Size;

fn card(answers: &Answers) -> TaskCard {
    TaskCard::from_answers(answers).expect("size answered")
}

fn conf(size: f64, nature: f64, risk: f64) -> Confidences {
    Confidences { size, nature, risk }
}

#[test]
fn the_six_answers_become_a_card() {
    let c = card(&answers_card("large", "bug", "sensitive", 0.2, 0.8, 0.05));
    assert_eq!(c.size, Size::Large);
    assert_eq!(c.nature.unwrap().value, Nature::Bug);
    assert_eq!(c.risk.unwrap().value, Risk::Sensitive);
    assert_eq!(c.ambiguity, Some(0.2));
    assert_eq!(c.verifiability, Some(0.8));
}

#[test]
fn unknown_or_missing_values_are_treated_as_missing() {
    let c = card(&answers_card("tiny", "astrology", "spicy", 0.2, 0.8, 0.05));
    assert!(c.nature.is_none());
    assert!(c.risk.is_none());

    let mut partial = Answers::default();
    partial.insert(
        "size",
        ecotokens::jev::Answer::Choice(ecotokens::jev::ChoiceAnswer {
            choice: "tiny".into(),
            probabilities: Default::default(),
            confidence: 0.9,
        }),
    );
    let c = card(&partial);
    assert!(c.nature.is_none() && c.risk.is_none());
    assert_eq!((c.ambiguity, c.verifiability), (None, None));
    assert_eq!(
        c.fallback(0.6),
        Fallback::Raise,
        "no risk answer is no reliable risk"
    );

    assert!(
        TaskCard::from_answers(&Answers::default()).is_none(),
        "no size, no card"
    );
    assert!(
        TaskCard::from_answers(&answers_card(
            "gigantic",
            "bug",
            "reversible",
            0.1,
            0.9,
            0.0
        ))
        .is_none(),
        "unknown size"
    );
}

#[test]
fn reliability_uses_the_card_threshold() {
    let c = card(&answers_card_with(
        "large",
        "bug",
        "reversible",
        0.1,
        0.9,
        0.0,
        conf(0.9, 0.7, 0.55),
    ));
    assert!(c.nature_reliable(0.6));
    assert!(!c.risk_reliable(0.6));
    assert!(c.risk_reliable(0.5), "a lower threshold trusts it");
    assert!(!c.nature_reliable(0.75));

    let mut settings = router_settings();
    settings.router_card_min_confidence = Some(0.8);
    assert_eq!(c.fallback(settings.card_min_confidence()), Fallback::Raise);
}

#[test]
fn the_fallback_matrix_follows_the_precedence_size_then_risk_then_nature() {
    let min = 0.6;
    let cases = [
        // (size, nature, risk) confidences → fallback
        (conf(0.9, 0.9, 0.9), Fallback::Rules),
        (conf(0.5, 0.9, 0.9), Fallback::MainSession),
        (conf(0.5, 0.1, 0.1), Fallback::MainSession),
        (conf(0.9, 0.9, 0.5), Fallback::Raise),
        (conf(0.9, 0.1, 0.5), Fallback::Raise),
        (conf(0.9, 0.5, 0.9), Fallback::SizeOnly),
    ];
    for (c, expected) in cases {
        let card = card(&answers_card_with(
            "everyday",
            "bug",
            "reversible",
            0.1,
            0.9,
            0.0,
            c,
        ));
        assert_eq!(card.fallback(min), expected, "{c:?}");
    }
}

fn resolve(answers: &Answers) -> Resolution {
    Policy::default_policy().resolve(&card(answers), &Settings::default())
}

#[test]
fn an_unreliable_size_leaves_the_message_to_the_main_session() {
    let r = resolve(&answers_card_with(
        "large",
        "bug",
        "reversible",
        0.1,
        0.9,
        0.0,
        conf(0.4, 0.9, 0.9),
    ));
    assert_eq!(r, Resolution::MainSession);
}

#[test]
fn an_unreliable_risk_raises_the_size_only_route_by_one_tier() {
    for (size, expected) in [
        ("tiny", Size::Everyday),
        ("everyday", Size::Large),
        ("large", Size::Hardest),
        ("hardest", Size::Hardest),
    ] {
        let Resolution::Matched { matched, raised } = resolve(&answers_card_with(
            size,
            "bug",
            "reversible",
            0.1,
            0.9,
            0.0,
            conf(0.9, 0.9, 0.3),
        )) else {
            panic!("{size}: expected a match");
        };
        assert_eq!(matched.tier, expected, "{size}");
        assert_eq!(
            matched.rule, "size-only",
            "{size}: the rules are not consulted"
        );
        assert!(raised, "{size}");
    }
}

#[test]
fn a_precautionary_raise_changes_no_constraint() {
    let Resolution::Matched { matched, raised } = resolve(&answers_card_with(
        "hardest",
        "bug",
        "sensitive",
        0.1,
        0.9,
        0.0,
        conf(0.9, 0.9, 0.3),
    )) else {
        panic!("expected a match");
    };
    assert!(raised);
    assert_eq!(matched.tier, Size::Hardest, "never above hardest");
    assert_eq!(matched.role, Role::Producer);
    assert_eq!(matched.review, Review::None);
    assert_eq!(matched.approval, Approval::None);
    assert!(matched.hints.is_empty());
}

#[test]
fn an_unreliable_nature_only_blocks_the_rules_that_need_it() {
    // Research would go to the analyst, but with the nature in doubt the
    // size-only rule decides.
    let Resolution::Matched { matched, raised } = resolve(&answers_card_with(
        "large",
        "research",
        "reversible",
        0.1,
        0.9,
        0.0,
        conf(0.9, 0.4, 0.9),
    )) else {
        panic!("expected a match");
    };
    assert_eq!(matched.rule, "size-only");
    assert_eq!(matched.tier, Size::Large);
    assert!(!raised);

    // A rule that reads only the risk still applies.
    let Resolution::Matched { matched, .. } = resolve(&answers_card_with(
        "everyday",
        "bug",
        "sensitive",
        0.1,
        0.9,
        0.0,
        conf(0.9, 0.4, 0.9),
    )) else {
        panic!("expected a match");
    };
    assert_eq!(matched.rule, "sensitive-work");
    assert_eq!(matched.tier, Size::Large);
}
