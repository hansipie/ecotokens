# ecotokens

Token-saving companion for [Claude Code](https://claude.ai/code). ecotokens intercepts tool outputs before they reach the model, filters the noise, and records how many tokens you saved.

## How it works

ecotokens installs as a `PreToolUse` hook in Claude Code. When Claude runs a shell command, ecotokens:

1. Runs the command and captures its output
2. Applies a family-specific filter (git, cargo, python, …)
3. Returns the compressed output to Claude
4. Records the before/after token counts in a local metrics store

The result: Claude sees a clean, concise output — and you keep your context window.

## Installation

```bash
cargo install --path .
ecotokens install
```

To also register the MCP server (enables `search`, `outline`, `trace` from Claude Code):

```bash
ecotokens install --with-mcp
```

To uninstall:

```bash
ecotokens uninstall
```

## Commands

| Command | Description |
|---------|-------------|
| `ecotokens gain` | Interactive TUI dashboard — savings by family or project |
| `ecotokens gain --json` | JSON report |
| `ecotokens install` | Install the PreToolUse hook in `~/.claude/settings.json` |
| `ecotokens uninstall` | Remove the hook |
| `ecotokens config` | Show current configuration |
| `ecotokens index [--path DIR]` | Index a codebase for BM25 + symbolic search |
| `ecotokens search QUERY` | Search the indexed codebase |
| `ecotokens outline PATH` | List symbols in a file or directory |
| `ecotokens symbol ID` | Look up a symbol by its stable ID |
| `ecotokens trace callers SYMBOL` | Find callers of a symbol |
| `ecotokens trace callees SYMBOL` | Find callees of a symbol |
| `ecotokens watch [--path DIR]` | Watch a directory and keep the index up to date |
| `ecotokens mcp` | Start the MCP server (JSON-RPC over stdio) |

## Gain dashboard

```
ecotokens gain
```

Interactive TUI showing token savings per command family and per project, with a 14-day sparkline.

**Keybindings:**

| Key | Action |
|-----|--------|
| `↑` / `↓` or `j` / `k` | Navigate |
| `b` | Toggle family / project view |
| `d` | Cycle detail mode (split / diff / log) — family view only |
| `s` | Cycle sparkline scale (linear / log / capped) |
| `q` / `Esc` | Quit |

## Supported command families

| Family | Examples |
|--------|----------|
| `git` | `git status`, `git diff`, `git log` |
| `cargo` | `cargo build`, `cargo test`, `cargo clippy` |
| `python` | `pytest`, `ruff`, `uv run` |
| `cpp` | `gcc`, `clang`, `make`, `cmake` |
| `fs` | `ls`, `find`, `tree` |
| `markdown` | `.md` files |
| `config` | `.toml`, `.json`, `.yaml` |
| `generic` | Everything else (truncated to 200 lines / 50 KB) |

## MCP server

When installed with `--with-mcp`, ecotokens exposes four tools to Claude Code:

- **`ecotokens_search`** — semantic search over the indexed codebase
- **`ecotokens_outline`** — list symbols in a file or directory
- **`ecotokens_symbol`** — retrieve a symbol's source by stable ID
- **`ecotokens_trace_callers`** / **`ecotokens_trace_callees`** — call graph tracing

## Embeddings (optional)

For semantic search, configure a local embedding provider:

```bash
# Ollama
ecotokens config --embed-provider ollama --embed-url http://localhost:11434

# LM Studio
ecotokens config --embed-provider lmstudio --embed-url http://localhost:1234

# Disable
ecotokens config --embed-provider none
```

## Requirements

- Rust ≥ 1.75 (stable)
- Claude Code with hook support

## License

MIT
