use bincode;
use hnsw_rs::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswMeta {
    pub model_id: String,
    pub dimension: usize,
    pub vector_count: usize,
    pub indexed_at: String,
}

impl HnswMeta {
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let path = dir.join("hnsw_meta.json");
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }

    pub fn load(dir: &Path) -> Option<Self> {
        let data = std::fs::read_to_string(dir.join("hnsw_meta.json")).ok()?;
        serde_json::from_str(&data).ok()
    }
}

#[derive(Serialize, Deserialize)]
struct StoredData {
    id_map: Vec<String>,
    vectors: Vec<Vec<f32>>,
}

/// HNSW-backed vector index for semantic search.
///
/// Vectors are persisted as bincode (`hnsw_index.bin`). The HNSW graph is rebuilt
/// from the stored vectors on each load, which is acceptable for project-scale indices
/// (< 20k vectors ≈ sub-second rebuild on CPU).
pub struct HnswIndex {
    id_map: Vec<String>,
    vectors: Vec<Vec<f32>>,
}

impl HnswIndex {
    /// Build an in-memory index from `(chunk_id, embedding)` pairs.
    pub fn build(data: &[(String, Vec<f32>)]) -> Self {
        HnswIndex {
            id_map: data.iter().map(|(id, _)| id.clone()).collect(),
            vectors: data.iter().map(|(_, v)| v.clone()).collect(),
        }
    }

    /// ANN search: returns `(chunk_id, cosine_similarity)` pairs sorted by score desc.
    pub fn search(&self, query: &[f32], top_k: usize) -> Vec<(String, f32)> {
        if self.vectors.is_empty() || top_k == 0 {
            return vec![];
        }
        let n = self.vectors.len();
        let max_elements = n + 1;
        let hnsw: Hnsw<f32, DistCosine> = Hnsw::new(16, max_elements, 16, 200, DistCosine {});

        let data_refs: Vec<(&Vec<f32>, usize)> = self
            .vectors
            .iter()
            .enumerate()
            .map(|(i, v)| (v, i))
            .collect();
        hnsw.parallel_insert(&data_refs);

        let ef_search = (top_k * 5).max(50);
        let query_vec: Vec<f32> = query.to_vec();
        let neighbours = hnsw.search(&query_vec, top_k, ef_search);

        neighbours
            .into_iter()
            .filter_map(|n| {
                let id = self.id_map.get(n.d_id)?.clone();
                // DistCosine returns 1 - cosine; convert back to similarity
                let score = (1.0_f32 - n.distance).max(0.0);
                Some((id, score))
            })
            .collect()
    }

    /// Persist the index vectors to `{dir}/hnsw_index.bin`.
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let data = StoredData {
            id_map: self.id_map.clone(),
            vectors: self.vectors.clone(),
        };
        let bytes = bincode::serialize(&data).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("hnsw_index.bin"), &bytes).map_err(|e| e.to_string())
    }

    /// Load from `{dir}/hnsw_index.bin`. Returns `None` if absent or corrupt.
    pub fn load(dir: &Path) -> Option<Self> {
        let bytes = std::fs::read(dir.join("hnsw_index.bin")).ok()?;
        let data: StoredData = bincode::deserialize(&bytes).ok()?;
        Some(HnswIndex {
            id_map: data.id_map,
            vectors: data.vectors,
        })
    }

    /// Reconstruit la map `chunk_id → vecteur` depuis l'index sauvegardé.
    pub fn to_embeddings(&self) -> std::collections::HashMap<String, Vec<f32>> {
        self.id_map
            .iter()
            .cloned()
            .zip(self.vectors.iter().cloned())
            .collect()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.id_map.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.id_map.is_empty()
    }
}
