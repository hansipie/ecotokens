use ecotokens::config::settings::EmbedProvider;
use ecotokens::config::Settings;
use std::collections::HashMap;

#[test]
fn default_values_when_no_config_file() {
    let s = Settings::default();
    assert_eq!(s.summary_threshold_lines, 500);
    assert_eq!(s.summary_threshold_bytes, 51200);
    assert!(s.masking_enabled);
    assert!(!s.exact_token_counting);
    assert!(!s.debug);
    assert_eq!(s.default_model, "claude-sonnet-4-6");
    assert!(s.exclusions.is_empty());
}

#[test]
fn valid_config_round_trips() {
    let mut s = Settings::default();
    s.exclusions = vec!["grep".to_string()];
    s.debug = true;
    s.summary_threshold_lines = 200;

    let json = serde_json::to_string(&s).unwrap();
    let s2: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(s2.exclusions, vec!["grep"]);
    assert!(s2.debug);
    assert_eq!(s2.summary_threshold_lines, 200);
}

#[test]
fn rejects_threshold_lines_below_10() {
    let mut s = Settings::default();
    s.summary_threshold_lines = 5;
    assert!(s.validate().is_err());
}

#[test]
fn rejects_threshold_lines_above_10000() {
    let mut s = Settings::default();
    s.summary_threshold_lines = 20000;
    assert!(s.validate().is_err());
}

#[test]
fn rejects_threshold_bytes_below_1024() {
    let mut s = Settings::default();
    s.summary_threshold_bytes = 512;
    assert!(s.validate().is_err());
}

#[test]
fn valid_settings_pass_validation() {
    let s = Settings::default();
    assert!(s.validate().is_ok());
}

#[test]
fn model_pricing_has_known_models() {
    let s = Settings::default();
    assert!(s.model_pricing.contains_key("claude-sonnet-4-6"));
    assert!(s.model_pricing.contains_key("claude-opus-4-6"));
}

#[test]
fn deserialization_with_missing_fields_uses_defaults() {
    let json = r#"{"exclusions": ["ls"]}"#;
    let s: Settings = serde_json::from_str(json).unwrap();
    assert_eq!(s.exclusions, vec!["ls"]);
    assert_eq!(s.summary_threshold_lines, 500);
    assert!(s.masking_enabled);
}

// ── Pricing resource file ──────────────────────────────────────────────────────

#[test]
fn model_pricing_not_in_serialized_json() {
    let s = Settings::default();
    let json = serde_json::to_string(&s).unwrap();
    assert!(
        !json.contains("model_pricing"),
        "model_pricing ne doit pas apparaître dans config.json"
    );
}

#[test]
fn pricing_json_overrides_builtin() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let abbrev_path = dir.path().join("abbreviations.json");
    let pricing_path = dir.path().join("pricing.json");

    std::fs::write(&config_path, "{}").unwrap();
    let custom: HashMap<&str, serde_json::Value> = HashMap::from([(
        "claude-sonnet-4-6",
        serde_json::json!({"input_usd_per_1m": 0.01, "output_usd_per_1m": 0.02}),
    )]);
    std::fs::write(&pricing_path, serde_json::to_string(&custom).unwrap()).unwrap();

    let s = Settings::load_from_paths_pub(&config_path, &abbrev_path, &pricing_path);
    let price = s.model_pricing.get("claude-sonnet-4-6").unwrap();
    assert!((price.input_usd_per_1m - 0.01).abs() < f64::EPSILON);
    assert!((price.output_usd_per_1m - 0.02).abs() < f64::EPSILON);
    // Les modèles non overridés restent présents depuis le built-in
    assert!(s.model_pricing.contains_key("claude-opus-4-6"));
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
    let mut s = Settings::default();
    s.embed_provider = EmbedProvider::None;
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
    let mut s = Settings::default();
    s.embed_provider = EmbedProvider::Ollama {
        url: "http://localhost:11434".to_string(),
        model: "qwen3-embedding:latest".to_string(),
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
