# MCP Server

The ecotokens MCP server exposes code-intelligence tools over the [Model Context Protocol](https://modelcontextprotocol.io/) (stdio transport). It is backed by the same tantivy index and trace engines used by the CLI commands.

## Starting the server

```bash
ecotokens mcp-server                                    # default index (~/.config/ecotokens/index)
ecotokens mcp-server --index-dir /path/to/index         # custom index directory
```

The server speaks JSON-RPC 2.0 over stdin/stdout (stdio transport) and stays alive until the parent process closes it.

## Auto-registration

`ecotokens install` registers the server automatically in each supported agent's settings file:

| Target | Settings file | Entry key |
|--------|--------------|-----------|
| Claude Code | `~/.claude/settings.json` | `mcpServers.ecotokens` |
| Gemini CLI | `~/.gemini/settings.json` | `mcpServers.ecotokens` |
| Qwen Code | `~/.qwen/settings.json` | `mcpServers.ecotokens` |

Registered entry (uses the absolute path of the installed binary):

```json
{
  "mcpServers": {
    "ecotokens": {
      "command": "/path/to/ecotokens",
      "args": ["mcp-server"]
    }
  }
}
```

`ecotokens uninstall` removes the entry idempotently.

## Project scoping

Search results are automatically filtered to the current git project. When the agent invokes `ecotokens_search`, the server resolves the git root of its working directory and retains only results whose file paths exist under that root. Files from other indexed projects are silently excluded.

## Tools

### `ecotokens_search`

BM25 + semantic search over the indexed codebase. Prefer this over `grep`/`find` for code exploration.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `query` | string | required | The search query |
| `top_k` | integer | `5` | Maximum number of results |

Returns a JSON array of results, each with `file_path`, `line_start`, `score`, and `snippet`.

---

### `ecotokens_outline`

Lists symbols (functions, structs, enums, traits, …) in a file or directory. Use before reading a file in full to understand its structure.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `path` | string | required | Path to a file or directory |
| `depth` | integer | `1` | Recursion depth for directories |
| `kinds` | string[] | all | Filter by symbol kind (`fn`, `struct`, `impl`, `enum`, …) |

Returns a JSON array of symbol descriptors with `id`, `name`, `kind`, `file`, and `line`.

---

### `ecotokens_symbol`

Retrieves the full source of a symbol by its stable ID. Use `ecotokens_outline` first to discover valid IDs.

IDs follow the form `path/to/file.rs::name#kind` — for example `src/main.rs::cmd_search#fn`.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `id` | string | required | Stable symbol ID |

Returns the raw source snippet of the symbol.

---

### `ecotokens_trace_callers`

Finds all call sites of a symbol — which functions call it, and where.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `symbol` | string | required | Symbol name to trace |

Returns a JSON array of `{ caller, file, line }` edges.

---

### `ecotokens_trace_callees`

Finds all symbols called by a given function, with optional recursive depth.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `symbol` | string | required | Symbol name to trace |
| `depth` | integer | `1` | Recursion depth |

Returns a JSON array of `{ callee, file, line }` edges.

---

### `ecotokens_duplicates`

Detects near-duplicate or structurally similar code blocks and returns refactoring proposals.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `threshold` | float | `70.0` | Minimum similarity percentage (0–100) |
| `min_lines` | integer | `5` | Minimum block size in lines |
| `top_k` | integer | `10` | Maximum number of duplicate groups to return |

Returns a plain-text report grouped by similarity, with file paths, line ranges, and a refactoring suggestion (exact duplicate, near duplicate, or subset).

## Recommended usage pattern

```
1. ecotokens_search "my query"          → find candidate locations
2. ecotokens_outline src/file.rs        → inspect structure without reading the full file
3. ecotokens_symbol src/file.rs::fn#fn  → fetch only the relevant symbol
4. ecotokens_trace_callers fn           → find all usage sites
5. ecotokens_trace_callees fn           → understand dependencies
```

This sequence replaces multiple `grep` + `Read` round-trips with targeted, token-efficient lookups.

## Index prerequisite

The server requires a pre-built index. Run `ecotokens index` (or enable `ecotokens auto-watch`) before using the MCP tools:

```bash
ecotokens index                          # index current directory
ecotokens index --path /path/to/project  # index a specific project
ecotokens auto-watch enable              # keep index up to date automatically
```

The default index location is `~/.config/ecotokens/index`.
