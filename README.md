<p align="center">
  <img src="assets/banner.png" alt="ecotokens">
</p>

<p align="center">
  <a href="https://github.com/hansipie/ecotokens/actions/workflows/ci.yml"><img src="https://github.com/hansipie/ecotokens/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://crates.io/crates/ecotokens"><img src="https://img.shields.io/crates/v/ecotokens.svg" alt="crates.io"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License"></a>
</p>

<p align="right">
  <a href="https://www.producthunt.com/products/ecotokens?embed=true&amp;utm_source=badge-featured&amp;utm_medium=badge&amp;utm_campaign=badge-ecotokens" target="_blank" rel="noopener noreferrer"><img alt="EcoTokens - Save 90% of your AI coding tokens — automatically | Product Hunt" width="250" height="54" src="https://api.producthunt.com/widgets/embed-image/v1/featured.svg?post_id=1105858&amp;theme=light&amp;t=1774341425433"></a>
</p>
<br>

Token-saving companion for [Claude Code](https://claude.ai/code), [Gemini CLI](https://github.com/google-gemini/gemini-cli), and [Qwen Code](https://github.com/QwenLM/qwen-code). Built on a *"set it and forget it!"* philosophy: one install command, zero configuration, and ecotokens works automatically from there — intercepting tool outputs before they reach the model, filtering the noise, and recording how many tokens you saved.

<p align="center">
  <img src="assets/demo.gif" alt="ecotokens demo" width="800">
</p>

## How it works

ecotokens installs hooks that intercept tool outputs before they reach the model. Two interception points are supported:

**PreToolUse / BeforeTool** — fires before every shell (`Bash`) command:

1. Runs the command and captures its output
2. Applies a family-specific filter (git, cargo, python, …)
3. Optionally summarizes large outputs via a local AI model (Ollama)
4. Returns the compressed output to the model
5. Records the before/after token counts in a local metrics store

**PostToolUse** *(Claude Code only)* — fires after native tool calls (`Read`, `Grep`, `Glob`):

1. Intercepts the tool result before it enters the context window
2. Applies a specialized filter (outline for source files, grep result trimming, glob path denoising)
3. Returns the compressed result to the model
4. Records the savings under the `native_read`, `grep`, or `fs` family

Claude Code uses the `PreToolUse` + `PostToolUse` hooks (`~/.claude/settings.json`). Gemini CLI uses the `BeforeTool` hook (`~/.gemini/settings.json`). Qwen Code uses the `PreToolUse` hook (`~/.qwen/settings.json`).

For a focused view of the runtime path, see [`docs/hook-filter-metrics-flow.md`](docs/hook-filter-metrics-flow.md).

The result: the model sees clean, concise output — and you keep your context window.

## Quick install

```bash
cargo install --git https://github.com/hansipie/ecotokens
```

For exact token counting (tiktoken cl100k_base instead of the character heuristic):

```bash
cargo install --git https://github.com/hansipie/ecotokens --features exact-tokens
```

## Installation

### Claude Code

```bash
cargo install --path .
ecotokens install
```

### Gemini CLI

Requires [Gemini CLI](https://github.com/google-gemini/gemini-cli) ≥ 0.1.0.

```bash
cargo install --path .
ecotokens install --target gemini
```

This writes a `BeforeTool` hook entry into `~/.gemini/settings.json`.

### Qwen Code

Requires [Qwen Code](https://github.com/QwenLM/qwen-code).

```bash
cargo install --path .
ecotokens install --target qwen
```

This writes a `PreToolUse` hook entry into `~/.qwen/settings.json`.

### All targets at once

```bash
ecotokens install --target all
```

`--target all` covers Claude Code, Gemini CLI, and Qwen Code in a single command.

### With exact token counting

By default, token counts use a fast character heuristic (`chars × 0.25`, ~80-85% accuracy). Enable exact counting via [tiktoken](https://github.com/openai/tiktoken) (cl100k_base encoding):

```bash
cargo install --path . --features exact-tokens
```

This has no effect on filtering behavior — only the token counts recorded in metrics are more precise.

### With AI summarization

Enable AI-powered output compression via Ollama at install time:

```bash
ecotokens install --ai-summary                          # use default model (llama3.2:3b)
ecotokens install --ai-summary-model qwen2.5:3b         # specify model (implies --ai-summary)
```

This writes `ai_summary_enabled` and `ai_summary_model` to `~/.config/ecotokens/config.json`. Ollama must be running and the model must be pulled (`ollama pull llama3.2:3b`).

### Uninstall

```bash
ecotokens uninstall                    # Claude Code
ecotokens uninstall --target gemini    # Gemini CLI
ecotokens uninstall --target qwen      # Qwen Code
ecotokens uninstall --target all       # all targets
```

## Commands

| Command | Description |
|---------|-------------|
| `ecotokens install` | Install the PreToolUse + PostToolUse hooks in `~/.claude/settings.json` |
| `ecotokens uninstall` | Remove the hooks |
| `ecotokens filter -- CMD [ARGS]` | Run a command, filter its output, record metrics |
| `ecotokens hook-post` | PostToolUse handler — intercept native tool results (Read, Grep, Glob) |
| `ecotokens gain` | Interactive TUI dashboard — savings by family or project |
| `ecotokens gain --period PERIOD` | Filter TUI to a time window (`all`, `today`, `week`, `month`) |
| `ecotokens gain --history` | Print a savings summary table for 24h / 7 days / 30 days |
| `ecotokens gain --json` | JSON report |
| `ecotokens config` | Show current configuration |
| `ecotokens index [--path DIR]` | Index a codebase for BM25 + symbolic search |
| `ecotokens search QUERY` | Search the indexed codebase |
| `ecotokens outline PATH` | List symbols in a file or directory |
| `ecotokens symbol ID` | Look up a symbol by its stable ID |
| `ecotokens trace callers SYMBOL` | Find callers of a symbol |
| `ecotokens trace callees SYMBOL` | Find callees of a symbol |
| `ecotokens watch [--path DIR]` | Watch a directory and keep the index up to date |
| `ecotokens auto-watch enable` | Start watch automatically on each Claude Code session |
| `ecotokens auto-watch disable` | Disable automatic watch |
| `ecotokens duplicates` | Detect near-duplicate code blocks in the indexed codebase |
| `ecotokens clear --all` | Delete all recorded interceptions |
| `ecotokens clear --before DATE` | Delete interceptions recorded before DATE (YYYY-MM-DD) |
| `ecotokens clear --older-than DURATION` | Delete interceptions older than a duration (e.g. `30d`, `2w`, `1m`) |
| `ecotokens clear --family FAMILY` | Delete interceptions of a specific command family |
| `ecotokens clear --project PATH` | Delete interceptions for a specific project (use `"(unknown)"` for entries without a git root) |

## Gain dashboard

```
ecotokens gain                                          # all time
ecotokens gain --period today                           # today only
ecotokens gain --period week                            # last 7 days
ecotokens gain --period month --model claude-sonnet-4-5 # last 30 days, custom model
ecotokens gain --history                                # summary table: 24h / 7d / 30d
ecotokens gain --history --json                         # same, as JSON
```

Interactive TUI showing token savings per command family and per project, with a sparkline. The `--period` flag filters both the stats and the history panels.

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

### Auto-watch *(Claude Code <del>& Qwen Code</del>)*

`ecotokens auto-watch` integrates with Claude Code <del>and Qwen Code</del>'s session lifecycle to start and stop the watcher automatically.

```bash
ecotokens auto-watch enable    # enable auto-watch, install SessionStart/SessionEnd hooks
ecotokens auto-watch disable   # disable (hooks remain installed but are no-ops)
```

When enabled, `ecotokens watch --background` starts automatically when a session opens, and stops when it closes. The setting is stored in `~/.config/ecotokens/config.json` (`auto_watch: true/false`).

> **Note:** Auto-watch relies on `SessionStart` / `SessionEnd` hooks. <del>For Qwen Code, session hooks are installed automatically if ecotokens is already installed for Qwen (`ecotokens install --target qwen`).</del> Gemini CLI does not expose session lifecycle hooks.

## Bonus Tools

_Less code is less tokens_

### Duplicates command

`ecotokens duplicates` scans the indexed codebase for near-identical code blocks and reports them grouped by similarity.

```bash
ecotokens duplicates                          # default: threshold=70%, min_lines=5
ecotokens duplicates --threshold 80           # only report ≥ 80% similarity
ecotokens duplicates --min-lines 10           # ignore blocks shorter than 10 lines
ecotokens duplicates --json                   # JSON output
```

Each group shows the file paths, line ranges, similarity score, and a refactoring proposal (exact duplicate, near duplicate, or subset).

## Configuration

```bash
ecotokens config           # show all settings (text)
ecotokens config --json    # show all settings (JSON)
```

Output includes:

```
hook_installed        : true
debug                 : false
exclusions            : []
embed_provider        : ollama (http://localhost:11434)
ai_summary_enabled    : false
ai_summary_model      : llama3.2:3b (default)
ai_summary_url        : http://localhost:11434 (default)
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
| `native_read` | Claude Code `Read` tool results (PostToolUse, outline-based compression) |

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

## Benchmarks

Measured over 13 days on a real developer workstation (4 129 hook executions):

| Metric | Value |
|--------|-------|
| Tokens saved | **6 714 085** |
| Overall reduction | **89.6 %** |
| Git commands | 96.6 % reduction |
| Cargo commands | 75.4 % reduction |
| Best single run | `git diff --staged` — 1.68M → 782 tokens (**99.97 %**) |

→ [Full benchmark report](docs/BENCHMARKS.md)

## Precision Guarantees

Filtering is aggressive on noise, conservative on signal:

- **Short outputs are never modified** — outputs under 200 lines or 50 KB pass through unchanged
- **Errors are always preserved** — `error[`, `FAILED`, `E   ` (pytest), `--- FAIL:` (Go), stack traces and panic messages are never removed
- **Failure sections are fully kept** — structured blocks (`=== FAILURES ===`, `failures:`, failure diffs) are always passed through in their entirety
- **Conservative fallback** — if a family filter doesn't improve the output (filtered ≥ original), the original is returned as-is
- **Secrets are redacted before filtering** — sensitive values are detected and replaced before any content reaches the model:

  | Pattern | Replaced by |
  |---|---|
  | AWS Access Key (`AKIA…`) | `[AWS_KEY]` |
  | GitHub PAT (`ghp_…`) | `[GITHUB_TOKEN]` |
  | Bearer token | `[BEARER_TOKEN]` |
  | PEM private key | `[PRIVATE_KEY]` |
  | `.env` secrets (`SECRET=`, `TOKEN=`, `API_KEY=`…) | `[REDACTED]` |
  | JWT token | `[JWT_TOKEN]` |
  | URL credentials (`user:pass@host`) | `[CREDENTIALS]` |
- **UTF-8 safe truncation** — truncation always happens at character boundaries, never mid-codepoint
- **Head + tail preservation** — when generic truncation applies, the first and last 20 lines are always kept (start context + end result)

## Requirements

- Rust ≥ 1.75 (stable)
- One or more of: Claude Code (with hook support), Gemini CLI ≥ 0.1.0, Qwen Code
- Ollama (optional, for semantic search embeddings and AI summarization)

## Contributing

Contributions are welcome! Please read the [contributing guidelines](docs/CONTRIBUTING.md) before submitting a pull request.

## License

MIT
