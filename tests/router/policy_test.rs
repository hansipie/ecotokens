#[path = "common.rs"]
mod common;
use common::{answers_card, plain_card};

use ecotokens::config::Settings;
use ecotokens::jev::Answers;
use ecotokens::router::card::TaskCard;
use ecotokens::router::policy::{Approval, Match, Policy, Resolution, Review, Role};
use ecotokens::router::Size;

fn resolve_with(policy: &Policy, answers: &Answers) -> Match {
    let card = TaskCard::from_answers(answers).expect("size answered");
    match policy.resolve(&card, &Settings::default()) {
        Resolution::Matched { matched, .. } => matched,
        other => panic!("expected a match, got {other:?}"),
    }
}

fn resolve(answers: &Answers) -> Match {
    resolve_with(&Policy::default_policy(), answers)
}

fn card(size: &str, nature: &str, risk: &str, ambiguity: f64, verifiability: f64) -> Answers {
    answers_card(size, nature, risk, ambiguity, verifiability, 0.05)
}

#[test]
fn the_default_policy_has_the_documented_version_and_rules_in_order() {
    let policy = Policy::default_policy();
    assert_eq!(policy.version(), "2026.1");
    let names: Vec<_> = policy.rules().iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "sensitive-work",
            "design-options",
            "research-sources",
            "mechanical",
            "ui-browser",
            "bounded-bug",
            "spec-feature",
            "size-only"
        ]
    );
}

#[test]
fn sensitive_work_goes_to_at_least_large_with_review_and_approval() {
    let m = resolve(&card("everyday", "implementation", "sensitive", 0.1, 0.9));
    assert_eq!(m.rule, "sensitive-work");
    assert_eq!(m.tier, Size::Large);
    assert_eq!(m.review, Review::Required);
    assert_eq!(m.approval, Approval::Irreversible);

    let m = resolve(&card("tiny", "bug", "destructive", 0.1, 0.9));
    assert_eq!((m.rule.as_str(), m.tier), ("sensitive-work", Size::Large));

    // A sensitive nature counts as a sensitive risk.
    let m = resolve(&card("everyday", "sensitive", "reversible", 0.1, 0.9));
    assert_eq!(m.rule, "sensitive-work");

    // Near miss: a reversible implementation.
    let m = resolve(&card("everyday", "implementation", "reversible", 0.1, 0.9));
    assert_ne!(m.rule, "sensitive-work");
}

#[test]
fn sensitive_work_never_routes_below_the_size_tier() {
    let m = resolve(&card("hardest", "implementation", "sensitive", 0.1, 0.9));
    assert_eq!(m.tier, Size::Hardest);
    let m = resolve(&card("large", "implementation", "destructive", 0.1, 0.9));
    assert_eq!(m.tier, Size::Large);
    let m = resolve(&card("tiny", "implementation", "sensitive", 0.1, 0.9));
    assert_eq!(m.tier, Size::Large);
}

#[test]
fn an_ambiguous_design_goes_to_the_analyst() {
    let m = resolve(&card("large", "design", "reversible", 0.9, 0.2));
    assert_eq!(m.rule, "design-options");
    assert_eq!(m.role, Role::Analyst);
    assert_eq!(m.tier, Size::Large);
    assert_eq!(m.agent_name(), "router-analyst");
    assert!(!m.hints.is_empty());

    // Near miss: a clearly specified design is routed by size.
    let m = resolve(&card("large", "design", "reversible", 0.1, 0.9));
    assert_eq!(m.rule, "size-only");
    assert_eq!(m.role, Role::Producer);
}

#[test]
fn research_goes_to_the_analyst() {
    let m = resolve(&card("everyday", "research", "reversible", 0.1, 0.5));
    assert_eq!(m.rule, "research-sources");
    assert_eq!(m.agent_name(), "router-analyst");
    let m = resolve(&card("everyday", "implementation", "reversible", 0.1, 0.5));
    assert_ne!(m.rule, "research-sources");
}

#[test]
fn a_verifiable_mechanical_job_goes_to_the_smallest_tier() {
    for size in ["tiny", "everyday"] {
        let m = resolve(&card(size, "mechanical", "reversible", 0.1, 0.9));
        assert_eq!(m.rule, "mechanical", "{size}");
        assert_eq!(m.tier, Size::Tiny, "{size}");
        assert!(!m.hints.is_empty());
    }
    // Near miss: not verifiable.
    let m = resolve(&card("everyday", "mechanical", "reversible", 0.1, 0.1));
    assert_eq!((m.rule.as_str(), m.tier), ("size-only", Size::Everyday));
}

