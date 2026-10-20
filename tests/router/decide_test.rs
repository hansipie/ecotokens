#[path = "common.rs"]
mod common;
use common::{answers, router_settings};

use std::time::Duration;

use ecotokens::jev::{Answers, JevError, Question, StubJudge};
use ecotokens::router::{
    decide, delegation_context, questions, route, skip_reason, state_for, Decision, Routing, Size,
};

const T: Duration = Duration::from_millis(500);

#[test]
fn each_size_is_delegated_to_its_helper() {
    let settings = router_settings();
    for size in Size::ALL {
        let r = decide(&answers(size.as_str(), 0.9, 0.05), &settings);
        assert_eq!(r.decision, Decision::Delegated);
        assert_eq!(r.size, Some(size));
        let ctx = delegation_context(&r).expect("delegated message gets context");
        assert!(ctx.contains(size.agent_name()), "{ctx}");
        assert!(ctx.contains(size.model_label()), "{ctx}");
    }
}

#[test]
fn sizes_map_to_models_smallest_to_biggest() {
    let models: Vec<_> = Size::ALL.iter().map(|s| s.model_alias()).collect();
    assert_eq!(models, ["haiku", "sonnet", "opus", "fable"]);
}

#[test]
fn below_sixty_percent_the_main_session_keeps_it() {
    let settings = router_settings();
    let r = decide(&answers("tiny", 0.59, 0.05), &settings);
    assert_eq!(r.decision, Decision::SelfUnsure);
    assert_eq!(delegation_context(&r), None);

    let r = decide(&answers("tiny", 0.60, 0.05), &settings);
    assert_eq!(r.decision, Decision::Delegated);
}

#[test]
fn follow_up_replies_stay_in_the_conversation() {
    let settings = router_settings();
    let r = decide(&answers("tiny", 0.99, 0.95), &settings);
    assert_eq!(r.decision, Decision::SelfFollowup);
    assert_eq!(delegation_context(&r), None);
}

#[test]
fn missing_or_unknown_answers_are_errors() {
    let settings = router_settings();
    let r = decide(&Answers::default(), &settings);
    assert_eq!(r.decision, Decision::Error);
    assert_eq!(delegation_context(&r), None);

    let r = decide(&answers("gigantic", 0.99, 0.01), &settings);
    assert_eq!(r.decision, Decision::Error);
}

#[test]
fn route_reports_service_failures() {
    let settings = router_settings();
    let judge = StubJudge::failing(JevError::Timeout);
    let r = route("write an email", &settings, &judge, T);
    assert_eq!(r.decision, Decision::Error);
    assert!(r.service_failure);

    let judge = StubJudge::failing(JevError::Http(422));
    let r = route("write an email", &settings, &judge, T);
    assert_eq!(r.decision, Decision::Error);
    assert!(!r.service_failure);
}

#[test]
fn commands_and_empty_messages_never_reach_jev() {
    let settings = router_settings();
    let judge = StubJudge::with_answers(answers("tiny", 1.0, 0.0));
    for prompt in [
        "",
        "   ",
        "/clear",
        "  /plan",
        "!ls -la",
        "<task-notification>\n<task-id>b1</task-id>\n</task-notification>",
        "Another Claude session sent a message:\n<agent-message from=\"a1\">\n[Subagent hand-back] The text below is the final report of a subagent...",
    ] {
        assert!(skip_reason(prompt).is_some(), "{prompt:?}");
        let r = route(prompt, &settings, &judge, T);
        assert_eq!(r.decision, Decision::Skipped);
    }
    assert_eq!(judge.call_count(), 0);
}

#[test]
fn messages_that_only_mention_harness_markers_are_still_routed() {
    for prompt in [
        "what is a <task-notification> in Claude Code?",
        "Another Claude session sent a message, can you explain it?",
    ] {
        assert!(skip_reason(prompt).is_none(), "{prompt:?}");
    }
}

#[test]
fn one_request_carries_both_questions_and_the_message() {
    let settings = router_settings();
    let judge = StubJudge::with_answers(answers("large", 0.9, 0.1));
    let r = route("build a CLI", &settings, &judge, T);
    assert_eq!(r.decision, Decision::Delegated);
    assert_eq!(judge.call_count(), 1);
    let (state, qs) = judge.last_request().unwrap();
    assert_eq!(state, state_for("build a CLI"));
    assert_eq!(qs, questions());
    match &qs["size"] {
        Question::Choice { criteria, .. } => {
            let keys: Vec<_> = criteria.keys().cloned().collect();
            assert_eq!(keys, ["everyday", "hardest", "large", "tiny"]);
        }
        other => panic!("size must be a Choice, got {other:?}"),
    }
    assert!(matches!(qs["needs_conversation"], Question::Noul { .. }));
}

