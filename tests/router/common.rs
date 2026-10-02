use std::collections::HashMap;

use ecotokens::config::Settings;
use ecotokens::jev::{Answer, Answers, ChoiceAnswer};

/// Answers for the router's two questions.
pub fn answers(size: &str, confidence: f64, followup: f64) -> Answers {
    let mut a = Answers::default();
    a.insert(
        "size",
        Answer::Choice(ChoiceAnswer {
            choice: size.to_string(),
            probabilities: HashMap::from([(size.to_string(), confidence)]),
            confidence,
        }),
    );
    a.insert("needs_conversation", Answer::Noul(followup));
    a
}

pub fn router_settings() -> Settings {
    Settings {
        router_enabled: true,
        ..Settings::default()
    }
}
