# ecotokens

Token-saving companion for [Claude Code](https://claude.ai/code) and [GitHub Copilot](https://github.com/features/copilot) (VS Code). ecotokens intercepts tool outputs before they reach the model, filters the noise, and records how many tokens you saved.

## How it works

ecotokens works in two complementary modes:

**Hook mode (Claude Code only)** — installs as a `PreToolUse` hook. When Claude runs a shell command, ecotokens:

1. Runs the command and captures its output
2. Applies a family-specific filter (git, cargo, python, …)
3. Returns the compressed output to Claude
4. Records the before/after token counts in a local metrics store

**MCP server mode (Claude Code + GitHub Copilot in VS Code)** — exposes five tools the model can call directly: codebase search, symbol lookup, call graph tracing, and `ecotokens_run` (runs any shell command and returns token-optimized output).

The result: the model sees clean, concise output — and you keep your context window.

## Installation

### Claude Code

```bash
cargo install --path .
ecotokens install                # hook only
ecotokens install --with-mcp     # hook + MCP server (search, outline, trace, run)
```

### GitHub Copilot (VS Code)

Requires VS Code ≥ 1.99 with the GitHub Copilot extension.

```bash
cargo install --path .
ecotokens install --target vscode
```

This writes the MCP server entry into `~/.config/Code/User/settings.json`. Copilot in Agent mode will then have access to all five ecotokens tools.

### Both at once

```bash
ecotokens install --target all --with-mcp
```

### Uninstall

```bash
ecotokens uninstall                   # Claude Code
ecotokens uninstall --target vscode   # VS Code
ecotokens uninstall --target all      # both
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

When registered (via `--with-mcp` for Claude Code, or `--target vscode` for Copilot), ecotokens exposes five tools:

- **`ecotokens_search`** — semantic search over the indexed codebase
- **`ecotokens_outline`** — list symbols in a file or directory
- **`ecotokens_symbol`** — retrieve a symbol's source by stable ID
- **`ecotokens_trace_callers`** / **`ecotokens_trace_callees`** — call graph tracing
- **`ecotokens_run`** — execute a shell command and return token-optimized output (filters noise, records savings)

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
- Claude Code with hook support, and/or VS Code ≥ 1.99 with GitHub Copilot

## License

MIT
