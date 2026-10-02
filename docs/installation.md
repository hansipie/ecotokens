# Installation

[← Back to the README](../README.md)

Per-agent setup, build from source, and uninstall. For the two-minute version, see the README.

## Per-agent setup

> Ready-to-copy configuration snippets for all supported integrations: [`docs/example-configs.md`](example-configs.md).

### Claude Code

```bash
cargo install --path .
ecotokens install
```

Install commands print a section per target, followed by the entries that were written:

```text
Install Claude Code
  ok      hook                   ~/.claude/settings.json
  ok      post-hook              ~/.claude/settings.json
  ok      MCP server             ~/.claude/settings.json
```

In addition to hook installation, this also registers an MCP server entry in `~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "ecotokens": {
      "command": "ecotokens",
      "args": ["mcp-server"]
    }
  }
}
```

### Codex

```bash
cargo install --path .
ecotokens install --target codex
```

This installs three things:

- **Plugin** (`~/.codex/plugins/ecotokens/.codex-plugin/plugin.json`) — identifies ecotokens to Codex
- **Hooks** (`~/.codex/hooks.json`) — `PreToolUse` (bash pre-exec filtering) and `PostToolUse` (bash post-exec filtering)
- **MCP server** (`~/.codex/config.toml`) — registers `ecotokens mcp-server` under `[mcp_servers.ecotokens]`

### Hermes

