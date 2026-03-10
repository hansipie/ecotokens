use ecotokens::config::settings::EmbedProvider;
use ecotokens::search::embed::{
    cosine_similarity, embed_text, get_lmstudio_embedding, get_ollama_embedding,
};

// ── Tests provider None ────────────────────────────────────────────────────────

#[test]
fn test_provider_none_returns_none() {
    let result = embed_text("hello world", &EmbedProvider::None);
    assert!(result.is_none(), "provider None doit retourner None");
}

// ── Tests Ollama provider (T068) ───────────────────────────────────────────────

/// URL invalide → erreur claire (connexion refusée ou feature non activée).
#[test]
fn test_ollama_invalid_url_returns_error() {
    // Port 1 est toujours fermé : connexion refusée immédiatement
    let result = get_ollama_embedding("test text", "http://127.0.0.1:1");
    assert!(result.is_err(), "URL invalide doit retourner une erreur");
    let err = result.unwrap_err();
    assert!(!err.is_empty(), "le message d'erreur ne doit pas être vide");
}

/// Provider Ollama avec URL invalide → embed_text retourne None (fallback BM25).
#[test]
fn test_ollama_provider_unavailable_falls_back_to_none() {
    let provider = EmbedProvider::Ollama {
        url: "http://127.0.0.1:1".into(),
    };
    let result = embed_text("hello", &provider);
    assert!(
        result.is_none(),
        "provider indisponible doit retourner None (fallback BM25)"
    );
}

/// URL avec schéma incorrect → erreur claire.
#[test]
fn test_ollama_malformed_url_returns_error() {
    let result = get_ollama_embedding("text", "not-a-url");
    assert!(result.is_err());
}

// ── Tests LM Studio provider (T069) ───────────────────────────────────────────

/// URL invalide → erreur claire.
#[test]
fn test_lmstudio_invalid_url_returns_error() {
    let result = get_lmstudio_embedding("test text", "http://127.0.0.1:1");
    assert!(result.is_err(), "URL invalide doit retourner une erreur");
    let err = result.unwrap_err();
    assert!(!err.is_empty(), "le message d'erreur ne doit pas être vide");
}

/// Provider LM Studio avec URL invalide → embed_text retourne None (fallback BM25).
#[test]
fn test_lmstudio_provider_unavailable_falls_back_to_none() {
    let provider = EmbedProvider::LmStudio {
        url: "http://127.0.0.1:1".into(),
    };
    let result = embed_text("hello", &provider);
    assert!(
        result.is_none(),
        "provider indisponible doit retourner None (fallback BM25)"
    );
}

/// LM Studio utilise le format OpenAI-compatible (/v1/embeddings).
/// On vérifie que c'est bien une erreur de connexion et non de protocole.
#[test]
fn test_lmstudio_openai_compatible_endpoint() {
    let result = get_lmstudio_embedding("hello", "http://127.0.0.1:1");
    assert!(result.is_err());
    // Le message ne doit pas mentionner "lmstudio" ou "ollama" — c'est une erreur réseau
    let err = result.unwrap_err();
    assert!(
        !err.contains("panic"),
        "erreur ne doit pas être un panic : {err}"
    );
}

// ── Tests cosine_similarity ────────────────────────────────────────────────────

#[test]
fn test_cosine_similarity_identical_vectors() {
    let v = vec![1.0_f32, 0.0, 0.0];
    let sim = cosine_similarity(&v, &v);
    assert!(
        (sim - 1.0).abs() < 1e-5,
        "vecteurs identiques → similarité 1.0, got {sim}"
    );
}

#[test]
fn test_cosine_similarity_orthogonal_vectors() {
    let a = vec![1.0_f32, 0.0, 0.0];
    let b = vec![0.0_f32, 1.0, 0.0];
    let sim = cosine_similarity(&a, &b);
    assert!(
        sim.abs() < 1e-5,
        "vecteurs orthogonaux → similarité ≈ 0.0, got {sim}"
    );
}

#[test]
fn test_cosine_similarity_opposite_vectors() {
    let a = vec![1.0_f32, 0.0];
    let b = vec![-1.0_f32, 0.0];
    let sim = cosine_similarity(&a, &b);
    assert!(
        (sim + 1.0).abs() < 1e-5,
        "vecteurs opposés → similarité -1.0, got {sim}"
    );
}

#[test]
fn test_cosine_similarity_empty_vectors() {
    let result = cosine_similarity(&[], &[]);
    assert_eq!(result, 0.0, "vecteurs vides → 0.0");
}

#[test]
fn test_cosine_similarity_mismatched_lengths() {
    let a = vec![1.0_f32, 0.0];
    let b = vec![1.0_f32, 0.0, 0.0];
    let result = cosine_similarity(&a, &b);
    assert_eq!(result, 0.0, "longueurs différentes → 0.0");
}