#[test]
fn a_large_mechanical_job_falls_through_to_a_later_rule() {
    for size in ["large", "hardest"] {
        let m = resolve(&card(size, "mechanical", "reversible", 0.1, 0.9));
        assert_ne!(m.rule, "mechanical", "{size}");
        assert_eq!(m.rule, "size-only", "{size}");
        assert_eq!(m.tier, Size::parse(size).unwrap());
    }
}

#[test]
fn ui_work_is_routed_by_size_with_a_browser_hint() {
    let m = resolve(&card("everyday", "ui", "reversible", 0.1, 0.9));
    assert_eq!((m.rule.as_str(), m.tier), ("ui-browser", Size::Everyday));
    assert!(m.hints.iter().any(|h| h.to_lowercase().contains("browser")));
    let m = resolve(&card("everyday", "bug", "reversible", 0.1, 0.9));
    assert_ne!(m.rule, "ui-browser");
}

#[test]
fn a_bug_gets_at_least_the_everyday_tier() {
    let m = resolve(&card("tiny", "bug", "reversible", 0.1, 0.9));
    assert_eq!((m.rule.as_str(), m.tier), ("bounded-bug", Size::Everyday));
    let m = resolve(&card("hardest", "bug", "reversible", 0.1, 0.9));
    assert_eq!((m.rule.as_str(), m.tier), ("bounded-bug", Size::Hardest));
    assert!(m
        .hints
        .iter()
        .any(|h| h.to_lowercase().contains("reproduce")));
    let m = resolve(&card("tiny", "implementation", "reversible", 0.1, 0.9));
    assert_ne!(m.rule, "bounded-bug");
}

#[test]
fn a_clear_feature_is_routed_by_size() {
    let m = resolve(&card("large", "implementation", "reversible", 0.1, 0.9));
    assert_eq!((m.rule.as_str(), m.tier), ("spec-feature", Size::Large));
    // Near miss: an ambiguous feature falls to the last rule, same tier.
    let m = resolve(&card("large", "implementation", "reversible", 0.9, 0.9));
    assert_eq!((m.rule.as_str(), m.tier), ("size-only", Size::Large));
}

#[test]
fn the_last_rule_matches_everything_and_routes_by_size() {
    for size in Size::ALL {
        let m = resolve(&card(
            size.as_str(),
            "implementation",
            "reversible",
            0.9,
            0.1,
        ));
        assert_eq!(m.rule, "size-only");
        assert_eq!(m.tier, size);
        assert_eq!(m.agent_name(), size.agent_name());
        assert_eq!(m.review, Review::None);
        assert_eq!(m.approval, Approval::None);
    }
}

#[test]
fn first_match_wins_and_names_its_rule() {
    // Sensitive and research at once: the earlier rule decides.
    let m = resolve(&card("everyday", "research", "sensitive", 0.1, 0.5));
    assert_eq!(m.rule, "sensitive-work");
    // Design and sensitive nature.
    let m = resolve(&card("everyday", "sensitive", "reversible", 0.9, 0.5));
    assert_eq!(m.rule, "sensitive-work");
}

#[test]
fn plain_cards_route_like_the_size_only_router() {
    let mut total = 0;
    let mut same = 0;
    for size in Size::ALL {
        for nature in ["implementation", "ui"] {
            for ambiguity in [0.1, 0.9] {
                for verifiability in [0.1, 0.9] {
                    let m = resolve(&card(
                        size.as_str(),
                        nature,
                        "reversible",
                        ambiguity,
                        verifiability,
                    ));
                    total += 1;
                    if m.tier == size && m.role == Role::Producer {
                        same += 1;
                    }
                }
            }
        }
    }
    assert!(same * 100 >= total * 95, "{same}/{total}");

    // The plain helper card from the shared test code is one of them.
    let m = resolve(&plain_card("everyday"));
    assert_eq!(m.tier, Size::Everyday);
}

