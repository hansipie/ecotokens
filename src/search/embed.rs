use crate::config::settings::EmbedProvider;

/// Obtient l'embedding d'un texte via le provider configuré.
pub fn embed_text(text: &str, provider: &EmbedProvider) -> Option<Vec<f32>> {
    match provider {
        EmbedProvider::None | EmbedProvider::Legacy => None,
        EmbedProvider::Candle { model } => embed_text_candle(text, model),
        EmbedProvider::Ollama { url, model } => embed_text_ollama(text, url, model),
    }
}

/// Candle-backed embedding with thread-local provider caching.
/// The provider (model weights) is initialised once per thread on first use.
/// A failed initialisation is remembered (per model_id) so the download is not
/// retried for every chunk — which would conflict with the indexing progress bar.
fn embed_text_candle(text: &str, model_id: &str) -> Option<Vec<f32>> {
    use crate::embed::candle::CandleProvider;
    use std::cell::RefCell;

    thread_local! {
        // Some((id, Some(p))) = ready · Some((id, None)) = init failed, do not retry
        static CACHE: RefCell<Option<(String, Option<CandleProvider>)>> = const { RefCell::new(None) };
    }

    CACHE.with(|cell| {
        let mut guard = cell.borrow_mut();
        let needs_init = guard
            .as_ref()
            .map(|(id, _)| id.as_str() != model_id)
            .unwrap_or(true);
        if needs_init {
            match CandleProvider::new(model_id) {
                Ok(p) => *guard = Some((model_id.to_string(), Some(p))),
                Err(e) => {
                    eprintln!("ecotokens: candle provider init failed: {e}");
                    *guard = Some((model_id.to_string(), None));
                }
            }
        }
        guard.as_ref()?.1.as_ref()?.embed(text).ok()
    })
}

fn embed_text_ollama(text: &str, url: &str, model: &str) -> Option<Vec<f32>> {
    use std::cell::RefCell;
    // Reuse one client per thread so indexing keeps HTTP connections alive
    // instead of paying DNS/TLS setup for every chunk.
    thread_local! {
        static CLIENT: RefCell<Option<reqwest::blocking::Client>> = const { RefCell::new(None) };
    }
    let client = CLIENT.with(|cell| {
        let mut guard = cell.borrow_mut();
        if guard.is_none() {
            *guard = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .ok();
        }
        guard.clone()
    })?;

    let base = url.trim_end_matches('/');
    // `/api/embed` with `truncate` cuts an input that exceeds the model's context
    // instead of failing; the legacy `/api/embeddings` answers HTTP 500 for it
    // ("exceeds the available context size"), which left large chunks unindexed.
    let payload = serde_json::json!({ "model": model, "input": text, "truncate": true });
    let mut response = client
        .post(format!("{base}/api/embed"))
        .json(&payload)
        .send()
        .ok()?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        // Ollama older than 0.1.26 has no `/api/embed`.
        let legacy = serde_json::json!({ "model": model, "prompt": text });
        response = client
            .post(format!("{base}/api/embeddings"))
            .json(&legacy)
            .send()
            .ok()?;
    }
    if !response.status().is_success() {
        eprintln!(
            "ecotokens: ollama embedding failed: HTTP {}",
            response.status()
        );
        return None;
    }

    let json: serde_json::Value = response.json().ok()?;
    parse_ollama_embedding(&json)
}

/// Extracts the vector from an Ollama embedding response and L2-normalises it.
///
/// Reads `/api/embed` (`{"embeddings": [[...]]}`) as well as the legacy
/// `/api/embeddings` (`{"embedding": [...]}`). Returns `None` for a missing,
/// empty or all-zero vector.
pub fn parse_ollama_embedding(json: &serde_json::Value) -> Option<Vec<f32>> {
    let value = json["embeddings"]
        .get(0)
        .filter(|v| v.is_array())
        .unwrap_or(&json["embedding"]);
    let raw: Vec<f32> = serde_json::from_value(value.clone()).ok()?;
    if raw.is_empty() {
        return None;
    }

    // L2-normalisation — Ollama ne retourne pas de vecteurs normalisés
    let norm: f32 = raw.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm == 0.0 {
        return None;
    }
    Some(raw.into_iter().map(|x| x / norm).collect())
}

/// Similarité cosinus entre deux vecteurs de même dimension.
/// Retourne 0.0 si les vecteurs sont vides ou de longueurs différentes.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Charge la table d'embeddings depuis le fichier JSON dans le répertoire d'index.
/// Retourne une map `"file_path:chunk_idx" → Vec<f32>`.
pub fn load_embeddings(index_dir: &std::path::Path) -> std::collections::HashMap<String, Vec<f32>> {
    let path = index_dir.join("embeddings.json");
    let Ok(data) = std::fs::read_to_string(&path) else {
        return std::collections::HashMap::new();
    };
    serde_json::from_str(&data).unwrap_or_default()
}

/// Sauvegarde la table d'embeddings dans le répertoire d'index.
#[allow(dead_code)]
pub fn save_embeddings(
    index_dir: &std::path::Path,
    embeddings: &std::collections::HashMap<String, Vec<f32>>,
) -> Result<(), String> {
    let path = index_dir.join("embeddings.json");
    let json = serde_json::to_string(embeddings).map_err(|e| e.to_string())?;
    // Atomic write (project policy) to avoid a corrupt embeddings.json under
    // concurrent writes or a crash.
    crate::config::atomic_write(&path, json).map_err(|e| e.to_string())
}
