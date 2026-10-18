use ecotokens::config::settings::EmbedProvider;
use ecotokens::search::embed::{cosine_similarity, embed_text};

// ── Tests provider None ────────────────────────────────────────────────────────

#[test]
fn test_provider_none_returns_none() {
    let result = embed_text("hello world", &EmbedProvider::None);
    assert!(result.is_none(), "provider None doit retourner None");
}

// ── Tests Legacy provider (T068) ──────────────────────────────────────────────

/// Provider Legacy (ancien ollama/lmstudio) → embed_text retourne None (fallback BM25).
#[test]
fn test_legacy_provider_falls_back_to_none() {
    let result = embed_text("hello", &EmbedProvider::Legacy);
    assert!(
        result.is_none(),
        "provider Legacy doit retourner None (fallback BM25)"
    );
}

// ── Tests Ollama provider ──────────────────────────────────────────────────────

#[test]
fn test_ollama_provider_unreachable_returns_none() {
    let provider = EmbedProvider::Ollama {
        url: "http://127.0.0.1:19999".to_string(),
        model: "qwen3-embedding:latest".to_string(),
    };
    let result = embed_text("hello", &provider);
    assert!(
        result.is_none(),
        "Ollama injoignable doit retourner None (fallback BM25)"
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

// ── Ollama: /api/embed with truncation, legacy fallback ─────────────────────────

use ecotokens::search::embed::parse_ollama_embedding;
use std::io::{Read, Write};
use std::net::TcpListener;

#[test]
fn parse_ollama_embed_response_is_normalised() {
    let v = parse_ollama_embedding(&serde_json::json!({ "embeddings": [[3.0, 4.0]] })).unwrap();
    assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
}

#[test]
fn parse_ollama_legacy_response_is_normalised() {
    let v = parse_ollama_embedding(&serde_json::json!({ "embedding": [3.0, 4.0] })).unwrap();
    assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
}

#[test]
fn parse_ollama_rejects_missing_empty_and_zero_vectors() {
    assert!(parse_ollama_embedding(&serde_json::json!({})).is_none());
    assert!(parse_ollama_embedding(&serde_json::json!({ "embeddings": [[]] })).is_none());
    assert!(parse_ollama_embedding(&serde_json::json!({ "embeddings": [] })).is_none());
    assert!(parse_ollama_embedding(&serde_json::json!({ "embedding": [0.0, 0.0] })).is_none());
}

/// Serves one canned `(status, body)` per connection, in order, and returns the
/// raw requests it received once all responses were sent.
fn serve(responses: Vec<(u16, &'static str)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let (head_end, content_len) = loop {
                let n = stream.read(&mut chunk).unwrap();
                buf.extend_from_slice(&chunk[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                    let len = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    break (pos + 4, len);
                }
            };
            while buf.len() < head_end + content_len {
                let n = stream.read(&mut chunk).unwrap();
                buf.extend_from_slice(&chunk[..n]);
            }
            requests.push(String::from_utf8_lossy(&buf).into_owned());
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(reply.as_bytes()).unwrap();
        }
        requests
    });
    (url, handle)
}

#[test]
fn ollama_embedding_uses_api_embed_with_truncate() {
    let (url, server) = serve(vec![(200, r#"{"embeddings": [[3.0, 4.0]]}"#)]);
    let provider = EmbedProvider::Ollama {
        url,
        model: "m".to_string(),
    };
    let v = embed_text("some very long chunk", &provider).expect("embedding");
    assert!((v[0] - 0.6).abs() < 1e-6);

    let requests = server.join().unwrap();
    assert!(
        requests[0].starts_with("POST /api/embed "),
        "{}",
        requests[0]
    );
    assert!(
        requests[0].contains(r#""truncate":true"#),
        "{}",
        requests[0]
    );
    assert!(requests[0].contains(r#""input":"some very long chunk""#));
}

#[test]
fn ollama_embedding_falls_back_to_the_legacy_endpoint_on_404() {
    let (url, server) = serve(vec![(404, "{}"), (200, r#"{"embedding": [3.0, 4.0]}"#)]);
    let provider = EmbedProvider::Ollama {
        url,
        model: "m".to_string(),
    };
    let v = embed_text("chunk", &provider).expect("embedding");
    assert!((v[1] - 0.8).abs() < 1e-6);

    let requests = server.join().unwrap();
    assert!(requests[0].starts_with("POST /api/embed "));
    assert!(requests[1].starts_with("POST /api/embeddings "));
    assert!(requests[1].contains(r#""prompt":"chunk""#));
}