#[test]
fn target_label_names_helper_or_self_decision() {
    let settings = router_settings();
    let delegated = decide(&answers("everyday", 0.9, 0.05), &settings);
    assert_eq!(delegated.target_label().as_deref(), Some("router-everyday"));

    let unsure = decide(&answers("everyday", 0.1, 0.05), &settings);
    assert_eq!(unsure.decision, Decision::SelfUnsure);
    assert_eq!(unsure.target_label().as_deref(), Some("self (unsure)"));

    let followup = decide(&answers("everyday", 0.9, 0.99), &settings);
    assert_eq!(followup.decision, Decision::SelfFollowup);
    assert_eq!(followup.target_label().as_deref(), Some("self (followup)"));
}

#[test]
fn gate_and_route_decisions_round_trip_and_never_produce_guidance() {
    for (decision, name) in [
        (Decision::Private, "private"),
        (Decision::NotAllowed, "not_allowed"),
        (Decision::NoRoute, "no_route"),
    ] {
        assert_eq!(decision.as_str(), name);
        assert_eq!(Decision::parse(name), Some(decision));
        assert!(Decision::ALL.contains(&decision), "{name} listed in ALL");
        let routing = Routing::without_jev(decision);
        assert_eq!(delegation_context(&routing), None, "{name}");
        assert_eq!(routing.target_label(), None, "{name}");
    }
    assert_eq!(Decision::ALL.len(), 9);
}

#[test]
fn one_request_carries_the_six_question_ids() {
    use common::CountingJudge;
    let settings = router_settings();
    let judge = CountingJudge::new(answers("large", 0.9, 0.1));
    let r = route("build a CLI", &settings, &judge, T);
    assert_eq!(r.decision, Decision::Delegated);
    assert_eq!(
        judge.request_count(),
        1,
        "exactly one request per message (SC-005)"
    );
    let mut ids = judge.question_ids().remove(0);
    ids.sort();
    assert_eq!(
        ids,
        [
            "ambiguity",
            "nature",
            "needs_conversation",
            "risk",
            "size",
            "verifiability"
        ]
    );
}

#[test]
fn the_new_questions_have_the_documented_shapes() {
    let qs = questions();
    match &qs["nature"] {
        Question::Choice { criteria, .. } => {
            let keys: Vec<_> = criteria.keys().cloned().collect();
            assert_eq!(
                keys,
                [
                    "bug",
                    "design",
                    "implementation",
                    "mechanical",
                    "research",
                    "sensitive",
                    "ui"
                ]
            );
        }
        other => panic!("nature must be a Choice, got {other:?}"),
    }
    match &qs["risk"] {
        Question::Choice { criteria, .. } => {
            let keys: Vec<_> = criteria.keys().cloned().collect();
            assert_eq!(keys, ["destructive", "reversible", "sensitive"]);
        }
        other => panic!("risk must be a Choice, got {other:?}"),
    }
    assert!(matches!(qs["ambiguity"], Question::Noul { .. }));
    assert!(matches!(qs["verifiability"], Question::Noul { .. }));
}

#[test]
fn a_decision_fills_the_card_rule_route_and_policy_version() {
    let settings = router_settings();
    let r = decide(
        &common::answers_card("tiny", "bug", "reversible", 0.1, 0.9, 0.05),
        &settings,
    );
    assert_eq!(r.decision, Decision::Delegated);
    assert_eq!(r.rule.as_deref(), Some("bounded-bug"));
    assert_eq!(r.route.as_deref(), Some("router-everyday"));
    assert_eq!(r.policy_version.as_deref(), Some("2026.1"));
    assert!(!r.raised);
    assert!(r.card.is_some());
    assert_eq!(r.target_label().as_deref(), Some("router-everyday"));
}

#[test]
fn an_unreliable_risk_raises_and_records_it() {
    let settings = router_settings();
    let a = common::answers_card_with(
        "tiny",
        "bug",
        "reversible",
        0.1,
        0.9,
        0.05,
        common::Confidences {
            risk: 0.2,
            ..Default::default()
        },
    );
    let r = decide(&a, &settings);
    assert_eq!(r.decision, Decision::Delegated);
    assert!(r.raised);
    assert_eq!(r.route.as_deref(), Some("router-everyday"));
    assert_eq!(r.rule.as_deref(), Some("size-only"));
}
