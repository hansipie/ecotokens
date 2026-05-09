use crate::config::settings::EmbedProvider;

/// Obtient l'embedding d'un texte via le provider configuré.
pub fn embed_text(text: &str, provider: &EmbedProvider) -> Option<Vec<f32>> {
    match provider {
        EmbedProvider::None | EmbedProvider::Legacy => None,
        EmbedProvider::Candle { model } => embed_text_candle(text, model),
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
    std::fs::write(&path, json).map_err(|e| e.to_string())
}
