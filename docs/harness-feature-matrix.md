# Feature × Harness Matrix

> Last updated: 2026-06-04

## Main table

| Feature | Claude Code | Gemini CLI | Qwen Code | Pi | Hermes | Codex |
|---|:---:|:---:|:---:|:---:|:---:|:---:|
| **Pre-tool filtering (shell)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Post-tool filtering (native tools)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **MCP server** | ✓ | ✓ | ✓ | — | — | ✓ |
| **Auto-watch** | ✓ | — | ✓ | ✓ | ✓ | — |
| **Session hooks (start/end)** | ✓ | — | ✓ | — | ✓ | — |
| **Secret masking** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Metrics & Gain dashboard** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **AI summarization (optional)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Word abbreviations (optional)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Code intelligence (CLI)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

## Per-harness details

| Harness | Mechanism | Pre-tool hook | Post-tool hook | Intercepted native tools |
|---|---|---|---|---|
| **Claude Code** | JSON hooks `~/.claude/settings.json` | `PreToolUse` | `PostToolUse` | Read, Grep, Glob |
| **Gemini CLI** | JSON hooks `~/.gemini/settings.json` | `BeforeTool` | `AfterTool` | read_file, search_file_content, list_directory |
| **Qwen Code** | JSON hooks `~/.qwen/settings.json` | `PreToolUse` | `PostToolUse` | read_file, search_files, list_dir |
| **Pi** | TypeScript extension `~/.pi/agent/extensions/` | event `tool_call` | event `tool_result` | read, grep, find, ls |
| **Hermes** | Python plugin `~/.hermes/plugins/ecotokens/` | `transform_terminal_output` | `transform_tool_result` | all non-terminal tools |
| **Codex** | JSON plugin `~/.codex/plugins/ecotokens/` + MCP `~/.codex/config.toml` | — | — | — |

## Notes

- **Codex**: JSON plugin + MCP server registered in `~/.codex/config.toml` under `[mcp_servers.ecotokens]`. Session hooks are not yet supported (`install_codex_plugin` creates no hook files).
- **Gemini CLI**: no native session hooks, so no automatic auto-watch.
- **Pi**: no distinct session hooks; auto-watch is handled directly in the TypeScript extension.
- **MCP server**: registered automatically on install for Claude Code, Gemini CLI, Qwen Code, and Codex. Accessible via CLI for Pi and Hermes.
- **AI summarization** and **Word abbreviations** are opt-in features enabled via `ecotokens install --ai-summary` / `ecotokens abbreviations enable`, independently of the harness.
