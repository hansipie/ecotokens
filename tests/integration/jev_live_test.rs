//! Live Jev calls — run with `TYPESAFE_API_KEY=… cargo test --test jev_live_test -- --ignored`.
//! Used to sanity-check the default thresholds against the real model.

use ecotokens::config::Settings;
use ecotokens::jev::{judge_from_parts, JevContext};
use ecotokens::rewrite::detect::{classify_with, rewrite_gate, ContentKind};

fn live_settings() -> Settings {
    Settings {
        jev_enabled: true,
        jev_timeout_ms: 5_000,
        ..Settings::default()
    }
}

fn live_judge(s: &Settings) -> Box<dyn ecotokens::jev::Judge> {
    judge_from_parts(
        true,
        s.jev_url.as_deref(),
        std::env::var(ecotokens::jev::API_KEY_ENV).ok(),
        s.jev_max_input_chars,
    )
    .expect("set TYPESAFE_API_KEY to run live Jev tests")
}

#[test]
#[ignore]
fn live_classification_matches_obvious_samples() {
    let s = live_settings();
    let judge = live_judge(&s);
    let ctx = Some(JevContext::new(judge.as_ref(), &s));

    let prose = "We moved the release to next week. The migration needs one more review, \
                 and the documentation still describes the old configuration format.";
    let code = "fn main() {\n    let total: i32 = (0..10).sum();\n    println!(\"{total}\");\n}";
    let trace = "Exception in thread \"main\" java.lang.NullPointerException\n    \
                 at com.example.App.run(App.java:42)\n    at com.example.App.main(App.java:10)";

    assert_eq!(classify_with(prose, ctx), ContentKind::Prose);
    assert_eq!(classify_with(code, ctx), ContentKind::Code);
    assert_eq!(classify_with(trace, ctx), ContentKind::Diagnostic);
}

#[test]
#[ignore]
fn live_language_detection() {
    let s = live_settings();
    let judge = live_judge(&s);
    let ctx = Some(JevContext::new(judge.as_ref(), &s));
    let french = "Nous avons repoussé la mise en production à la semaine prochaine, \
                  car la migration doit encore être relue.";
    assert_eq!(rewrite_gate(french, true, ctx, None).language, Some("fr"));
}
