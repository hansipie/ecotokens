use ecotokens::config::Settings;
use tempfile::TempDir;

fn load(json: &str) -> Settings {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.json");
    std::fs::write(&config, json).unwrap();
    Settings::load_from_paths_pub(&config, &dir.path().join("abbreviations.json"))
}

#[test]
fn new_router_fields_have_their_documented_defaults() {
    let s = Settings::default();
    assert!(s.router_private.is_empty());
    assert!(s.router_allowed.is_empty());
    assert!(!s.router_strict);
    assert_eq!(s.router_max_attempts, 2);
    assert_eq!(s.router_card_min_confidence, None);
    assert!((s.router_ambiguity_min_prob - 0.5).abs() < 1e-9);
    assert!((s.router_verifiable_min_prob - 0.5).abs() < 1e-9);
}

#[test]
fn a_settings_file_from_the_previous_release_still_loads() {
    let s =
        load(r#"{"router_enabled": true, "router_min_confidence": 0.7, "router_timeout_ms": 900}"#);
    assert!(s.router_enabled);
    assert!(s.router_private.is_empty());
    assert!(!s.router_strict);
    assert_eq!(s.router_max_attempts, 2);
    assert!((s.router_ambiguity_min_prob - 0.5).abs() < 1e-9);
}

#[test]
fn the_new_fields_are_read_from_the_file() {
    let s = load(
        r#"{"router_private": ["/a", "/b/*"], "router_allowed": ["/c"], "router_strict": true,
            "router_max_attempts": 3, "router_card_min_confidence": 0.8,
            "router_ambiguity_min_prob": 0.6, "router_verifiable_min_prob": 0.4}"#,
    );
    assert_eq!(s.router_private, ["/a", "/b/*"]);
    assert_eq!(s.router_allowed, ["/c"]);
    assert!(s.router_strict);
    assert_eq!(s.router_max_attempts, 3);
    assert_eq!(s.router_card_min_confidence, Some(0.8));
    assert!((s.router_ambiguity_min_prob - 0.6).abs() < 1e-9);
    assert!((s.router_verifiable_min_prob - 0.4).abs() < 1e-9);
}

#[test]
fn card_confidence_falls_back_to_the_size_confidence_threshold() {
    let s = load(r#"{"router_min_confidence": 0.75}"#);
    assert!((s.card_min_confidence() - 0.75).abs() < 1e-9);
    let s = load(r#"{"router_min_confidence": 0.75, "router_card_min_confidence": 0.5}"#);
    assert!((s.card_min_confidence() - 0.5).abs() < 1e-9);
}

#[test]
fn unreadable_settings_are_not_trusted_but_a_missing_file_is() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.json");
    assert!(Settings::config_file_trusted(&config), "missing = defaults");
    std::fs::write(&config, "{ not json").unwrap();
    assert!(!Settings::config_file_trusted(&config));
    std::fs::write(&config, r#"{"router_strict": true}"#).unwrap();
    assert!(Settings::config_file_trusted(&config));
}
