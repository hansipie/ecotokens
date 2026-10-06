use ecotokens::config::settings::EmbedProvider;
use ecotokens::config::Settings;

#[test]
fn default_values_when_no_config_file() {
    let s = Settings::default();
    assert_eq!(s.summary_threshold_lines, 500);
    assert_eq!(s.summary_threshold_bytes, 51200);
    assert!(s.masking_enabled);
    assert!(!s.exact_token_counting);
    assert!(!s.debug);
    assert_eq!(s.price_input_usd_per_mtok, None);
    assert_eq!(s.price_output_usd_per_mtok, None);
    assert!(s.exclusions.is_empty());
}

#[test]
fn valid_config_round_trips() {
    let s = Settings {
        exclusions: vec!["grep".to_string()],
        debug: true,
        summary_threshold_lines: 200,
        ..Default::default()
    };

    let json = serde_json::to_string(&s).unwrap();
    let s2: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(s2.exclusions, vec!["grep"]);
    assert!(s2.debug);
    assert_eq!(s2.summary_threshold_lines, 200);
}

#[test]
fn rejects_threshold_lines_below_10() {
    let s = Settings {
        summary_threshold_lines: 5,
        ..Default::default()
    };
    assert!(s.validate().is_err());
}

#[test]
fn rejects_threshold_lines_above_10000() {
    let s = Settings {
        summary_threshold_lines: 20000,
        ..Default::default()
    };
    assert!(s.validate().is_err());
}

#[test]
fn rejects_threshold_bytes_below_1024() {
    let s = Settings {
        summary_threshold_bytes: 512,
        ..Default::default()
    };
    assert!(s.validate().is_err());
}

#[test]
fn valid_settings_pass_validation() {
    let s = Settings::default();
    assert!(s.validate().is_ok());
}

#[test]
fn deserialization_with_missing_fields_uses_defaults() {
    let json = r#"{"exclusions": ["ls"]}"#;
    let s: Settings = serde_json::from_str(json).unwrap();
    assert_eq!(s.exclusions, vec!["ls"]);
    assert_eq!(s.summary_threshold_lines, 500);
    assert!(s.masking_enabled);
}

// ── User-entered pricing ───────────────────────────────────────────────────────

#[test]
fn price_round_trips_through_json() {
    let s = Settings {
        price_input_usd_per_mtok: Some(3.0),
        price_output_usd_per_mtok: Some(15.0),
        ..Default::default()
    };
    let json = serde_json::to_string(&s).unwrap();
    let s2: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(s2.price_input_usd_per_mtok, Some(3.0));
    assert_eq!(s2.price_output_usd_per_mtok, Some(15.0));
}

#[test]
fn legacy_model_keys_and_pricing_json_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let abbrev_path = dir.path().join("abbreviations.json");
    std::fs::write(
        &config_path,
        r#"{"default_model": "claude-opus-5", "model_pricing": {"x": {"input_usd_per_1m": 1.0, "output_usd_per_1m": 2.0}}}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("pricing.json"), "{}").unwrap();

    let s = Settings::load_from_paths_pub(&config_path, &abbrev_path);
    assert_eq!(s.price_input_usd_per_mtok, None);
    assert_eq!(s.price_output_usd_per_mtok, None);
}

#[test]
fn validate_price_rejects_negative_and_non_finite() {
    use ecotokens::config::validate_price;
    assert!(validate_price("--input", None).is_ok());
    assert!(validate_price("--input", Some(0.0)).is_ok());
    assert!(validate_price("--input", Some(3.5)).is_ok());
    assert!(validate_price("--input", Some(-1.0)).is_err());
    assert!(validate_price("--output", Some(f64::NAN)).is_err());
    assert!(validate_price("--output", Some(f64::INFINITY)).is_err());
}

// ── T072t — Tests embed_provider (CLI --embed-provider) ───────────────────────

#[test]
fn embed_provider_candle_by_default() {
    let s = Settings::default();
    assert_eq!(
        s.embed_provider,
        EmbedProvider::Candle {
            model: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
        }
    );
}

