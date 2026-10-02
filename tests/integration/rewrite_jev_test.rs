use std::collections::HashMap;
use std::time::Duration;

use ecotokens::config::Settings;
use ecotokens::jev::{
    Answer, Answers, ChoiceAnswer, JevContext, JevError, StubJudge, StubJudgeOutcome,
};
use ecotokens::rewrite::provider::StubProvider;
use ecotokens::rewrite::{rewrite, rewrite_with_judge, Mode, Origin, RewriteRequest, Status};

const INPUT: &str = "The meeting moved to Thursday because several people were travelling \
                     and could not join the call on Monday morning.";
const OUTPUT: &str = "Because several people were travelling and unable to join on Monday \
                      morning, the meeting was moved to Thursday.";

fn request(text: &str, mode: Mode) -> RewriteRequest {
    RewriteRequest {
        text: text.to_string(),
        mode,
        model: "stub".into(),
        timeout: Duration::from_secs(5),
        origin: Origin::Cli,
        truncation_ratio: 0.5,
        context_tokens: 8192,
        save_diff: false,
        diff_dir: std::env::temp_dir(),
        diff_retention: 0,
    }
}

fn prose_gate() -> Answers {
    let mut a = Answers::default();
    a.insert(
        "kind",
        Answer::Choice(ChoiceAnswer {
            choice: "prose".into(),
            probabilities: HashMap::from([("prose".to_string(), 0.97)]),
            confidence: 0.95,
        }),
    );
    a
}

fn verdict(faithful: f64, first: f64, last: f64) -> Answers {
    let mut a = Answers::default();
    a.insert("faithful", Answer::Noul(faithful))
        .insert("first_line_commentary", Answer::Noul(first))
        .insert("last_line_commentary", Answer::Noul(last));
    a
}

/// Gate request first, then one verification request.
fn judge(gate: Answers, verify: Answers) -> StubJudge {
    let j = StubJudge::new();
    j.push(StubJudgeOutcome::Answers(gate));
    j.push(StubJudgeOutcome::Answers(verify));
    j
}

#[test]
fn faithful_response_is_used() {
    let s = Settings::default();
    let j = judge(prose_gate(), verdict(0.95, 0.0, 0.0));
    let provider = StubProvider::with_response(OUTPUT);
    let result = rewrite_with_judge(
        request(INPUT, Mode::Paraphrase),
        &provider,
        Some(JevContext::new(&j, &s)),
    )
    .unwrap();
    assert_eq!(result.status, Status::Transformed);
    assert_eq!(result.text, OUTPUT);
    assert_eq!(j.call_count(), 2);
}

#[test]
fn unfaithful_response_falls_back_to_original() {
    let s = Settings::default();
    let j = judge(prose_gate(), verdict(0.05, 0.0, 0.0));
    // Long enough to pass the structural truncation check, but says something else.
    let provider = StubProvider::with_response(
        "The meeting was cancelled for good because nobody wanted to attend any of the \
         calls, on Monday or on any other day of the week.",
    );
    let result = rewrite_with_judge(
        request(INPUT, Mode::Paraphrase),
        &provider,
        Some(JevContext::new(&j, &s)),
    )
    .unwrap();
    assert_eq!(result.status, Status::Fallback);
    assert_eq!(result.text, INPUT);
    assert!(result.reason.unwrap().contains("unfaithful"));
}

#[test]
fn commentary_lines_flagged_by_jev_are_stripped_in_any_language() {
    let s = Settings::default();
    let j = judge(prose_gate(), verdict(0.9, 0.95, 0.9));
    // A French preamble the English-only regex would not recognise.
    let response = format!("Voici le texte reformulé :\n{OUTPUT}\nN'hésitez pas si besoin.");
    let provider = StubProvider::with_response(response);
    let result = rewrite_with_judge(
        request(INPUT, Mode::Paraphrase),
        &provider,
        Some(JevContext::new(&j, &s)),
    )
    .unwrap();
    assert_eq!(result.status, Status::Transformed);
    assert_eq!(result.text, OUTPUT);
}

#[test]
fn wrong_target_language_falls_back() {
    let s = Settings::default();
    let mut v = verdict(0.9, 0.0, 0.0);
    v.insert("in_target", Answer::Noul(0.02));
    let j = judge(prose_gate(), v);
    let provider = StubProvider::with_response(OUTPUT);
    let result = rewrite_with_judge(
        request(
            INPUT,
            Mode::Translate {
                target: "de".into(),
            },
        ),
        &provider,
        Some(JevContext::new(&j, &s)),
    )
    .unwrap();
    assert_eq!(result.status, Status::Fallback);
    assert!(result.reason.unwrap().contains("target language"));
}

#[test]
fn same_language_translation_is_a_no_op_via_jev() {
    let s = Settings::default();
    let mut gate = prose_gate();
    gate.insert(
        "language",
        Answer::Choice(ChoiceAnswer {
            choice: "en".into(),
            probabilities: HashMap::from([("en".to_string(), 0.98)]),
            confidence: 0.97,
        }),
    );
    let j = StubJudge::with_answers(gate);
    let provider = StubProvider::with_response("unused");
    let result = rewrite_with_judge(
        request(
            INPUT,
            Mode::Translate {
                target: "en".into(),
            },
        ),
        &provider,
        Some(JevContext::new(&j, &s)),
    )
    .unwrap();
    assert_eq!(result.status, Status::NoOp);
    assert_eq!(provider.call_count(), 0);
}

#[test]
fn jev_failure_gives_the_same_result_as_plain_rewrite() {
    let s = Settings::default();
    let responses = [
        OUTPUT.to_string(),
        format!("Here is the paraphrased text:\n{OUTPUT}"),
        "too short".to_string(),
    ];
    for response in responses {
        let failing = StubJudge::failing(JevError::Timeout);
        let with_jev = rewrite_with_judge(
            request(INPUT, Mode::Paraphrase),
            &StubProvider::with_response(response.clone()),
            Some(JevContext::new(&failing, &s)),
        )
        .unwrap();
        let plain = rewrite(
            request(INPUT, Mode::Paraphrase),
            &StubProvider::with_response(response.clone()),
        )
        .unwrap();
        assert_eq!(with_jev.status, plain.status, "response {response:?}");
        assert_eq!(with_jev.text, plain.text, "response {response:?}");
        assert_eq!(with_jev.reason, plain.reason, "response {response:?}");
    }
}
