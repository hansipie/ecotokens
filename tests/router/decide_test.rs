#[path = "common.rs"]
mod common;
use common::{answers, router_settings};

use std::time::Duration;

use ecotokens::jev::{Answers, JevError, Question, StubJudge};
use ecotokens::router::{
    decide, delegation_context, questions, route, skip_reason, state_for, Decision, Size,
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
    for prompt in ["", "   ", "/clear", "  /plan", "!ls -la"] {
        assert!(skip_reason(prompt).is_some(), "{prompt:?}");
        let r = route(prompt, &settings, &judge, T);
        assert_eq!(r.decision, Decision::Skipped);
    }
    assert_eq!(judge.call_count(), 0);
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
