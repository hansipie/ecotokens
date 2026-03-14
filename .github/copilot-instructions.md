# ecotokens — Copilot Instructions

## What this project does

**ecotokens** is a token-saving companion for Claude Code and Gemini CLI. It intercepts bash/shell command outputs before they reach the model, compresses them with family-specific filters, and records how many tokens were saved.

## Commands

```bash
# Build
cargo build

# Test full suite
cargo test

# Test a single integration or unit test file (no .rs suffix)
cargo test --test filter_git_test
cargo test --test gain_tui_test

# Lint (warnings are errors)
cargo clippy -- -D warnings

# Format check (required before push)
cargo fmt --check
cargo fmt

# Pre-push one-liner
cargo fmt --check && cargo clippy -- -D warnings && cargo test

# Install ecotokens hook
ecotokens install --target claude    # Claude Code only
ecotokens install --target gemini    # Gemini CLI (writes BeforeTool hook + mcpServers in ~/.gemini/settings.json)
ecotokens install --target all       # all targets
ecotokens uninstall --target gemini  # remove Gemini registration
```

## Architecture

The project is organized into 12 modules, each with a `mod.rs` exporting its public API:

| Module | Role |
|---|---|
| `filter/` | Family-specific output compression (Git, Cargo, Python, C++, FS, Markdown, ConfigFile, Generic) |
| `hook/` | Handles hook stdin→stdout for both Claude Code (`PreToolUse`, `ecotokens hook`) and Gemini CLI (`BeforeTool`, `ecotokens hook-gemini`) |
| `metrics/` | Appends `Interception` records to `~/.config/ecotokens/metrics.jsonl`; aggregates for reports |
| `tokens/` | `estimate_tokens(s)` → `chars * 0.25`; optional exact counting via `tiktoken-rs` feature gate |
| `search/` | Tantivy BM25 index + tree-sitter symbol extraction; chunk size 50 lines |
| `trace/` | Call graph queries (callers/callees) over the search index |
| `tui/` | Ratatui dashboards (gain, outline, trace, progress, watch) |
| `mcp/` | JSON-RPC stdio server (rmcp crate) exposing 4 tools |
| `config/` | TOML settings at `~/.config/ecotokens/settings.toml`; all fields optional with defaults |
| `masking/` | Regex-based PII/secret redaction (AWS keys, GitHub PATs, JWTs, `.env` secrets, etc.) |
| `install/` | Idempotent hook + MCP registration for Claude Code (`~/.claude/settings.json` + `~/.claude.json`) and Gemini CLI (`~/.gemini/settings.json`) |
| `daemon/` | `notify`-based file watcher; debounces 500 ms before reindexing |

**Data flow:**
```
AI CLI bash call (Claude Code or Gemini CLI)
  → hook (rewrite command via PreToolUse or BeforeTool)
    → filter (run cmd, compress output, return filtered text)
      → metrics (record before/after token counts)
        → AI CLI (sees compressed output)
```

## Key conventions

### Error handling
- Fallible functions return `Result<T>`. Non-critical paths use `.unwrap_or()`. Test setup uses `.expect("message")` — never bare `.unwrap()` in tests.
- Domain errors use custom enums (e.g., `TraceError`). No `unsafe` blocks.

### Token counting
- Default is the estimate path (`chars * 0.25`). The `exact-tokens` feature flag enables tiktoken-rs. Never assume exact counting is active unless explicitly configured.

### Filter pattern
- Each command family implements a dedicated filter function (e.g., `filter_git()`, `filter_cargo()`). When no family matches, `filter_generic()` applies head+tail truncation (first/last 20 lines + summary).
- Thresholds (500 lines / 50 KB by default) come from `Settings`; filters must respect them.

### Metrics storage
- `Interception` is the central record type (before/after tokens, command string, family, git root, timestamp, mode). Written as JSONL — one JSON object per line, never a JSON array.

### Testing
- Unit tests live in `tests/<module>_test.rs`, not inline. Integration tests are in `tests/integration/`.
- All tests that touch the filesystem use `tempfile::tempdir()` — never write to real home directories.
- Ratatui tests use `TestBackend` for headless rendering.

### Async boundary
- `tokio` is used **only** for the MCP server. The file watcher uses `std::sync::mpsc`. Don't introduce async into non-MCP modules.

### Code style
- Edition 2021, MSRV 1.75 stable (no nightly features).
- Line width: 100 characters (enforced by `rustfmt`).
- Import groups: `std` → external crates → `crate` (enforced by `rustfmt`).
- `serde` field naming: `rename_all = "snake_case"` on structs.

### Symbol IDs
- Symbols extracted by tree-sitter get stable UUID v4 IDs that survive file edits. Use these IDs (not file+line) when referring to symbols across sessions.
