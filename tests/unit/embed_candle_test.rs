use ecotokens::config::settings::{EmbedProvider, Settings};

#[cfg(test)]
mod tests {
    use super::*;

    // T012
    #[test]
    fn embed_provider_default_is_candle() {
        let settings = Settings::default();
        match settings.embed_provider {
            EmbedProvider::Candle { ref model } => {
                assert!(
                    model.contains("MiniLM"),
                    "expected MiniLM model, got {model}"
                );
            }
            other => panic!("expected EmbedProvider::Candle, got {other:?}"),
        }
    }

    // T060 — migration: config ollama legacy → désérialisée en EmbedProvider::Legacy
    #[test]
    fn ollama_legacy_deserializes_to_legacy_variant() {
        let json = r#"{
            "embed_provider": {
                "type": "ollama",
                "url": "http://localhost:11434",
                "model": "nomic-embed-text"
            }
        }"#;
        let settings: Settings = serde_json::from_str(json).expect("json parse failed");
        assert_eq!(settings.embed_provider, EmbedProvider::Legacy);
    }

    // T061 — migration: config lm_studio legacy → désérialisée en EmbedProvider::Legacy
    #[test]
    fn lmstudio_legacy_deserializes_to_legacy_variant() {
        let json = r#"{
            "embed_provider": {
                "type": "lm_studio",
                "url": "http://localhost:1234",
                "model": "nomic-embed-text-v1.5"
            }
        }"#;
        let settings: Settings = serde_json::from_str(json).expect("json parse failed");
        assert_eq!(settings.embed_provider, EmbedProvider::Legacy);
    }

    // T031 — nécessite le téléchargement du modèle (~90 MB) → ignore en CI
    #[test]
    #[ignore]
    fn candle_provider_embeds_text() {
        use ecotokens::embed::candle::CandleProvider;

        let provider = CandleProvider::new("sentence-transformers/all-MiniLM-L6-v2")
            .expect("CandleProvider::new failed");
        let vec = provider.embed("hello world").expect("embed failed");
        assert_eq!(vec.len(), 384, "expected 384-dim vector");

        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "vector should be L2-normalised, got norm={norm}"
        );
    }

    // T032 — fallback BM25 si le provider échoue
    #[test]
    fn fallback_bm25_when_provider_fails() {
        use ecotokens::search::embed::embed_text;

        // EmbedProvider::None retourne toujours None → BM25 fallback
        let result = embed_text("test query", &EmbedProvider::None);
        assert!(result.is_none(), "None provider should return None");
        // L'appelant (search_index) gère le fallback BM25 si None
    }
}
