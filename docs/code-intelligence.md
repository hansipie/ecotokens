# Code intelligence

[← Back to the README](../README.md)

Index a codebase, search it, trace calls, find duplicates, and expose all of it to agents over MCP.

## Watch command

`ecotokens watch` monitors a directory and automatically re-indexes files as they change.

```bash
ecotokens watch                    # foreground, TUI progress
ecotokens watch --path ./src       # watch a specific directory
ecotokens watch --background       # fork to background
ecotokens watch --status           # show status of background process
ecotokens watch --status --json    # JSON status output
ecotokens watch --stop             # stop the background process
```

> **Note:** Background logs are only written if global `debug` is enabled (`ecotokens config --debug true`).

### Auto-watch *(Claude Code, Codex, Hermes, Pi, Qwen Code)*

`ecotokens auto-watch` integrates with agent session lifecycles to start and stop the watcher automatically where both lifecycle events are available.

```bash
ecotokens auto-watch enable    # enable auto-watch
ecotokens auto-watch disable   # disable (hooks remain installed but are no-ops)
```

When enabled, `ecotokens watch --background` starts automatically when a session opens, and stops when it closes on agents that expose an end-of-session event. The setting is stored in `~/.config/ecotokens/config.json` (`auto_watch: true/false`).

Support by agent:

| Agent | Mechanism | Notes |
|-------|-----------|-------|
| Claude Code | `SessionStart` / `SessionEnd` shell hooks in `~/.claude/settings.json` | Installed by `auto-watch enable` |
| Codex | — | Session hooks not yet supported; auto-watch not available for Codex |
| Hermes | `on_session_start` / `on_session_end` plugin hooks | Built into the Hermes plugin; install first with `ecotokens install --target hermes` |
| Pi | `session_start` / `session_end` events in the TypeScript extension | Built into the Pi extension |
| Qwen Code | `SessionStart` / `SessionEnd` shell hooks in `~/.qwen/settings.json` | Installed automatically if Qwen hook is present |
| Gemini CLI | — | Gemini does not expose session lifecycle hooks |

## MCP server (Claude Code, Codex, Gemini CLI, Qwen Code)

`ecotokens mcp-server` starts a stdio MCP server backed by the ecotokens index and trace engines.

```bash
ecotokens mcp-server
ecotokens mcp-server --index-dir ~/.config/ecotokens/index
```

Exposed tools:

- `ecotokens_search` - BM25 + semantic search
- `ecotokens_outline` - symbol outline for file/directory
- `ecotokens_symbol` - fetch full symbol source by stable ID
- `ecotokens_trace_callers` - find callers of a symbol
- `ecotokens_trace_callees` - find callees (with depth)
- `ecotokens_duplicates` - detect near-duplicate code blocks
- `ecotokens_rewrite` - paraphrase, retone, or translate a block of prose using a local model

For Claude Code, Codex, Gemini CLI, and Qwen Code, `ecotokens install` registers this server automatically in each target's settings file (`mcpServers` in JSON settings, `[mcp_servers.ecotokens]` in Codex's `config.toml`).

Full reference for each tool: [mcp-server.md](mcp-server.md).

## Search command

`ecotokens search QUERY` performs BM25 (+ optional semantic) search over the indexed codebase and returns results anchored to the matching line.

```bash
ecotokens search "embed_text"                        # top 5 results, 2 lines of context
ecotokens search "embed_text" --context 4            # 4 lines above and below the match
ecotokens search "error" --include "*.rs"            # Rust files only
ecotokens search "TODO" --exclude "*.md" --exclude "*.toml"
ecotokens search "find_callers" --no-trace           # pure BM25, no trace augmentation
ecotokens search "find_callers" --json               # JSON output with callers array
ecotokens search "query" --top-k 10                  # more results
```

Output format:

```
src/search/query.rs:29 (score: 11.068)
  27:  
  28:  pub fn search_index(opts: SearchOptions) -> tantivy::Result<Vec<SearchResult>> {
  29:      let index = Index::open_in_dir(&opts.index_dir)?;
  30:      let (_, file_path_field, content_field, kind_field, line_start_field, _) = build_schema();
  31:  
```

When the query matches a symbol name, callers are automatically appended:

```
# Symbol match - call sites via trace
  src/main.rs:1301 [caller]  cmd_search
```

Results are automatically scoped to the current git project when using the global index - files from other indexed projects are silently filtered out.

## Duplicates command

_Less code is less tokens_

`ecotokens duplicates` scans the indexed codebase for near-identical code blocks and reports them grouped by similarity.

```bash
ecotokens duplicates                          # default: threshold=70%, min_lines=5
ecotokens duplicates --threshold 80           # only report ≥ 80% similarity
ecotokens duplicates --min-lines 10           # ignore blocks shorter than 10 lines
ecotokens duplicates --json                   # JSON output
```

Each group shows the file paths, line ranges, similarity score, and a refactoring proposal (exact duplicate, near duplicate, or subset).

## Embeddings

`ecotokens search` uses **dual BM25 + vector retrieval** with score fusion (`0.4 × BM25 + 0.6 × cosine`). Two embedding backends are available: the built-in Candle engine (zero-config) or an external Ollama instance.

### Provider

**Candle** (default) — runs `sentence-transformers/all-MiniLM-L6-v2` (384 dim) locally. The model is downloaded automatically from HuggingFace Hub on first use (~90 MB, cached in `~/.cache/huggingface/`).

```bash
# Candle is active by default - nothing to configure
ecotokens index --path /your/project
ecotokens search "your query"
```

**Ollama** — delegates embedding to a running Ollama instance. Supports any Ollama embedding model, including large-dimension models like `qwen3-embedding:latest` (2560 dim).

```bash
# Switch to Ollama with qwen3-embedding (Ollama must be running)
ecotokens config --embed-provider ollama --embed-model qwen3-embedding:latest

# Custom URL (default: http://localhost:11434)
ecotokens config --embed-provider ollama \
  --embed-url http://localhost:11434 \
  --embed-model qwen3-embedding:latest

# Change just the model without touching the provider
ecotokens config --embed-model nomic-embed-text

# Revert to Candle
ecotokens config --embed-provider candle
```

> When switching between providers (or between models with different output dimensions), `ecotokens index` automatically detects the change and rebuilds the vector index.

Each result includes a `retrieval_source` field (`bm25`, `vector`, or `both`) visible in JSON output.

### Disable embeddings

```bash
ecotokens config --embed-provider none    # fall back to pure BM25
```

### Workflow

```bash
# 1. Index your project (Candle embeddings computed automatically)
ecotokens index --path /your/project

# 2. Search with hybrid scoring
ecotokens search "your query"

# 3. JSON output with retrieval_source
ecotokens search "your query" --json
```

### Model change detection

When the configured embedding model changes, `ecotokens index` automatically rebuilds the vector index (`hnsw_index.bin`) without touching the BM25 index. Embeddings for unchanged files are reused between runs.
