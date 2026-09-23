use ecotokens::config::Settings;
use ecotokens::doctor::{check_jev, DoctorStatus};
use ecotokens::jev::client::{
    build_request, parse_response, prepare_state, sample_text, HttpJudge,
};
use ecotokens::jev::{judge_from_parts, JevError, Question, Questions, MODEL};
use serde_json::json;

#[test]
fn build_request_has_api_shape() {
    let mut questions = Questions::new();
    questions.insert("urgent".into(), Question::noul("Is this urgent?"));
    questions.insert(
        "kind".into(),
        Question::choice("Which kind?", [("a", "Option A"), ("b", "Option B")]),
    );
    questions.insert(
        "human".into(),
        Question::noul_with("Wants a human?", "asks for a person", "no such request"),
    );
    let body = build_request(&json!({"text": "hi"}), &questions);

    assert_eq!(body["model"], MODEL);
    assert_eq!(body["state"]["text"], "hi");
    assert_eq!(body["questions"]["urgent"]["type"], "noul");
    assert!(body["questions"]["urgent"].get("criteria").is_none());
    assert_eq!(body["questions"]["kind"]["type"], "choice");
    assert_eq!(body["questions"]["kind"]["criteria"]["b"], "Option B");
    assert_eq!(
        body["questions"]["human"]["criteria"]["true"],
        "asks for a person"
    );
    assert_eq!(
        body["questions"]["human"]["criteria"]["false"],
        "no such request"
    );
}

#[test]
fn parse_response_reads_all_answer_types() {
    let body = r#"{
        "model": "jev-1.13.0",
        "answers": {
            "urgent": {"type": "noul", "noul": 0.95},
            "kind": {"type": "choice", "choice": "b", "confidence": 0.7,
                     "probabilities": {"a": 0.2, "b": 0.8}},
            "level": {"type": "score", "score": 2.4, "confidence": 0.6,
                      "legend": {}, "probabilities": {}}
        },
        "usage": {"input_tokens": 10, "output_tokens": 3}
    }"#;
    let answers = parse_response(body).expect("valid response");
    assert_eq!(answers.noul("urgent"), Some(0.95));
    let kind = answers.choice("kind").expect("choice answer");
    assert_eq!(kind.choice, "b");
    assert!((kind.prob("b") - 0.8).abs() < 1e-9);
    assert_eq!(kind.prob("missing"), 0.0);
    assert!((kind.confidence - 0.7).abs() < 1e-9);
    // Wrong-typed lookups are `None`, never a panic.
    assert_eq!(answers.noul("kind"), None);
    assert!(answers.choice("urgent").is_none());
    assert!(answers.choice("absent").is_none());
}

#[test]
fn parse_response_rejects_malformed_bodies() {
    assert!(matches!(
        parse_response("not json"),
        Err(JevError::BadResponse(_))
    ));
    assert!(matches!(
        parse_response(r#"{"model": "x"}"#),
        Err(JevError::BadResponse(_))
    ));
    assert!(matches!(
        parse_response(r#"{"answers": {"q": {"type": "noul"}}}"#),
        Err(JevError::BadResponse(_))
    ));
}

#[test]
fn prepare_state_masks_secrets_in_every_string() {
    let token = format!("ghp_{}", "a1B2".repeat(9));
    let state = json!({
        "text": format!("push failed with token {token}"),
        "nested": [{"line": format!("GITHUB_TOKEN={token}")}],
        "count": 3
    });
    let prepared = prepare_state(state, 100_000);
    let serialized = prepared.to_string();
    assert!(!serialized.contains(&token), "secret leaked: {serialized}");
    assert_eq!(prepared["count"], 3);
}

#[test]
fn prepare_state_caps_total_size() {
    let long = "word ".repeat(10_000);
    let prepared = prepare_state(json!({"a": long.clone(), "b": long}), 2_000);
    let a = prepared["a"].as_str().unwrap().chars().count();
    let b = prepared["b"].as_str().unwrap().chars().count();
    assert!(a + b <= 2_000, "total {} exceeds cap", a + b);
}

#[test]
fn sample_text_keeps_head_middle_and_tail() {
    let text: String = (0..1000).map(|i| format!("{i:04}\n")).collect();
    let sampled = sample_text(&text, 300);
    assert!(sampled.chars().count() <= 300);
    assert!(sampled.starts_with("0000"));
    assert!(sampled.trim_end().ends_with("0999"));
    assert!(sampled.contains("050"), "middle sample missing: {sampled}");
    assert_eq!(sample_text("short", 300), "short");
}

#[test]
fn http_judge_requires_https() {
    assert!(HttpJudge::new(None, "k".into(), 1000).is_ok());
    assert!(HttpJudge::new(Some("https://example.test/v1"), "k".into(), 1000).is_ok());
    assert!(HttpJudge::new(Some("http://example.test/v1"), "k".into(), 1000).is_err());
    assert!(HttpJudge::new(Some("not a url"), "k".into(), 1000).is_err());
}

#[test]
fn http_judge_debug_hides_api_key() {
    let judge = HttpJudge::new(None, "super-secret-key".into(), 1000).unwrap();
    assert!(!format!("{judge:?}").contains("super-secret-key"));
}

#[test]
fn judge_is_none_unless_enabled_with_a_key() {
    assert!(judge_from_parts(false, None, Some("k".into()), 1000).is_none());
    assert!(judge_from_parts(true, None, None, 1000).is_none());
    assert!(judge_from_parts(true, None, Some("   ".into()), 1000).is_none());
    assert!(judge_from_parts(true, Some("http://insecure.test"), Some("k".into()), 1000).is_none());
    assert!(judge_from_parts(true, None, Some("k".into()), 1000).is_some());
}

#[test]
fn transport_failures_trip_the_breaker_request_errors_do_not() {
    assert!(JevError::Timeout.trips_breaker());
    assert!(JevError::Transport("refused".into()).trips_breaker());
    assert!(JevError::Http(401).trips_breaker());
    assert!(JevError::Http(529).trips_breaker());
    assert!(!JevError::Http(422).trips_breaker());
    assert!(!JevError::BadResponse("x".into()).trips_breaker());
}

#[test]
fn doctor_reports_jev_state_without_leaking_the_key() {
    let mut s = Settings::default();
    let off = check_jev(&s, Some("secret-key"));
    assert_eq!(off.status, DoctorStatus::Ok);
    assert!(off.message.contains("disabled"));

    s.jev_enabled = true;
    let no_key = check_jev(&s, None);
    assert_eq!(no_key.status, DoctorStatus::Warning);

    let on = check_jev(&s, Some("secret-key"));
    assert_eq!(on.status, DoctorStatus::Ok);
    assert!(on.message.contains("enabled"));
    assert!(!on.message.contains("secret-key"));
}
