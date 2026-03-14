<p align="center">
  <img src="assets/banner.png" alt="ecotokens">
</p>

Token-saving companion for [Claude Code](https://claude.ai/code), [Gemini CLI](https://github.com/google-gemini/gemini-cli), and [GitHub Copilot](https://github.com/features/copilot) (VS Code). ecotokens intercepts tool outputs before they reach the model, filters the noise, and records how many tokens you saved.

<p align="center">
  <img src="docs/demo.gif" alt="ecotokens demo" width="800">
</p>

## How it works

ecotokens works in two complementary modes:

**Hook mode (Claude Code + Gemini CLI)** — installs as a hook that fires before every shell command. When the AI runs a shell command, ecotokens:

1. Runs the command and captures its output
2. Applies a family-specific filter (git, cargo, python, …)
3. Optionally summarizes large outputs via a local AI model (Ollama)
4. Returns the compressed output to the model
5. Records the before/after token counts in a local metrics store

Claude Code uses the `PreToolUse` hook (`~/.claude/settings.json`). Gemini CLI uses the `BeforeTool` hook (`~/.gemini/settings.json`).

For a focused view of the runtime path, see [`docs/hook-filter-metrics-flow.md`](docs/hook-filter-metrics-flow.md).

**MCP server mode (Claude Code + Gemini CLI + GitHub Copilot in VS Code)** — exposes tools the model can call directly: codebase search, symbol lookup, call graph tracing, and `ecotokens_run` (runs any shell command and returns token-optimized output).

The result: the model sees clean, concise output — and you keep your context window.

## Quick install

```bash
cargo install --git https://github.com/hansipie/ecotokens
```

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

This writes the MCP server entry into `~/.config/Code/User/settings.json`. Copilot in Agent mode will then have access to all ecotokens tools.

### Gemini CLI

Requires [Gemini CLI](https://github.com/google-gemini/gemini-cli) ≥ 0.1.0.

```bash
cargo install --path .
ecotokens install --target gemini                # hook only
ecotokens install --target gemini --with-mcp     # hook + MCP server
```

This writes a `BeforeTool` hook entry and (optionally) an MCP server entry into `~/.gemini/settings.json`.

### All targets at once

```bash
ecotokens install --target all --with-mcp
```

`--target all` covers Claude Code, Gemini CLI, and VS Code in a single command.

### With AI summarization

Enable AI-powered output compression via Ollama at install time:

```bash
ecotokens install --ai-summary                          # use default model (llama3.2:3b)
ecotokens install --ai-summary-model qwen2.5:3b         # specify model (implies --ai-summary)
ecotokens install --with-mcp --ai-summary-model llama3.2:3b  # combined
```

This writes `ai_summary_enabled` and `ai_summary_model` to `~/.config/ecotokens/config.json`. Ollama must be running and the model must be pulled (`ollama pull llama3.2:3b`).

### Uninstall

```bash
ecotokens uninstall                    # Claude Code
ecotokens uninstall --target vscode    # VS Code
ecotokens uninstall --target gemini    # Gemini CLI
ecotokens uninstall --target all       # all targets
```

## Commands

| Command | Description |
|---------|-------------|
| `ecotokens install` | Install the PreToolUse hook in `~/.claude/settings.json` |
| `ecotokens uninstall` | Remove the hook |
| `ecotokens filter -- CMD [ARGS]` | Run a command, filter its output, record metrics |
| `ecotokens gain` | Interactive TUI dashboard — savings by family or project |
| `ecotokens gain --json` | JSON report |
| `ecotokens config` | Show current configuration |
| `ecotokens index [--path DIR]` | Index a codebase for BM25 + symbolic search |
| `ecotokens search QUERY` | Search the indexed codebase |
| `ecotokens outline PATH` | List symbols in a file or directory |
| `ecotokens symbol ID` | Look up a symbol by its stable ID |
| `ecotokens trace callers SYMBOL` | Find callers of a symbol |
| `ecotokens trace callees SYMBOL` | Find callees of a symbol |
| `ecotokens watch [--path DIR]` | Watch a directory and keep the index up to date |
| `ecotokens duplicates` | Detect near-duplicate code blocks in the indexed codebase |
| `ecotokens mcp` | Start the MCP server (JSON-RPC over stdio) |

## Gain dashboard

```
ecotokens gain
ecotokens gain --period 7d
ecotokens gain --period today --model claude-sonnet-4-5
```

Interactive TUI showing token savings per command family and per project, with a sparkline.

**Keybindings:**

| Key | Action |
|-----|--------|
| `j` / `u` | Navigate up / down in list |
| `k` / `i` | Scroll history log down / up (family log view) |
| `b` | Toggle family / project view |
| `d` | Cycle detail mode (split / diff / log) — family view only |
| `s` | Cycle sparkline scale (linear / log / capped) |
| `q` / `Esc` | Quit |

## Filter command

`ecotokens filter` runs a command directly and returns its filtered output. Useful for testing filters or wrapping commands in scripts:

```bash
ecotokens filter -- cargo test
ecotokens filter --debug -- git log --oneline -50
```

The output is compressed by the same family-specific filters used by the hook, and token savings are recorded in the metrics store.

## Watch command

`ecotokens watch` monitors a directory and automatically re-indexes files as they change.

```bash
ecotokens watch                    # foreground, TUI progress
ecotokens watch --path ./src       # watch a specific directory
ecotokens watch --background       # fork to background, log to stdout
ecotokens watch --status           # show status of background process
ecotokens watch --status --json    # JSON status output
ecotokens watch --stop             # stop the background process
```

## Duplicates command

`ecotokens duplicates` scans the indexed codebase for near-identical code blocks and reports them grouped by similarity.

```bash
ecotokens duplicates                          # default: threshold=70%, min_lines=5, top_k=10
ecotokens duplicates --threshold 80           # only report ≥ 80% similarity
ecotokens duplicates --min-lines 10           # ignore blocks shorter than 10 lines
ecotokens duplicates --top-k 20              # return up to 20 groups
```

Each group shows the file paths, line ranges, similarity score, and a refactoring proposal (exact duplicate, near duplicate, or subset).

The `ecotokens_duplicates` MCP tool exposes the same feature directly to the model.

## Configuration

```bash
ecotokens config           # show all settings (text)
ecotokens config --json    # show all settings (JSON)
```

Output includes:

```
hook_installed        : true
mcp_registered        : true
vscode_mcp_registered : false
debug                 : false
threshold_lines       : 500
threshold_bytes       : 51200
exclusions            : []
embed_provider        : ollama (http://localhost:11434)
ai_summary_enabled    : false
ai_summary_model      : llama3.2:3b (default)
```

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

When registered (via `--with-mcp` for Claude Code or Gemini CLI, or `--target vscode` for Copilot), ecotokens exposes these tools:

| Tool | Description |
|------|-------------|
| `ecotokens_search` | Semantic + BM25 search over the indexed codebase |
| `ecotokens_outline` | List symbols in a file or directory |
| `ecotokens_symbol` | Retrieve a symbol's source by stable ID |
| `ecotokens_trace_callers` | Find callers of a symbol (call graph) |
| `ecotokens_trace_callees` | Find callees of a symbol (call graph) |
| `ecotokens_run` | Execute a shell command and return token-optimized output |
| `ecotokens_duplicates` | Detect near-duplicate code blocks in the indexed codebase |

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

## AI summarization (optional)

When enabled, large command outputs (> ~2500 tokens) are summarized by a local Ollama model instead of being truncated. Falls back to generic filtering if Ollama is unavailable or times out.

Enable via install:

```bash
ecotokens install --ai-summary-model llama3.2:3b
```

Or update the config file directly (`~/.config/ecotokens/config.json`):

```json
{
  "ai_summary_enabled": true,
  "ai_summary_model": "llama3.2:3b"
}
```

Ollama must be running locally. The model is called with a 3-second timeout to avoid blocking the model.

## Requirements

- Rust ≥ 1.75 (stable)
- One or more of: Claude Code (with hook support), Gemini CLI ≥ 0.1.0, VS Code ≥ 1.99 with GitHub Copilot
- Ollama (optional, for semantic search embeddings and AI summarization)

## License

MIT
