# Example configurations

Ready-to-copy configuration snippets for every supported ecotokens integration.

All snippets use `ecotokens` as the binary name. Replace it with the absolute path to the binary if it is not on your `PATH`.

> Run `ecotokens install [--target <target>]` to have ecotokens write these entries automatically and idempotently.

## Claude Code

**File:** `~/.claude/settings.json`

### Hooks (PreToolUse + PostToolUse)

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [{ "type": "command", "command": "ecotokens hook" }]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Read|Grep|Glob",
        "hooks": [{ "type": "command", "command": "ecotokens hook-post" }]
      }
    ]
  }
}
```

### MCP server

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

### Auto-watch (optional)

`SessionStart` and `SessionEnd` hooks start and stop `ecotokens watch` automatically with each session. Enabled via `ecotokens auto-watch enable`.

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "",
        "hooks": [{ "type": "command", "command": "ecotokens session-start" }]
      }
    ],
    "SessionEnd": [
      {
        "matcher": "",
        "hooks": [{ "type": "command", "command": "ecotokens session-end" }]
      }
    ]
  }
}
```

---

## Gemini CLI

**File:** `~/.gemini/settings.json`

Requires Gemini CLI ≥ 0.1.0.

### Hooks (BeforeTool + AfterTool)

```json
{
  "hooks": {
    "BeforeTool": [
      {
        "matcher": "run_shell_command",
        "hooks": [{ "type": "command", "command": "ecotokens hook-gemini" }]
      }
    ],
    "AfterTool": [
      {
        "matcher": "read_file|search_file_content|list_directory",
        "hooks": [{ "type": "command", "command": "ecotokens hook-post-gemini" }]
      }
    ]
  }
}
```

### MCP server

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

---

## Qwen Code

**File:** `~/.qwen/settings.json`

### Hooks (PreToolUse + PostToolUse)

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "run_shell_command",
        "hooks": [{ "type": "command", "command": "ecotokens hook-qwen" }]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "read_file|search_files|list_dir",
        "hooks": [{ "type": "command", "command": "ecotokens hook-post-qwen" }]
      }
    ]
  }
}
```

### MCP server

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

---

## Pi

Pi uses a TypeScript extension instead of a JSON config file. `ecotokens install --target pi` writes the extension to `~/.pi/agent/extensions/ecotokens.ts`. Pi auto-discovers any `.ts` file in that directory on startup, or immediately on `/reload` inside an active session.

No manual editing is required. To verify the extension is in place:

```bash
ls ~/.pi/agent/extensions/ecotokens.ts
```

---

## Hermes

Hermes uses a Python plugin installed under `~/.hermes/plugins/ecotokens/`.

`ecotokens install --target hermes` writes two files:

- `~/.hermes/plugins/ecotokens/plugin.yaml` — plugin manifest
- `~/.hermes/plugins/ecotokens/__init__.py` — hook implementations

### Activating the plugin

Add `ecotokens` to `plugins.enabled` in `~/.hermes/config.yaml`:

```yaml
plugins:
  enabled:
    - ecotokens
```

Or let ecotokens write this entry automatically:

```bash
ecotokens install --target hermes --enable-plugin
```

### Plugin manifest (`~/.hermes/plugins/ecotokens/plugin.yaml`)

```yaml
name: ecotokens
version: "0.1.0"
description: "Compress Hermes Agent tool outputs with ecotokens before they enter model context"
author: "ecotokens"
kind: standalone
provides_hooks:
  - transform_terminal_output
  - transform_tool_result
  - on_session_start
  - on_session_end
```

---

## Codex

Three files are installed by `ecotokens install --target codex`.

### Hooks (`~/.codex/hooks.json`)

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [{ "type": "command", "command": "ecotokens hook-codex" }]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Bash",
        "hooks": [{ "type": "command", "command": "ecotokens hook-post-codex" }]
      }
    ]
  }
}
```

> **Note:** the plugin manifest below describes a `SessionStart` hook, but ecotokens does **not** install one for Codex yet — only the pre/post `Bash` hooks shown above are installed. Codex does expose a [`SessionStart` hook](https://developers.openai.com/codex/hooks), but it has no `SessionEnd` equivalent, so auto-watch (which needs both a start and a stop event) is not wired up for Codex.

### MCP server (`~/.codex/config.toml`)

```toml
[mcp_servers.ecotokens]
command = "ecotokens"
args = ["mcp-server"]
```

### Plugin manifest (`~/.codex/plugins/ecotokens/.codex-plugin/plugin.json`)

```json
{
  "name": "ecotokens",
  "version": "0.1.0",
  "description": "Keep the ecotokens index warm during Codex sessions.",
  "author": { "name": "ecotokens" },
  "license": "MIT",
  "keywords": ["codex", "watch", "index"],
  "interface": {
    "displayName": "ecotokens",
    "shortDescription": "Starts ecotokens watch with Codex sessions",
    "longDescription": "Installs a Codex SessionStart hook that calls ecotokens session-start so auto-watch can keep the project index up to date.",
    "developerName": "ecotokens",
    "category": "Developer Tools",
    "capabilities": ["Read"],
    "defaultPrompt": []
  }
}
```
