# ecotokens Development Guidelines

Auto-generated from all feature plans. Last updated: 2026-05-01

## Active Technologies
- Rust stable ≥ 1.75 + tantivy 0.22 (index read), similar 2.x (diff/similarity ratio), clap 4 (CLI), rmcp (MCP server), serde/serde_json (serialisation) (002-duplicate-detection)
- Tantivy on-disk index (read-only for this feature); no new storage introduced (002-duplicate-detection)
- Rust stable ≥ 1.75 (no nightly) + serde_json (parsing JSON stdin/stdout), clap 4 (nouvelle sous-commande), tantivy 0.22 (lookup index pour Read), existing filter modules (008-posttooluse-native-tools)
- JSONL existant (`~/.config/ecotokens/metrics.jsonl`) — extension par nouveau champ `hook_type` avec `#[serde(default)]` (008-posttooluse-native-tools)
- Rust stable ≥ 1.75 (no nightly) + tantivy 0.22 (BM25), hnsw_rs 0.3.x (ANN index), candle (embedding), tree-sitter 0.24 (chunking), rmcp (MCP server) (009-semantic-search-embeddings)
- Index tantivy sur disque + `hnsw_index.bin` (bincode) + `hnsw_meta.json` (009-semantic-search-embeddings)

- Rust stable (≥ 1.75, no nightly) (001-token-companion)

## Project Structure

```text
src/
├── embed/
│   ├── mod.rs           # Module embed (009)
│   └── candle.rs        # CandleProvider — inference locale all-MiniLM-L6-v2 (009)
├── search/
│   ├── hnsw.rs          # HnswIndex + HnswMeta — index vectoriel ANN (009)
│   ├── embed.rs         # embed_text (Candle/Ollama/LmStudio), cosine_similarity
│   ├── index.rs         # index_directory, SemanticChunk, chunk_file_by_symbols/lines
│   └── query.rs         # search_index, SearchResult (+ line_end, retrieval_source)
tests/
├── unit/                # Tests unitaires rapides (sans réseau)
│   ├── hnsw_test.rs
│   ├── embed_candle_test.rs
│   └── chunking_symbol_test.rs
└── integration/
    └── semantic_search_test.rs   # Tests end-to-end (#[ignore] pour tests réseau)
```

## Commands

cargo test [ONLY COMMANDS FOR ACTIVE TECHNOLOGIES][ONLY COMMANDS FOR ACTIVE TECHNOLOGIES] cargo clippy

## Code Style

Rust stable (≥ 1.75, no nightly): Follow standard conventions

## Recent Changes
- 009-semantic-search-embeddings: Added Rust stable ≥ 1.75 (no nightly) + tantivy 0.22 (BM25), hnsw_rs 0.3.x (ANN index), candle (embedding), tree-sitter 0.24 (chunking), rmcp (MCP server)
- 008-posttooluse-native-tools: Added Rust stable ≥ 1.75 (no nightly) + serde_json (parsing JSON stdin/stdout), clap 4 (nouvelle sous-commande), tantivy 0.22 (lookup index pour Read), existing filter modules
- 004-savings-history-periods: Added [if applicable, e.g., PostgreSQL, CoreData, files or N/A]


<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