#[test]
fn the_number_of_files_is_not_an_input_of_the_rules() {
    // A rule cannot even name it (FR-006): the key is unknown, so the rule is invalid.
    let policy = Policy::from_user_toml(
        "[[rule]]\nname = \"many-files\"\nroute = \"large\"\n[rule.when]\nfiles = \"many\"\n",
    );
    assert!(policy
        .invalid()
        .iter()
        .any(|i| i.name.as_deref() == Some("many-files")));
}

#[test]
fn ambiguity_and_verifiability_use_the_settings_thresholds() {
    let policy = Policy::default_policy();
    let c = TaskCard::from_answers(&card("large", "design", "reversible", 0.4, 0.5)).unwrap();
    let settings = Settings {
        router_ambiguity_min_prob: 0.3,
        ..Settings::default()
    };
    let Resolution::Matched { matched, .. } = policy.resolve(&c, &settings) else {
        panic!();
    };
    assert_eq!(
        matched.rule, "design-options",
        "0.4 is high above a 0.3 threshold"
    );
    let Resolution::Matched { matched, .. } = policy.resolve(&c, &Settings::default()) else {
        panic!();
    };
    assert_eq!(
        matched.rule, "size-only",
        "0.4 is low under the default 0.5"
    );
}

// ── User file (user story 3) ─────────────────────────────────────────────

fn user(toml: &str) -> Policy {
    Policy::from_user_toml(toml)
}

fn invalid_names(p: &Policy) -> Vec<String> {
    p.invalid().iter().filter_map(|i| i.name.clone()).collect()
}

#[test]
fn user_rules_are_evaluated_before_the_defaults() {
    let policy = user(
        r#"
[[rule]]
name = "payments-need-review"
route = "large"
review = "required"
hints = ["Check idempotency of every payment call"]
[rule.when]
nature = ["implementation", "bug"]
risk = ["sensitive"]
"#,
    );
    let names: Vec<_> = policy.rules().iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names[0], "payments-need-review");
    assert_eq!(names.len(), 9, "one user rule plus the eight defaults");

    let m = resolve_with(&policy, &card("everyday", "bug", "sensitive", 0.1, 0.9));
    assert_eq!(m.rule, "payments-need-review");
    assert_eq!(m.source, ecotokens::router::policy::Source::User);
    assert_eq!(m.tier, Size::Large);
    assert_eq!(m.review, Review::Required);
    assert!(m.hints[0].contains("idempotency"));

    // Cards the user rule does not cover still follow the defaults.
    let m = resolve_with(&policy, &card("everyday", "bug", "reversible", 0.1, 0.9));
    assert_eq!(m.rule, "bounded-bug");
    assert_eq!(m.source, ecotokens::router::policy::Source::Default);
}

#[test]
fn a_user_rule_with_the_same_condition_as_a_default_wins_and_is_named() {
    let policy =
        user("[[rule]]\nname = \"my-ui\"\nroute = \"hardest\"\n[rule.when]\nnature = [\"ui\"]\n");
    let m = resolve_with(&policy, &card("tiny", "ui", "reversible", 0.1, 0.9));
    assert_eq!(m.rule, "my-ui");
    assert_eq!(m.tier, Size::Hardest);
}

#[test]
fn the_policy_version_changes_with_the_file_and_has_a_golden_value() {
    let a = "[[rule]]\nname = \"pay\"\nroute = \"large\"\n";
    let b = "[[rule]]\nname = \"pay\"\nroute = \"hardest\"\n";
    assert_eq!(
        user(a).version(),
        "2026.1+u1945e624",
        "FNV-1a 64 of the file bytes"
    );
    assert_ne!(user(a).version(), user(b).version());
    assert_eq!(user(a).version(), user(a).version(), "stable");
    assert!(user(a).has_user_file());
    assert!(!Policy::default_policy().has_user_file());
    assert_eq!(Policy::default_policy().version(), "2026.1");
}

