use crate::config::settings::EmbedProvider;

/// Configuration for an embedding provider API.
struct EmbedConfig<'a> {
    url_pattern: &'a str, // e.g., "{}/api/embeddings" or "{}/v1/embeddings"
    model: &'a str,
    input_field: &'a str,    // e.g., "prompt" or "input"
    embedding_path: &'a str, // e.g., "embedding" or "data[0].embedding"
}

/// Generic HTTP call to fetch an embedding from an API.
fn fetch_embedding(text: &str, base_url: &str, config: &EmbedConfig) -> Result<Vec<f32>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("client error: {e}"))?;

    let url = format!(
        "{}{}",
        base_url.trim_end_matches('/'),
        config.url_pattern.replace("{}", "")
    );
    let body = serde_json::json!({
        "model": config.model,
        config.input_field: text
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

    // Extract embedding based on path
    let embedding = match config.embedding_path {
        "embedding" => data["embedding"].as_array(),
        "data[0].embedding" => data["data"][0]["embedding"].as_array(),
        _ => None,
    }
    .ok_or_else(|| format!("missing '{}' field in response", config.embedding_path))?
    .iter()
    .map(|v| v.as_f64().unwrap_or(0.0) as f32)
    .collect();

    Ok(embedding)
}

/// Obtient l'embedding d'un texte via le provider configuré.
pub fn embed_text(text: &str, provider: &EmbedProvider) -> Option<Vec<f32>> {
    match provider {
        EmbedProvider::None => None,
        EmbedProvider::Ollama { url } => {
            let config = EmbedConfig {
                url_pattern: "{}/api/embeddings",
                model: "nomic-embed-text",
                input_field: "prompt",
                embedding_path: "embedding",
            };
            fetch_embedding(text, url, &config).ok()
        }
        EmbedProvider::LmStudio { url } => {
            let config = EmbedConfig {
                url_pattern: "{}/v1/embeddings",
                model: "nomic-embed-text-v1.5",
                input_field: "input",
                embedding_path: "data[0].embedding",
            };
            fetch_embedding(text, url, &config).ok()
        }
    }
}

/// Appel HTTP vers l'API Ollama pour obtenir l'embedding d'un texte.
#[allow(dead_code)]
pub fn get_ollama_embedding(text: &str, base_url: &str) -> Result<Vec<f32>, String> {
    let config = EmbedConfig {
        url_pattern: "{}/api/embeddings",
        model: "nomic-embed-text",
        input_field: "prompt",
        embedding_path: "embedding",
    };
    fetch_embedding(text, base_url, &config)
}

/// Appel HTTP vers l'API LM Studio (format OpenAI-compatible) pour obtenir l'embedding.
#[allow(dead_code)]
pub fn get_lmstudio_embedding(text: &str, base_url: &str) -> Result<Vec<f32>, String> {
    let config = EmbedConfig {
        url_pattern: "{}/v1/embeddings",
        model: "nomic-embed-text-v1.5",
        input_field: "input",
        embedding_path: "data[0].embedding",
    };
    fetch_embedding(text, base_url, &config)
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
pub fn save_embeddings(
    index_dir: &std::path::Path,
    embeddings: &std::collections::HashMap<String, Vec<f32>>,
) -> Result<(), String> {
    let path = index_dir.join("embeddings.json");
    let json = serde_json::to_string(embeddings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}
