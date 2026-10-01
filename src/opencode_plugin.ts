/**
 * ecotokens plugin for OpenCode — token savings via pre/post tool interception.
 * Install: ecotokens install --target opencode
 *
 * This file is the source of truth; `install` copies it verbatim to
 * ~/.config/opencode/plugins/ecotokens.ts. Do not edit the installed copy.
 * Restart OpenCode after installing ecotokens or this plugin — plugins are
 * loaded at startup only.
 *
 * OpenCode uses in-process JS/TS hooks (tool.execute.before / tool.execute.after).
 * Plugin docs: https://opencode.ai/docs/plugins
 */
import type { Plugin } from "@opencode-ai/plugin"
import { execSync, spawnSync } from "child_process"

// ─── helpers ────────────────────────────────────────────────────────────────

function ecotokensAvailable(): boolean {
  try {
    execSync("ecotokens --version", { stdio: "ignore" })
    return true
  } catch {
    return false
  }
}

/**
 * POSIX single-quote escaping: `'foo'` → `'foo'\''bar'`.
 * Unlike JSON.stringify (double quotes), the result is passed to the shell
 * verbatim — no $VAR, $(...), backtick, or ! expansion by the outer shell.
 */
function shQuote(s: string): string {
  return `'${s.replace(/'/g, `'\\''`)}'`
}

/**
 * Call ecotokens hook-post with the PostHookInput payload.
 * Returns filtered output or null if passthrough.
 *
 * Format expected by src/hook/post_handler.rs::PostHookInput:
 *   { tool_name, tool_input, tool_response: { output }, cwd }
 */
function callHookPost(
  toolName: string,
  toolInput: unknown,
  output: string,
  cwd: string,
): string | null {
  const payload = JSON.stringify({
    tool_name: toolName,
    tool_input: toolInput,
    tool_response: { output },
    cwd,
  })

  const result = spawnSync("ecotokens", ["hook-post", "--agent", "opencode"], {
    input: payload,
    encoding: "utf-8",
    timeout: 10_000,
  })

  if (result.status !== 0 || !result.stdout) return null

  try {
    const parsed = JSON.parse(result.stdout) as {
      hookSpecificOutput?: { additionalContext?: string }
    }
    return parsed?.hookSpecificOutput?.additionalContext ?? null
  } catch {
    return null
  }
}

// Mapping OpenCode tool names → ecotokens tool names for handle_post_input()
// src/hook/post_handler.rs routes on "Read" | "Grep" | "Glob"
const OPENCODE_TO_CLAUDE_TOOL: Record<string, string> = {
  read: "Read",
  grep: "Grep",
  glob: "Glob",
}

// Minimum output length to trigger filtering (avoids unnecessary spawns)
const MIN_OUTPUT_CHARS = 200

// ─── plugin ─────────────────────────────────────────────────────────────────

export const EcotokensPlugin: Plugin = async ({ directory }) => {
  if (!ecotokensAvailable()) {
    console.warn("[ecotokens] binary not found in PATH, plugin disabled")
    return {}
  }

  return {
    // ── 1. Bash pre-execution: PreToolUse rewrite ──────────────────────────
    //
    // Mutate output.args.command before execution.
    // ecotokens filter runs the command, filters stdout, and records metrics.
    "tool.execute.before": async (input, output) => {
      if (input.tool !== "bash") return
      const args = output.args as { command?: string }
      if (!args.command) return
      // Guard against double-wrapping (re-entrant calls or already-wrapped commands).
      if (args.command.startsWith("ecotokens")) return
      args.command = `ecotokens filter --agent opencode --cwd ${shQuote(directory)} -- bash -c ${shQuote(args.command)}`
    },

    // ── 2. Native tools post-execution: PostToolUse ───────────────────────
    //
    // Replace output.output with filtered version for Read/Grep/Glob.
    "tool.execute.after": async (input, output) => {
      const claudeName = OPENCODE_TO_CLAUDE_TOOL[input.tool]
      if (!claudeName) return

      const rawOutput = output.output
      if (typeof rawOutput !== "string" || rawOutput.length < MIN_OUTPUT_CHARS) return

      const filtered = callHookPost(claudeName, input.args, rawOutput, directory)
      if (!filtered || filtered === rawOutput) return

      output.output = filtered
    },
  }
}
