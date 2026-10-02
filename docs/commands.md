# Commands

[← Back to the README](../README.md)

Every `ecotokens` command, shell completions, the gain dashboard and the filter command.

## Commands

| Command | Description |
|---------|-------------|
| `ecotokens install` | Install the PreToolUse + PostToolUse hooks and register the MCP server entry in `~/.claude/settings.json` |
| `ecotokens install --target codex` | Install the Codex plugin, `PreToolUse`/`PostToolUse` hooks in `~/.codex/hooks.json`, and MCP server in `~/.codex/config.toml` |
| `ecotokens install --target hermes` | Install the Hermes plugin in `~/.hermes/plugins/` |
| `ecotokens install --target hermes --enable-plugin` | Install and add to `plugins.enabled` in `~/.hermes/config.yaml` directly |
| `ecotokens install --target opencode` | Install the OpenCode plugin in `~/.config/opencode/plugins/ecotokens.ts` |
| `ecotokens uninstall` | Remove all hooks (PreToolUse, PostToolUse, SessionStart, SessionEnd where supported) and the MCP server entry |
| `ecotokens filter -- CMD [ARGS]` | Run a command, filter its output, record metrics |
| `ecotokens filter --cwd DIR -- CMD [ARGS]` | Same, with an explicit working directory |
| `ecotokens filter-output --command LABEL --exit-code N` | Filter captured output read from stdin and record metrics (used by Hermes hooks) |
| `ecotokens filter-output ... --hook-type transform-tool-result` | Same, attributed to the `transform_tool_result` hook in metrics |
| `ecotokens hook-post` | PostToolUse handler - intercept native tool results (Read, Grep, Glob) |
| `ecotokens hook-prompt` | UserPromptSubmit handler - size the message and route it to a helper agent (used by the router hook, not run by hand) |
| `ecotokens gain` | Interactive TUI dashboard - savings by family or project |
| `ecotokens gain --period PERIOD` | Filter TUI to a time window (`all`, `today`, `week`, `month`) |
| `ecotokens gain --history` | Print a savings summary table for 24h / 7 days / 30 days |
| `ecotokens gain --json` | JSON report |
| `ecotokens jev [--period PERIOD] [--json]` | TUI with detailed Jev usage - calls, fallbacks, latency, tokens, cost, recent calls (also `v` in `gain`) |
| `ecotokens game [--period PERIOD]` | Space Invaders mini-game - filtered commands spawn enemies scaled by tokens saved |
| `ecotokens config [--debug true\|false]` | Show or update global configuration (including debug mode) |
| `ecotokens doctor [--json]` | Diagnose PATH, config, hook, MCP, and metrics setup without mutating files |
| `ecotokens gain price --input X --output Y` | Set the model price in USD per million tokens, used for the cost avoided figure of `gain` (cost stays `n/a` until set) |
| `ecotokens config --embed-provider candle\|ollama\|none` | Set the embedding backend (`candle` = local BERT, `ollama` = Ollama HTTP API, `none` = BM25 only) |
| `ecotokens config --embed-model MODEL` | Set the embedding model (e.g. `qwen3-embedding:latest` for Ollama, or a HuggingFace ID for Candle) |
| `ecotokens config --embed-url URL` | Set the Ollama base URL (default: `http://localhost:11434`) |
| `ecotokens config --jev true\|false` | Enable or disable TypeSafe Jev judgments (requires `TYPESAFE_API_KEY`) |
| `ecotokens config --jev-line-select true\|false` | Enable or disable Jev line selection in the generic filter |
| `ecotokens index [--path DIR]` | Index a codebase for BM25 + symbolic search |
| `ecotokens search QUERY [--context N] [--include GLOB] [--exclude GLOB] [--no-trace]` | Search the indexed codebase with line numbers, context, and optional trace augmentation |
| `ecotokens outline PATH` | List symbols in a file or directory |
| `ecotokens symbol ID` | Look up a symbol by its stable ID |
| `ecotokens trace callers SYMBOL` | Find callers of a symbol |
| `ecotokens trace callees SYMBOL` | Find callees of a symbol |
| `ecotokens watch [--path DIR]` | Watch a directory and keep the index up to date |
| `ecotokens mcp-server [--index-dir DIR]` | Start the stdio MCP server exposing search/outline/symbol/trace/duplicates tools |
| `ecotokens auto-watch enable` | Start watch automatically on each Claude Code, Codex, Hermes, Pi or Qwen Code session |
| `ecotokens auto-watch disable` | Disable automatic watch |
| `ecotokens abbreviations enable` | Replace common words with abbreviations in filtered outputs + inject a matching instruction at SessionStart |
| `ecotokens abbreviations disable` | Turn abbreviations off (default) |
| `ecotokens abbreviations list` | List the active dictionary (defaults merged with user overrides) |
| `ecotokens duplicates` | Detect near-duplicate code blocks in the indexed codebase |
| `ecotokens show ID` | Print the full (secret-masked) output behind a filtered result — see [Recovering the full output](../README.md#recovering-the-full-output) |
| `ecotokens clear --all` | Delete all recorded interceptions and saved raw outputs |
| `ecotokens clear --before DATE` | Delete interceptions recorded before DATE (YYYY-MM-DD) |
| `ecotokens clear --older-than DURATION` | Delete interceptions older than a duration (e.g. `30d`, `2w`, `1m`) |
| `ecotokens clear --family FAMILY` | Delete interceptions of a specific command family |
| `ecotokens clear --project PATH` | Delete interceptions for a specific project (use `"[undefined]"` for entries without a git root) |
| `ecotokens completions SHELL` | Generate a shell completion script (`bash`, `zsh`, `fish`, `powershell`, `elvish`) |
| `ecotokens rewrite --mode MODE [--to TARGET]` | Paraphrase, retone, adjust reading level, or translate text using a local model |
| `ecotokens router on [--timeout-ms N]` | Turn the model router on: `UserPromptSubmit` hook plus `router-*` helper agents in `~/.claude` (Claude Code only, needs `TYPESAFE_API_KEY`) |
| `ecotokens router off` | Turn the router off and remove its hook and helper agents |
| `ecotokens router status [--json]` | Messages per size and decision, Jev requests, tokens, latency and estimated cost |
| `ecotokens router try MESSAGE... [--json] [--timeout-ms N]` | Size sample messages with Jev (live, not recorded, works while the router is off) |
| `ecotokens router price --input USD --output USD` | Set the Jev price per million tokens used for cost estimates |

## Shell completions

```bash
# zsh
ecotokens completions zsh > ~/.zsh/completions/_ecotokens

# bash
ecotokens completions bash > ~/.local/share/bash-completion/completions/ecotokens

# fish
ecotokens completions fish > ~/.config/fish/completions/ecotokens.fish

# PowerShell
ecotokens completions powershell >> $PROFILE
```

`ecotokens install` installs or updates the completion script for your current shell automatically (user-level XDG paths; for zsh, add `~/.local/share/zsh/site-functions` to `fpath`), and `ecotokens uninstall` removes it. Only bash, zsh and fish are installed automatically; use `ecotokens completions elvish|powershell` for the others. Enumerated values such as `rewrite --mode` and `config --embed-provider` are completed too.

The scripts are written to these locations:

| Shell | Path |
|-------|------|
| bash | `$XDG_DATA_HOME/bash-completion/completions/ecotokens` |
| zsh | `$XDG_DATA_HOME/zsh/site-functions/_ecotokens` |
| fish | `$XDG_CONFIG_HOME/fish/completions/ecotokens.fish` |

`$XDG_DATA_HOME` defaults to `~/.local/share` and `$XDG_CONFIG_HOME` to `~/.config`. Re-running `install` is idempotent, and `uninstall` removes the script for every supported shell. If the script cannot be written, `install` prints a warning and carries on: the failure does not abort the installation.

Reload your shell (or open a new terminal) to activate completions.

## Gain dashboard

```
ecotokens gain                                          # all time
ecotokens gain --period today                           # today only
ecotokens gain --period week                            # last 7 days
ecotokens gain --period month                           # last 30 days
ecotokens gain --history                                # summary table: 24h / 7d / 30d
ecotokens gain --history --json                         # same, as JSON
```

Cost avoided is computed from the input price you set with `ecotokens gain price --input <USD per 1M tokens> --output <USD per 1M tokens>`. Without a price it shows `n/a` (`null` in `--json`). Running `ecotokens gain price` with no flag shows the current values.

Interactive TUI showing token savings per command family and per project, with a sparkline. The `--period` flag filters both the stats and the history panels. All terminal views (gain, jev, game, outline, trace, watch) are described in [`docs/TUI.md`](TUI.md).

Keybindings for every view are listed in [TUI.md](TUI.md#gain-dashboard).

## Filter command

`ecotokens filter` runs a command directly and returns its filtered output. Useful for testing filters or wrapping commands in scripts:

```bash
ecotokens filter -- cargo test
ecotokens filter --debug -- git log --oneline -50
ecotokens filter --cwd /path/to/project -- cargo test
```

The output is compressed by the same family-specific filters used by the hook, and token savings are recorded in the metrics store.

## Game command

`ecotokens game` turns your token savings into a Space Invaders mini-game. Every filtered command spawns an enemy whose strength scales with the tokens it saved.

```bash
ecotokens game                    # spawn enemies from all recorded commands
ecotokens game --period today     # only spawn enemies from today's commands
```

Commands present at startup form the classic marching formation; commands filtered live while the game is running spawn as free-roaming "snakes". Arrow keys move, space fires.
