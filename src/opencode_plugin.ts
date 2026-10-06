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
 * Decode a single shell-quoted token: `'…'` with `'\''` escapes, or `"…"`
 * with backslash escapes (JSON-style `\"`, `\\`, `\n`, `\uXXXX` — the shape
 * produced by the old JSON.stringify-based wrapper and re-emitted by models
 * that copy wrapped commands from their transcript).
 * Returns null when `s` is not exactly one quoted token.
 */
function unquoteToken(s: string): string | null {
  const t = s.trim()
  if (t.startsWith("'")) {
    let out = ""
    let i = 1
    while (i < t.length) {
      if (t[i] === "'") {
        if (t.startsWith("'\\''", i)) {
          out += "'"
          i += 4
          continue
        }
        i++
        return t.slice(i).trim() === "" ? out : null
      }
      out += t[i]
      i++
    }
    return null
  }
  if (t.startsWith('"')) {
    let out = ""
    let i = 1
    while (i < t.length) {
      const c = t[i]
      if (c === "\\") {
        const n = t[i + 1]
        if (n === "n") out += "\n"
        else if (n === "t") out += "\t"
        else if (n === "r") out += "\r"
        else if (n === "u") {
          out += String.fromCharCode(parseInt(t.slice(i + 2, i + 6), 16))
          i += 6
          continue
        } else out += n ?? ""
        i += 2
        continue
      }
      if (c === '"') {
        i++
        return t.slice(i).trim() === "" ? out : null
      }
      out += c
      i++
    }
    return null
  }
  return null
}

/**
 * Peel `ecotokens filter … -- bash -c '<inner>'` layers until a non-ecotokens
 * shell command is reached. Returns null when `cmd` starts with `ecotokens`
 * but is not a peelable filter wrapper (`ecotokens gain`, a direct
 * `ecotokens filter …` invocation, or a corrupt chain) — such commands must be
 * left untouched instead of wrapped again.
 */
function peelEcotokensWrapper(cmd: string): string | null {
  let s = cmd.trim()
  while (s.startsWith("ecotokens")) {
    if (!s.startsWith("ecotokens filter")) return null
    const marker = " -- bash -c "
    const idx = s.indexOf(marker)
    if (idx === -1) return null
    const inner = unquoteToken(s.slice(idx + marker.length))
    if (inner === null) return null
    s = inner
  }
  return s
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
      // Re-entrancy guard + self-healing: models copy their already-wrapped
      // tool calls out of the transcript and wrap them again, growing an
      // exponentially-escaped chain. Peel every `ecotokens filter … -- bash -c`
      // layer back to the original command, then re-wrap exactly once.
      // Non-peeleable `ecotokens …` commands (gain, search, a direct filter
      // invocation) are left untouched.
      const raw = args.command.trim()
      let inner: string
      if (raw.startsWith("ecotokens")) {
        const peeled = peelEcotokensWrapper(raw)
        if (peeled === null) return
        inner = peeled
      } else {
        inner = raw
      }
      args.command = `ecotokens filter --agent opencode --cwd ${shQuote(directory)} -- bash -c ${shQuote(inner)}`
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
