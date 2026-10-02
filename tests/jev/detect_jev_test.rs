use std::collections::HashMap;

use ecotokens::config::Settings;
use ecotokens::jev::{Answer, Answers, ChoiceAnswer, JevContext, JevError, StubJudge};
use ecotokens::rewrite::detect::{
    classify, classify_with, detect_source_language, is_predominantly_code, rewrite_gate,
    ContentKind,
};

fn choice(choice: &str, probs: &[(&str, f64)], confidence: f64) -> Answer {
    Answer::Choice(ChoiceAnswer {
        choice: choice.to_string(),
        probabilities: probs
            .iter()
            .map(|(k, p)| (k.to_string(), *p))
            .collect::<HashMap<_, _>>(),
        confidence,
    })
}

fn kind_answer(kind: &str, p: f64) -> Answers {
    let mut a = Answers::default();
    a.insert("kind", choice(kind, &[(kind, p)], p));
    a
}

const PROSE: &str = "The deployment finished late yesterday. Several services were \
                     restarted and the team verified that every dashboard looked healthy.";

#[test]
fn each_kind_maps_to_content_kind() {
    let s = Settings::default();
    for (answer, expected) in [
        ("prose", ContentKind::Prose),
        ("source_code", ContentKind::Code),
        ("stack_trace_or_error", ContentKind::Diagnostic),
        ("structured_data", ContentKind::Structured),
        ("mixed", ContentKind::Structured),
    ] {
        let judge = StubJudge::with_answers(kind_answer(answer, 0.97));
        let got = classify_with(PROSE, Some(JevContext::new(&judge, &s)));
        assert_eq!(got, expected, "answer {answer}");
    }
}

#[test]
fn uncertain_prose_is_not_prose() {
    let s = Settings::default();
    let judge = StubJudge::with_answers(kind_answer("prose", 0.6));
    assert_eq!(
        classify_with(PROSE, Some(JevContext::new(&judge, &s))),
        ContentKind::Structured
    );
}

#[test]
fn exact_signals_never_call_jev() {
    let s = Settings::default();
    let judge = StubJudge::with_answers(kind_answer("prose", 0.99));
    let ctx = Some(JevContext::new(&judge, &s));
    assert_eq!(classify_with(r#"{"a": 1}"#, ctx), ContentKind::Structured);
    assert_eq!(
        classify_with("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@", ctx),
        ContentKind::Structured
    );
    assert_eq!(
        classify_with(
            "Traceback (most recent call last):\n  File \"x.py\", line 1\nValueError",
            ctx
        ),
        ContentKind::Diagnostic
    );
    assert_eq!(
        classify_with("thread 'main' panicked at src/main.rs:3:5", ctx),
        ContentKind::Diagnostic
    );
    assert_eq!(classify_with("   ", ctx), ContentKind::Structured);
    assert_eq!(judge.call_count(), 0);
}

#[test]
fn jev_failure_or_missing_answer_equals_heuristic() {
    let s = Settings::default();
    let samples = [
        PROSE,
        "fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n}\n",
        "name = \"x\"\nversion = \"1\"\nedition = \"2021\"",
        "short",
    ];
    for text in samples {
        let failing = StubJudge::failing(JevError::Timeout);
        assert_eq!(
            classify_with(text, Some(JevContext::new(&failing, &s))),
            classify(text)
        );
        let empty = StubJudge::with_answers(Answers::default());
        assert_eq!(
            classify_with(text, Some(JevContext::new(&empty, &s))),
            classify(text)
        );
        assert_eq!(classify_with(text, None), classify(text));
    }
}

#[test]
fn gate_refuses_only_confident_source_code() {
    let s = Settings::default();
    let confident = StubJudge::with_answers(kind_answer("source_code", 0.95));
    assert!(rewrite_gate(PROSE, false, Some(JevContext::new(&confident, &s)), None).is_code);

    let unsure = StubJudge::with_answers(kind_answer("source_code", 0.5));
    assert!(!rewrite_gate(PROSE, false, Some(JevContext::new(&unsure, &s)), None).is_code);
}

#[test]
fn gate_asks_language_only_for_translate_in_one_request() {
    let s = Settings::default();
    let mut answers = kind_answer("prose", 0.95);
    answers.insert("language", choice("fr", &[("fr", 0.92)], 0.9));
    let judge = StubJudge::with_answers(answers);
    let ctx = Some(JevContext::new(&judge, &s));

    let gate = rewrite_gate(PROSE, true, ctx, None);
    assert_eq!(gate.language, Some("fr"));
    assert_eq!(judge.call_count(), 1);
    let (_, questions) = judge.last_request().unwrap();
    assert!(questions.contains_key("kind") && questions.contains_key("language"));

    let gate = rewrite_gate(PROSE, false, ctx, None);
    assert_eq!(gate.language, None);
    let (_, questions) = judge.last_request().unwrap();
    assert!(!questions.contains_key("language"));
}

#[test]
fn low_confidence_or_unclear_language_is_none() {
    let s = Settings::default();
    for (lang, conf) in [("fr", 0.5), ("mixed_or_unclear", 0.99)] {
        let mut answers = kind_answer("prose", 0.95);
        answers.insert("language", choice(lang, &[(lang, conf)], conf));
        let judge = StubJudge::with_answers(answers);
        let gate = rewrite_gate(PROSE, true, Some(JevContext::new(&judge, &s)), None);
        assert_eq!(gate.language, None, "{lang} at {conf}");
    }
}

#[test]
fn gate_without_jev_or_on_failure_matches_heuristics() {
    let s = Settings::default();
    let french = "Le chat est sur la table et nous pensons que vous avez raison pour cela.";
    let failing = StubJudge::failing(JevError::Http(529));
    for jev in [None, Some(JevContext::new(&failing, &s))] {
        let gate = rewrite_gate(french, true, jev, None);
        assert_eq!(gate.is_code, is_predominantly_code(french));
        assert_eq!(gate.language, detect_source_language(french));
    }
}
