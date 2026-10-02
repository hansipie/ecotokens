//! `JevContext::ask_for` logs one row per call, but only for judges that
//! opt in (the production one), so stub-driven tests never touch the disk.
//! Single test: it sets a process-wide env var.

use std::time::Duration;

use ecotokens::config::Settings;
use ecotokens::jev::stats::{summarize, Purpose, DB_ENV};
use ecotokens::jev::{Answers, JevContext, JevError, Judge, Questions, StubJudge, Usage};
use serde_json::json;

struct Recording {
    fail: bool,
}

impl Judge for Recording {
    fn ask(&self, _: serde_json::Value, _: Questions, _: Duration) -> Result<Answers, JevError> {
        if self.fail {
            Err(JevError::Timeout)
        } else {
            Ok(Answers::default())
        }
    }
    fn ask_with_usage(
        &self,
        state: serde_json::Value,
        questions: Questions,
        timeout: Duration,
    ) -> Result<(Answers, Option<Usage>), JevError> {
        self.ask(state, questions, timeout).map(|a| {
            (
                a,
                Some(Usage {
                    input_tokens: 7,
                    output_tokens: 3,
                }),
            )
        })
    }
    fn records_stats(&self) -> bool {
        true
    }
}

#[test]
fn ask_for_records_only_for_recording_judges() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("jev.db");
    std::env::set_var(DB_ENV, &db);
    let settings = Settings::default();
    let t = Duration::from_secs(1);

    let stub = StubJudge::new();
    let _ = JevContext::new(&stub, &settings).ask_for(
        Purpose::Classify,
        json!({}),
        Questions::new(),
        t,
    );
    assert!(!db.exists(), "stub judges must not write stats");

    let good = Recording { fail: false };
    let bad = Recording { fail: true };
    JevContext::new(&good, &settings)
        .ask_for(Purpose::Classify, json!({}), Questions::new(), t)
        .unwrap();
    assert!(JevContext::new(&bad, &settings)
        .ask_for(Purpose::Verify, json!({}), Questions::new(), t)
        .is_err());

    let s = summarize(&db, &settings, None).unwrap();
    assert_eq!((s.calls, s.ok, s.fallbacks), (2, 1, 1));
    assert_eq!(s.by_purpose["classify"].input_tokens, 7);
    assert_eq!(s.errors["timeout"], 1);
}