Requires [Hermes](https://hermes.dev).

```bash
cargo install --path .
ecotokens install --target hermes
```

This installs an ecotokens plugin in `~/.hermes/plugins/`. The plugin intercepts two Hermes hooks:

- `transform_terminal_output` — filters raw terminal output before Hermes truncates it
- `transform_tool_result` — filters non-terminal tool results (`read_file`, `search_files`, `browser_snapshot`, MCP tools, etc.)

Each hook calls `ecotokens filter-output` as a subprocess with the appropriate `--hook-type`, so savings are tracked separately in `ecotokens gain`.

**Enabling the plugin** — Hermes requires explicit activation. Two options:

```bash
# Option A: via Hermes CLI (requires hermes to be installed and in PATH)
hermes plugins enable ecotokens

# Option B: write directly to ~/.hermes/config.yaml (no hermes CLI needed)
ecotokens install --target hermes --enable-plugin
```

`--enable-plugin` adds `ecotokens` to `plugins.enabled` in `~/.hermes/config.yaml`, creating the file if it does not exist and preserving all existing keys. Restart Hermes after enabling.

**Per-tool filtering** — tool result labels (`hermes-tool:<name>`) are mapped to the appropriate filter family automatically:

| Tool | Family | Filter applied |
|------|--------|---------------|
| `read_file`, `list_directory`, `create_file` | `fs` | file listing compaction |
| `search_files`, `find_files`, `search_in_file` | `grep` | match deduplication |
| `browser_snapshot`, `web_fetch`, `web_search` | `network` | HTML/JSON reduction |
| `run_python_code`, `execute_python` | `python` | traceback extraction |
| `delegate_task`, MCP tools, others | `generic` | 200-line / 50 KB cap |

**Runtime variables** — the generated plugin reads these from the environment:

| Variable | Default | Effect |
|----------|---------|--------|
| `ECOTOKENS_BIN` | path at install time | Override the ecotokens binary called by the plugin |
| `ECOTOKENS_HERMES_MIN_CHARS` | `2000` | Skip filtering outputs shorter than this |
| `ECOTOKENS_HERMES_TIMEOUT` | `10` | Subprocess timeout in seconds |

The plugin is fail-open: any error, timeout, or empty output returns the original content unchanged.

### Pi

Requires [Pi](https://pi.dev) (`@mariozechner/pi-coding-agent` ≥ 0.62.0).

```bash
cargo install --path .
ecotokens install --target pi
```

This writes a TypeScript extension to `~/.pi/agent/extensions/ecotokens.ts`. Pi auto-discovers it on next startup (or `/reload` inside an active session). The extension intercepts bash commands before execution and filters native tool results (`read`, `grep`, `find`, `ls`) after execution.

### OpenCode

Requires [OpenCode](https://opencode.ai).

```bash
cargo install --path .
ecotokens install --target opencode
```

This writes a JS/TS plugin to `~/.config/opencode/plugins/ecotokens.ts`. OpenCode auto-discovers it on next startup. The plugin intercepts `tool.execute.before` (bash command rewrite) and `tool.execute.after` (read/grep/glob output filtering) events in-process. Works with OpenCode in terminal, desktop, and ACP mode (Zed, JetBrains, etc.).

### Gemini CLI

Requires [Gemini CLI](https://github.com/google-gemini/gemini-cli) ≥ 0.1.0.

```bash
cargo install --path .
ecotokens install --target gemini
```

This writes `BeforeTool` and `AfterTool` hook entries into `~/.gemini/settings.json`. The `AfterTool` hook intercepts `read_file`, `search_file_content`, and `list_directory` results.

It also registers the ecotokens MCP server in `~/.gemini/settings.json`.

### Qwen Code

Requires [Qwen Code](https://github.com/QwenLM/qwen-code).

```bash
cargo install --path .
ecotokens install --target qwen
```

This writes `PreToolUse` and `PostToolUse` hook entries into `~/.qwen/settings.json`. The `PostToolUse` hook intercepts `read_file`, `search_files`, and `list_dir` results.

It also registers the ecotokens MCP server in `~/.qwen/settings.json`.

### All targets at once

```bash
ecotokens install --target all
```

`--target all` covers Claude Code, Codex, Hermes, Pi, Gemini CLI, Qwen Code, and OpenCode in a single command.

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
ecotokens uninstall --target codex     # Codex
ecotokens uninstall --target hermes    # Hermes
ecotokens uninstall --target pi        # Pi
ecotokens uninstall --target gemini    # Gemini CLI
ecotokens uninstall --target qwen      # Qwen Code
ecotokens uninstall --target opencode  # OpenCode
ecotokens uninstall --target all       # all targets
```

Uninstall commands use the same grouped output and only list entries that were present:

```text
Uninstall Claude Code
  removed hook                   ~/.claude/settings.json
  removed post-hook              ~/.claude/settings.json
  removed MCP server             ~/.claude/settings.json
```

If nothing is installed for the selected target, the command prints:

```text
Uninstall Claude Code
  note    nothing to uninstall
```

## How each agent is wired

Claude Code uses the `PreToolUse` + `PostToolUse` hooks (`~/.claude/settings.json`). Codex uses `PreToolUse` + `PostToolUse` hooks in `~/.codex/hooks.json` and registers the MCP server in `~/.codex/config.toml`. Hermes uses a plugin that sends outputs through `filter-output` via `HermesTransformTerminalOutput` and `HermesTransformToolResult` hook types. Pi uses a TypeScript extension (`~/.pi/agent/extensions/ecotokens.ts`) that intercepts `tool_call` (bash pre-exec) and `tool_result` (read/grep/find/ls post-exec) events in-process. Gemini CLI uses the `BeforeTool` + `AfterTool` hooks (`~/.gemini/settings.json`). Qwen Code uses the `PreToolUse` + `PostToolUse` hooks (`~/.qwen/settings.json`). OpenCode uses a JS/TS plugin (`~/.config/opencode/plugins/ecotokens.ts`) that intercepts `tool.execute.before` (bash) and `tool.execute.after` (read/grep/glob) events in-process.

## Build from source

```bash
git clone https://github.com/hansipie/ecotokens.git
cd ecotokens
cargo build --release
./target/release/ecotokens --help
```

To install the locally built binary into Cargo's bin directory:

```bash
cargo install --path .
```

With exact token counting enabled via [tiktoken](https://github.com/openai/tiktoken) (cl100k_base encoding):

```bash
cargo install --path . --features exact-tokens
```
By default, token counts use a fast character heuristic (`chars × 0.25`, ~80-85% accuracy). This has no effect on filtering behavior - only the token counts recorded in metrics are more precise.
