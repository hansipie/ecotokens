# Matrice features × harnesses

> Dernière mise à jour : 2026-06-04

## Tableau principal

| Feature | Claude Code | Gemini CLI | Qwen Code | Pi | Hermes | Codex |
|---|:---:|:---:|:---:|:---:|:---:|:---:|
| **Filtrage pre-tool (shell)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Filtrage post-tool (outils natifs)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **MCP server** | ✓ | ✓ | ✓ | — | — | ✓ |
| **Auto-watch** | ✓ | — | ✓ | ✓ | ✓ | — |
| **Session hooks (start/end)** | ✓ | — | ✓ | — | ✓ | — |
| **Masking de secrets** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Métriques & Gain dashboard** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **IA summarization (optionnel)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Word abbreviations (optionnel)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **Code intelligence (CLI)** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

## Détails par harness

| Harness | Mécanisme | Hook pre-tool | Hook post-tool | Outils natifs interceptés |
|---|---|---|---|---|
| **Claude Code** | Hooks JSON `~/.claude/settings.json` | `PreToolUse` | `PostToolUse` | Read, Grep, Glob |
| **Gemini CLI** | Hooks JSON `~/.gemini/settings.json` | `BeforeTool` | `AfterTool` | read_file, search_file_content, list_directory |
| **Qwen Code** | Hooks JSON `~/.qwen/settings.json` | `PreToolUse` | `PostToolUse` | read_file, search_files, list_dir |
| **Pi** | Extension TypeScript `~/.pi/agent/extensions/` | event `tool_call` | event `tool_result` | read, grep, find, ls |
| **Hermes** | Plugin Python `~/.hermes/plugins/ecotokens/` | `transform_terminal_output` | `transform_tool_result` | tous les outils non-terminal |
| **Codex** | Plugin JSON `~/.codex/plugins/ecotokens/` + MCP `~/.codex/config.toml` | — | — | — |

## Notes

- **Codex** : plugin JSON + MCP server enregistré dans `~/.codex/config.toml` sous `[mcp_servers.ecotokens]`. Les session hooks ne sont pas encore supportés (`install_codex_plugin` ne crée aucun fichier de hooks).
- **Gemini CLI** : pas de session hooks natifs, donc pas d'auto-watch automatique.
- **Pi** : pas de session hooks distincts, l'auto-watch est géré directement dans l'extension TypeScript.
- **MCP server** : enregistré automatiquement à l'installation pour Claude Code, Gemini CLI, Qwen Code et Codex. Accessible en CLI pour Pi et Hermes.
- **IA summarization** et **Word abbreviations** sont des options activables via `ecotokens install --ai-summary` / `ecotokens abbreviations enable`, indépendamment du harness.
