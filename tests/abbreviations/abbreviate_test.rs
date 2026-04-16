use ecotokens::abbreviations::{abbreviate, build_model_instructions, dictionary};
use ecotokens::config::Settings;
use std::collections::HashMap;

fn enabled_settings() -> Settings {
    let mut s = Settings::default();
    s.abbreviations_enabled = true;
    s
}

#[test]
fn disabled_is_identity() {
    let s = Settings::default();
    let input = "the configuration and the function are ready";
    let (out, count) = abbreviate(input, &s);
    assert_eq!(out, input);
    assert_eq!(count, 0);
}

#[test]
fn enabled_replaces_known_words() {
    let s = enabled_settings();
    let (out, count) = abbreviate("the configuration uses a function with parameter", &s);
    assert_eq!(out, "the config uses a fn with param");
    assert_eq!(count, 3);
}

#[test]
fn word_boundary_is_respected() {
    let s = enabled_settings();
    let (out, _) = abbreviate("refunction preconfiguration unfunction", &s);
    assert_eq!(out, "refunction preconfiguration unfunction");
}

#[test]
fn preserves_capitalization_capitalized() {
    let s = enabled_settings();
    let (out, _) = abbreviate("Function and Configuration", &s);
    assert_eq!(out, "Fn and Config");
}

#[test]
fn preserves_capitalization_all_caps() {
    let s = enabled_settings();
    let (out, _) = abbreviate("FUNCTION call in CONFIGURATION", &s);
    assert_eq!(out, "FN call in CONFIG");
}

#[test]
fn code_blocks_are_preserved() {
    let s = enabled_settings();
    let input =
        "see the configuration:\n```\nfn function() { /* keep me */ }\n```\nand error handling";
    let (out, _) = abbreviate(input, &s);
    assert!(out.contains("see the config:"));
    assert!(out.contains("and err handling"));
    assert!(out.contains("fn function() { /* keep me */ }"));
}

#[test]
fn custom_override_wins_over_default() {
    let mut s = enabled_settings();
    s.abbreviations_custom
        .insert("function".into(), "func".into());
    let (out, _) = abbreviate("the function is ready", &s);
    assert_eq!(out, "the func is ready");
}

#[test]
fn custom_extends_dictionary() {
    let mut s = enabled_settings();
    s.abbreviations_custom
        .insert("whatever".into(), "wh".into());
    let (out, count) = abbreviate("whatever the case", &s);
    assert_eq!(out, "wh the case");
    assert_eq!(count, 1);
}

#[test]
fn default_pairs_are_non_empty() {
    assert!(!dictionary::DEFAULT_PAIRS.is_empty());
    for (k, v) in dictionary::DEFAULT_PAIRS {
        assert!(!k.is_empty());
        assert!(!v.is_empty());
        assert!(v.len() < k.len(), "{k} → {v} should be shorter");
    }
}

#[test]
fn merged_pairs_custom_wins() {
    let mut custom = HashMap::new();
    custom.insert("error".into(), "E".into());
    let merged = dictionary::merged_pairs(&custom);
    let mapping: HashMap<_, _> = merged.into_iter().collect();
    assert_eq!(mapping.get("error").map(String::as_str), Some("E"));
}

#[test]
fn build_model_instructions_lists_entries() {
    let s = enabled_settings();
    let text = build_model_instructions(&s);
    assert!(text.contains("function"));
    assert!(text.contains("→"));
    assert!(text.contains("config"));
}

#[test]
fn abbreviations_disabled_in_default_settings() {
    let s = Settings::default();
    assert!(!s.abbreviations_enabled);
    assert!(s.abbreviations_custom.is_empty());
}

#[test]
fn settings_roundtrip_keeps_abbreviations_fields() {
    let mut s = Settings::default();
    s.abbreviations_enabled = true;
    s.abbreviations_custom.insert("foobar".into(), "fb".into());
    let json = serde_json::to_string(&s).unwrap();
    let s2: Settings = serde_json::from_str(&json).unwrap();
    assert!(s2.abbreviations_enabled);
    assert_eq!(s2.abbreviations_custom.get("foobar").unwrap(), "fb");
}

#[test]
fn missing_fields_in_json_default_to_off() {
    let json = r#"{}"#;
    let s: Settings = serde_json::from_str(json).unwrap();
    assert!(!s.abbreviations_enabled);
    assert!(s.abbreviations_custom.is_empty());
}
