# ecotokens Development Guidelines

Auto-generated from all feature plans. Last updated: 2026-02-21

## Active Technologies
- Rust stable ≥ 1.75 + tantivy 0.22 (index read), similar 2.x (diff/similarity ratio), clap 4 (CLI), rmcp (MCP server), serde/serde_json (serialisation) (002-duplicate-detection)
- Tantivy on-disk index (read-only for this feature); no new storage introduced (002-duplicate-detection)

- Rust stable (≥ 1.75, no nightly) (001-token-companion)

## Project Structure

```text
src/
tests/
```

## Commands

cargo test [ONLY COMMANDS FOR ACTIVE TECHNOLOGIES][ONLY COMMANDS FOR ACTIVE TECHNOLOGIES] cargo clippy

## Code Style

Rust stable (≥ 1.75, no nightly): Follow standard conventions

## Recent Changes
- 002-duplicate-detection: Added Rust stable ≥ 1.75 + tantivy 0.22 (index read), similar 2.x (diff/similarity ratio), clap 4 (CLI), rmcp (MCP server), serde/serde_json (serialisation)

- 001-token-companion: Added Rust stable (≥ 1.75, no nightly)

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
