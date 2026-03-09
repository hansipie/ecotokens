use crate::config::settings::EmbedProvider;

/// Obtient l'embedding d'un texte via le provider configuré.
/// Retourne `None` si le provider est `None`, si le provider est inaccessible
/// (fallback BM25 automatique), ou si appelé depuis un contexte async tokio
/// (reqwest::blocking ne peut pas créer son runtime interne dans ce cas).
pub fn embed_text(text: &str, provider: &EmbedProvider) -> Option<Vec<f32>> {
    match provider {
        EmbedProvider::None => None,
        EmbedProvider::Ollama { url } => {
            if tokio::runtime::Handle::try_current().is_ok() {
                return None;
            }
            get_ollama_embedding(text, url).ok()
        }
        EmbedProvider::LmStudio { url } => {
            if tokio::runtime::Handle::try_current().is_ok() {
                return None;
            }
            get_lmstudio_embedding(text, url).ok()
        }
    }
}

/// Appel HTTP vers l'API Ollama pour obtenir l'embedding d'un texte.
///
/// Endpoint : `POST {base_url}/api/embeddings`
/// Modèle par défaut : `nomic-embed-text`
pub fn get_ollama_embedding(text: &str, base_url: &str) -> Result<Vec<f32>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("client error: {e}"))?;

    let url = format!("{}/api/embeddings", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": "nomic-embed-text",
        "prompt": text
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| format!("request error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}: model not available", resp.status()));
    }

    let data: serde_json::Value = resp.json().map_err(|e| format!("parse error: {e}"))?;

    let embedding = data["embedding"]
        .as_array()
        .ok_or_else(|| "missing 'embedding' field in response".to_string())?
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect();

    Ok(embedding)
}

/// Appel HTTP vers l'API LM Studio (format OpenAI-compatible) pour obtenir l'embedding.
///
/// Endpoint : `POST {base_url}/v1/embeddings`
/// Modèle par défaut : `nomic-embed-text-v1.5`
pub fn get_lmstudio_embedding(text: &str, base_url: &str) -> Result<Vec<f32>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("client error: {e}"))?;

    let url = format!("{}/v1/embeddings", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": "nomic-embed-text-v1.5",
        "input": text
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| format!("request error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}: model not available", resp.status()));
    }

    let data: serde_json::Value = resp.json().map_err(|e| format!("parse error: {e}"))?;

    let embedding = data["data"][0]["embedding"]
        .as_array()
        .ok_or_else(|| "missing 'data[0].embedding' field in response".to_string())?
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect();

    Ok(embedding)
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
pub fn load_embeddings(
    index_dir: &std::path::Path,
) -> std::collections::HashMap<String, Vec<f32>> {
    let path = index_dir.join("embeddings.json");
    let Ok(data) = std::fs::read_to_string(&path) else {
        return std::collections::HashMap::new();
    };
    serde_json::from_str(&data).unwrap_or_default()
}

/// Sauvegarde la table d'embeddings dans le répertoire d'index.
pub fn save_embeddings(
    index_dir: &std::path::Path,
    embeddings: &std::collections::HashMap<String, Vec<f32>>,
) -> Result<(), String> {
    let path = index_dir.join("embeddings.json");
    let json = serde_json::to_string(embeddings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}
