use ecotokens::search::hnsw::HnswMeta;
use tempfile::TempDir;

#[cfg(test)]
mod tests {
    use super::*;

    // T011
    #[test]
    fn hnsw_meta_roundtrip() {
        let dir = TempDir::new().unwrap();
        let meta = HnswMeta {
            model_id: "all-MiniLM-L6-v2".to_string(),
            dimension: 384,
            vector_count: 42,
            indexed_at: "2026-05-01T00:00:00Z".to_string(),
        };
        meta.save(dir.path()).expect("save failed");

        let loaded = HnswMeta::load(dir.path()).expect("load returned None");
        assert_eq!(loaded.model_id, "all-MiniLM-L6-v2");
        assert_eq!(loaded.dimension, 384);
        assert_eq!(loaded.vector_count, 42);
    }

    // T019
    #[test]
    fn hnsw_build_search_cosine() {
        use ecotokens::search::hnsw::HnswIndex;

        // 10 synthetic 384-dim vectors; vector 0 is the query target
        let dim = 384;
        let mut data: Vec<(String, Vec<f32>)> = (0..10)
            .map(|i| {
                let mut v = vec![0.0f32; dim];
                v[i] = 1.0; // orthogonal unit vectors
                (format!("chunk_{i}"), v)
            })
            .collect();

        // Make vector 0 the "nearest" to a query close to it
        let index = HnswIndex::build(&data);

        let mut query = vec![0.0f32; dim];
        query[0] = 1.0; // identical to chunk_0

        let results = index.search(&query, 3);
        assert!(!results.is_empty(), "search returned no results");
        assert_eq!(results[0].0, "chunk_0", "top-1 should be chunk_0");

        // Drop unused mut
        let _ = &mut data;
    }

    // T020
    #[test]
    fn hnsw_save_load_roundtrip() {
        use ecotokens::search::hnsw::HnswIndex;

        let dir = TempDir::new().unwrap();
        let dim = 384;
        let data: Vec<(String, Vec<f32>)> = (0..5)
            .map(|i| {
                let mut v = vec![0.0f32; dim];
                v[i] = 1.0;
                (format!("c{i}"), v)
            })
            .collect();

        let index = HnswIndex::build(&data);
        index.save(dir.path()).expect("save failed");

        let loaded = HnswIndex::load(dir.path()).expect("load returned None");

        let mut query = vec![0.0f32; dim];
        query[2] = 1.0;

        let r1 = index.search(&query, 1);
        let r2 = loaded.search(&query, 1);
        assert_eq!(
            r1[0].0, r2[0].0,
            "results should be identical after roundtrip"
        );
    }
}