#[test]
fn an_invalid_rule_is_dropped_while_valid_ones_still_work() {
    let policy = user(
        r#"
[[rule]]
name = "good"
route = "hardest"
[rule.when]
nature = ["research"]

[[rule]]
name = "bad-key"
route = "large"
colour = "red"

[[rule]]
name = "bad-value"
route = "gigantic"

[[rule]]
name = "good"
route = "tiny"

[[rule]]
name = "analyst-reviewed"
route = "analyst"
review = "suggested"

[[rule]]
name = "review-without-risk"
route = "large"
review = "required"
[rule.when]
nature = ["bug"]

[[rule]]
name = "analyst-at-least"
route = "analyst"
bound = "at-least"

[[rule]]
name = "Bad Name"
route = "large"
"#,
    );
    let valid: Vec<_> = policy
        .rules()
        .iter()
        .take(1)
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(valid, ["good"]);
    assert_eq!(
        invalid_names(&policy),
        [
            "bad-key",
            "bad-value",
            "good",
            "analyst-reviewed",
            "review-without-risk",
            "analyst-at-least",
            "Bad Name"
        ]
    );
    for i in policy.invalid() {
        assert!(!i.reason.is_empty());
    }
    // The valid rule works.
    let m = resolve_with(&policy, &card("tiny", "research", "reversible", 0.1, 0.5));
    assert_eq!((m.rule.as_str(), m.tier), ("good", Size::Hardest));
    assert!(policy.all_warnings().len() >= 7);
}

#[test]
fn a_file_that_is_not_toml_is_ignored_with_one_warning() {
    let policy = user("this is = = not toml [[");
    assert_eq!(policy.version(), "2026.1", "defaults, no user version");
    assert_eq!(policy.rules(), Policy::default_policy().rules());
    assert_eq!(policy.warnings().len(), 1);
    assert!(!policy.has_user_file());
}

#[test]
fn no_user_file_gives_exactly_the_defaults() {
    let dir = tempfile::TempDir::new().unwrap();
    let policy = Policy::load_from(&dir.path().join("router_rules.toml"));
    assert_eq!(policy, Policy::default_policy());
    // An empty file is a valid user file with no rules: the defaults still decide.
    let empty = dir.path().join("empty.toml");
    std::fs::write(&empty, "").unwrap();
    let policy = Policy::load_from(&empty);
    assert_eq!(policy.rules(), Policy::default_policy().rules());
    assert!(policy.invalid().is_empty());
}

#[test]
fn rule_limits_are_enforced() {
    // More than three hints.
    let p =
        user("[[rule]]\nname = \"h\"\nroute = \"large\"\nhints = [\"a\", \"b\", \"c\", \"d\"]\n");
    assert_eq!(invalid_names(&p), ["h"]);
    // A hint over 160 characters.
    let long = "x".repeat(161);
    let p = user(&format!(
        "[[rule]]\nname = \"h\"\nroute = \"large\"\nhints = [\"{long}\"]\n"
    ));
    assert_eq!(invalid_names(&p), ["h"]);
    let ok = "x".repeat(160);
    let p = user(&format!(
        "[[rule]]\nname = \"h\"\nroute = \"large\"\nhints = [\"{ok}\"]\n"
    ));
    assert!(p.invalid().is_empty());
    // More than 100 rules.
    let mut many = String::new();
    for i in 0..101 {
        many.push_str(&format!("[[rule]]\nname = \"r{i}\"\nroute = \"large\"\n"));
    }
    let p = user(&many);
    assert_eq!(p.invalid().len(), 1, "only the 101st is dropped");
    assert_eq!(p.rules().len(), 100 + 8);
}

#[test]
fn bound_at_least_works_with_a_tier_and_not_with_the_analyst() {
    let p = user(
        "[[rule]]\nname = \"floor\"\nroute = \"large\"\nbound = \"at-least\"\n[rule.when]\nnature = [\"ui\"]\n",
    );
    let m = resolve_with(&p, &card("tiny", "ui", "reversible", 0.1, 0.9));
    assert_eq!(m.tier, Size::Large, "raised to the floor");
    let m = resolve_with(&p, &card("hardest", "ui", "reversible", 0.1, 0.9));
    assert_eq!(m.tier, Size::Hardest, "never lowered");
    let p = user("[[rule]]\nname = \"x\"\nroute = \"analyst\"\nbound = \"at-least\"\n");
    assert_eq!(invalid_names(&p), ["x"]);
}

#[test]
fn an_empty_when_matches_every_card() {
    let p = user("[[rule]]\nname = \"all\"\nroute = \"hardest\"\n");
    let m = resolve_with(&p, &card("tiny", "ui", "reversible", 0.1, 0.9));
    assert_eq!(m.rule, "all");
}