#[test]
fn embed_provider_none_roundtrip() {
    let s = Settings {
        embed_provider: EmbedProvider::None,
        ..Default::default()
    };
    let json = serde_json::to_string(&s).unwrap();
    let s2: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(s2.embed_provider, EmbedProvider::None);
}

#[test]
fn embed_provider_ollama_deserializes_to_ollama() {
    let json = r#"{"embed_provider": {"type": "ollama", "url": "http://localhost:11434", "model": "nomic-embed-text"}}"#;
    let s: Settings = serde_json::from_str(json).unwrap();
    assert_eq!(
        s.embed_provider,
        EmbedProvider::Ollama {
            url: "http://localhost:11434".to_string(),
            model: "nomic-embed-text".to_string(),
        }
    );
}

#[test]
fn embed_provider_ollama_roundtrip() {
    let s = Settings {
        embed_provider: EmbedProvider::Ollama {
            url: "http://localhost:11434".to_string(),
            model: "qwen3-embedding:latest".to_string(),
        },
        ..Default::default()
    };
    let json = serde_json::to_string(&s).unwrap();
    let s2: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(
        s2.embed_provider,
        EmbedProvider::Ollama {
            url: "http://localhost:11434".to_string(),
            model: "qwen3-embedding:latest".to_string(),
        }
    );
}

#[test]
fn embed_provider_legacy_lmstudio_deserializes_to_legacy() {
    let json = r#"{"embed_provider": {"type": "lm_studio", "url": "http://localhost:1234", "model": "nomic-embed-text-v1.5"}}"#;
    let s: Settings = serde_json::from_str(json).unwrap();
    assert_eq!(s.embed_provider, EmbedProvider::Legacy);
}

#[test]
fn embed_provider_missing_in_json_defaults_to_candle() {
    let json = r#"{"exclusions": []}"#;
    let s: Settings = serde_json::from_str(json).unwrap();
    assert_eq!(
        s.embed_provider,
        EmbedProvider::Candle {
            model: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
        }
    );
}

// ── Session handoff settings ────────────────────────────────────────────────

#[test]
fn handoff_defaults() {
    let s = Settings::default();
    assert!(!s.handoff_enabled);
    assert_eq!(s.handoff_max_chars, 4000);
    assert_eq!(s.handoff_stale_hours, 24);
    assert_eq!(s.handoff_retention_days, 30);
    assert_eq!(s.handoff_consumed_retention_hours, 48);
    assert!(!s.handoff_inject_startup);
}

#[test]
fn old_config_without_handoff_keys_still_loads() {
    let s: Settings = serde_json::from_str(r#"{"debug": true}"#).unwrap();
    assert!(s.debug);
    assert!(!s.handoff_enabled);
    assert_eq!(s.handoff_max_chars, 4000);
    assert_eq!(s.handoff_stale_hours, 24);
    assert_eq!(s.handoff_retention_days, 30);
    assert_eq!(s.handoff_consumed_retention_hours, 48);
}

#[test]
fn handoff_max_chars_is_capped_below_the_claude_code_injection_limit() {
    let mut s = Settings::default();
    assert_eq!(s.effective_handoff_max_chars(), 4000);
    s.handoff_max_chars = 9000;
    assert_eq!(s.effective_handoff_max_chars(), 9000);
    s.handoff_max_chars = 50_000;
    assert_eq!(s.effective_handoff_max_chars(), 9000);
}

#[test]
fn handoff_settings_round_trip() {
    let s = Settings {
        handoff_enabled: true,
        handoff_max_chars: 2500,
        handoff_stale_hours: 6,
        handoff_retention_days: 10,
        handoff_consumed_retention_hours: 12,
        handoff_inject_startup: true,
        ..Default::default()
    };
    let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert!(back.handoff_enabled && back.handoff_inject_startup);
    assert_eq!(back.handoff_max_chars, 2500);
    assert_eq!(back.handoff_stale_hours, 6);
    assert_eq!(back.handoff_retention_days, 10);
    assert_eq!(back.handoff_consumed_retention_hours, 12);
}
