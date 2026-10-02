use bincode;
use hnsw_rs::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

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
        // Atomic write (project policy): a crash mid-write must not leave a
        // truncated meta file that silently disables `model_changed` detection.
        crate::config::atomic_write(&path, json).map_err(|e| e.to_string())
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

/// Process-level cache of loaded indices, keyed by index directory and validated
/// against the mtime of `hnsw_index.bin`.
///
/// The per-instance `OnceLock` graph only avoids a rebuild for a *reused*
/// `HnswIndex`; callers that reload the index per query (the MCP server handles
/// one `search_index` call per client request) would otherwise deserialise the
/// whole vector file and rebuild the graph from scratch every single time. See
/// [`HnswIndex::load_cached`].
type IndexCache = Mutex<HashMap<PathBuf, (SystemTime, Arc<HnswIndex>)>>;

fn index_cache() -> &'static IndexCache {
    static CACHE: OnceLock<IndexCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// HNSW-backed vector index for semantic search.
///
/// Vectors are persisted as bincode (`hnsw_index.bin`). The HNSW graph is built
/// lazily on first search and memoised for the lifetime of the instance; use
/// [`HnswIndex::load_cached`] so that lifetime spans the whole process rather
/// than a single query.
pub struct HnswIndex {
    id_map: Vec<String>,
    vectors: Vec<Vec<f32>>,
    /// Built lazily on first `search()` and reused for the lifetime of the
    /// instance, so a reused index never pays the O(N log N) build cost twice.
    graph: OnceLock<Hnsw<'static, f32, DistCosine>>,
}

impl HnswIndex {
    /// Build an in-memory index from `(chunk_id, embedding)` pairs.
    pub fn build(data: &[(String, Vec<f32>)]) -> Self {
        HnswIndex {
            id_map: data.iter().map(|(id, _)| id.clone()).collect(),
            vectors: data.iter().map(|(_, v)| v.clone()).collect(),
            graph: OnceLock::new(),
        }
    }

    /// Dimension of the index, taken from the first non-empty vector (0 if none).
    fn dimension(&self) -> usize {
        self.vectors
            .iter()
            .map(|v| v.len())
            .find(|&l| l > 0)
            .unwrap_or(0)
    }

    /// Build the HNSW graph once, inserting only vectors whose length matches the
    /// index dimension. Heterogeneous or zero-length vectors (e.g. left over from a
    /// partial re-index with a different model) are skipped rather than fed to
    /// `DistCosine`, which would return garbage scores or panic.
    fn build_graph(&self) -> Hnsw<'static, f32, DistCosine> {
        let dim = self.dimension();
        let max_elements = self.vectors.len() + 1;
        let hnsw: Hnsw<'static, f32, DistCosine> =
            Hnsw::new(16, max_elements, 16, 200, DistCosine {});
        if dim == 0 {
            return hnsw;
        }
        for (i, v) in self.vectors.iter().enumerate() {
            if v.len() == dim {
                hnsw.insert((v.as_slice(), i));
            }
        }
        hnsw
    }

    /// ANN search: returns `(chunk_id, cosine_similarity)` pairs sorted by score desc.
    pub fn search(&self, query: &[f32], top_k: usize) -> Vec<(String, f32)> {
        if self.vectors.is_empty() || top_k == 0 {
            return vec![];
        }
        // Reject a query whose dimension does not match the index to avoid a
        // dimension-mismatch panic/garbage inside `DistCosine`.
        let dim = self.dimension();
        if dim == 0 || query.len() != dim {
            return vec![];
        }

        let hnsw = self.graph.get_or_init(|| self.build_graph());

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
        // Atomic write (project policy): a crash during this potentially large
        // binary write must not leave a truncated file that `load` silently
        // discards, losing the entire index.
        crate::config::atomic_write(&dir.join("hnsw_index.bin"), &bytes).map_err(|e| e.to_string())
    }

    /// Load from `{dir}/hnsw_index.bin`. Returns `None` if absent or corrupt.
    pub fn load(dir: &Path) -> Option<Self> {
        let bytes = std::fs::read(dir.join("hnsw_index.bin")).ok()?;
        let data: StoredData = bincode::deserialize(&bytes).ok()?;
        Some(HnswIndex {
            id_map: data.id_map,
            vectors: data.vectors,
            graph: OnceLock::new(),
        })
    }

    /// Like [`HnswIndex::load`], but shares one instance per index directory for
    /// the lifetime of the process, so the deserialisation *and* the lazily built
    /// HNSW graph are paid once instead of once per query.
    ///
    /// The cache entry is keyed on the mtime of `hnsw_index.bin`: a re-index
    /// (which rewrites that file) yields a fresh mtime and transparently evicts
    /// the stale entry, so a long-running MCP server never serves results from a
    /// superseded index.
    pub fn load_cached(dir: &Path) -> Option<Arc<Self>> {
        let mtime = std::fs::metadata(dir.join("hnsw_index.bin"))
            .and_then(|m| m.modified())
            .ok()?;

        if let Ok(cache) = index_cache().lock() {
            if let Some((cached_mtime, index)) = cache.get(dir) {
                if *cached_mtime == mtime {
                    return Some(Arc::clone(index));
                }
            }
        }

        let index = Arc::new(Self::load(dir)?);
        if let Ok(mut cache) = index_cache().lock() {
            cache.insert(dir.to_path_buf(), (mtime, Arc::clone(&index)));
        }
        Some(index)
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
