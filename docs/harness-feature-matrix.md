# Feature × Harness Matrix

> Last updated: 2026-09-24

## Main table

| Feature | Claude Code | Codex | Hermes | Pi | Gemini CLI | Qwen Code |
|---|:---:|:---:|:---:|:---:|:---:|:---:|
| **Pre-tool filtering (shell)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Post-tool filtering (native tools)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **MCP server** | ✓ | ✓ | — | — | ✓ | ✓ |
| **Auto-watch** | ✓ | — | ✓ | ✓ | — | ✓ |
| **Session hooks (start/end)** | ✓ | — | ✓ | — | — | ✓ |
| **Secret masking** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Metrics & Gain dashboard** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **AI summarization (optional)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Word abbreviations (optional)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Code intelligence (CLI)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Model router (optional)** | ✓ | — | — | — | — | — |
| **Local-LLM rewrite (optional)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Jev judgments (optional)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

## Per-harness details

| Harness | Mechanism | Pre-tool hook | Post-tool hook | Intercepted native tools |
|---|---|---|---|---|
| **Claude Code** | JSON hooks `~/.claude/settings.json` | `PreToolUse` | `PostToolUse` (+ `UserPromptSubmit` for the router) | Read, Grep, Glob |
| **Codex** | JSON plugin `~/.codex/plugins/ecotokens/` + MCP `~/.codex/config.toml` | — | — | — |
| **Hermes** | Python plugin `~/.hermes/plugins/ecotokens/` | `transform_terminal_output` | `transform_tool_result` | all non-terminal tools |
| **Pi** | TypeScript extension `~/.pi/agent/extensions/` | event `tool_call` | event `tool_result` | read, grep, find, ls |
| **Gemini CLI** | JSON hooks `~/.gemini/settings.json` | `BeforeTool` | `AfterTool` | read_file, search_file_content, list_directory |
| **Qwen Code** | JSON hooks `~/.qwen/settings.json` | `PreToolUse` | `PostToolUse` | read_file, search_files, list_dir |

## Notes

- **Harness priority**: Claude Code, Codex, Hermes, Pi, then the others (Gemini CLI, Qwen Code). Columns and rows follow this order.
- **Codex**: JSON plugin + MCP server registered in `~/.codex/config.toml` under `[mcp_servers.ecotokens]`. Session hooks are not yet supported (`install_codex_plugin` creates no hook files).
- **Pi**: no distinct session hooks; auto-watch is handled directly in the TypeScript extension.
- **Gemini CLI**: no native session hooks, so no automatic auto-watch.
- **MCP server**: registered automatically on install for Claude Code, Codex, Gemini CLI, and Qwen Code. Accessible via CLI for Hermes and Pi.
- **AI summarization** and **Word abbreviations** are opt-in features enabled via `ecotokens install --ai-summary` / `ecotokens abbreviations enable`, independently of the harness.
- **Model router**: Claude Code only. It relies on the `UserPromptSubmit` hook (`ecotokens hook-prompt`) and on helper agents written to `~/.claude/agents/`. Off by default; enable with `ecotokens router on`.
- **Local-LLM rewrite** (`ecotokens rewrite`, MCP tool `ecotokens_rewrite`) and **Jev judgments** (`ecotokens config --jev true`) are opt-in and harness-independent. Rewrite is reachable through the CLI everywhere and through MCP where the MCP server is registered. Every Jev use falls back to the existing heuristic.
