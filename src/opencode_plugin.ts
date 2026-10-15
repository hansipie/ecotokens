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
 * Decode the shell-quoted token starting at `s[i]`; returns
 * `[decoded, indexAfter]`. Accepted forms: `'…'` with `'\''` or `'"'"'`
 * escapes (the latter is what the Rust hook emits), or `"…"` decoded with
 * bash double-quote rules (`\` escapes only `$`, `` ` ``, `"`, `\` and
 * newline, and stays literal before anything else).
 * Returns null for anything else, including a double-quoted token with an
 * unescaped `$` or `` ` ``: the outer shell expands those before `bash -c`
 * runs, so moving them to an inner shell would change what runs.
 */
function parseQuoted(s: string, i: number): [string, number] | null {
  const quote = s[i++]
  let out = ""
  if (quote === "'") {
    while (i < s.length) {
      if (s[i] !== "'") {
        out += s[i++]
        continue
      }
      const esc = [`'"'"'`, `'\\''`].find((e) => s.startsWith(e, i))
      if (!esc) return [out, i + 1]
      out += "'"
      i += esc.length
    }
    return null
  }
  if (quote === '"') {
    while (i < s.length) {
      const c = s[i]
      if (c === '"') return [out, i + 1]
      if (c === "$" || c === "`") return null
      if (c === "\\" && i + 1 < s.length && "$`\"\\\n".includes(s[i + 1])) {
        if (s[i + 1] !== "\n") out += s[i + 1]
        i += 2
        continue
      }
      out += c
      i++
    }
    return null
  }
  return null
}

/** Skip whitespace in `s` from `i`. */
function skipSpace(s: string, i: number): number {
  while (i < s.length && /\s/.test(s[i])) i++
  return i
}

/**
 * Parse one `ecotokens filter [--agent A] [--cwd C] -- bash -c '<inner>'`
 * layer — exactly the shape the hook and the plugins emit — and return
 * `<inner>`. Anything else (other options, a marker inside a comment, commands
 * before the marker, text after the quoted token) is not a wrapper: null.
 */
function parseWrapper(cmd: string): string | null {
  const s = cmd.trim()
  if (!s.startsWith("ecotokens filter ")) return null
  let i = "ecotokens filter ".length
  for (;;) {
    i = skipSpace(s, i)
    if (s.startsWith("-- bash -c ", i)) {
      const tok = parseQuoted(s, skipSpace(s, i + "-- bash -c ".length))
      return tok && s.slice(tok[1]).trim() === "" ? tok[0] : null
    }
    const opt = ["--agent ", "--cwd "].find((o) => s.startsWith(o, i))
    if (!opt) return null
    i = skipSpace(s, i + opt.length)
    // Option value: a quoted token or a bare word free of shell metacharacters.
    if (s[i] === "'" || s[i] === '"') {
      const tok = parseQuoted(s, i)
      if (!tok) return null
      i = tok[1]
    } else {
      const word = /^[^\s;&|<>`$()\\'"#]+/.exec(s.slice(i))
      if (!word) return null
      i += word[0].length
    }
  }
}

/** True when `s` is an `ecotokens` invocation (`ecotokens-foo` is not). */
function isEcotokensCmd(s: string): boolean {
  return s === "ecotokens" || /^ecotokens\s/.test(s)
}

/** True when `s` has no shell operator, substitution or newline. */
function isSimpleCommand(s: string): boolean {
  return !/[;&|<>`$()\n\\]/.test(s)
}

/**
 * Peel `ecotokens filter … -- bash -c '<inner>'` layers (see parseWrapper)
 * until a command that is not a strict wrapper is reached, and return it: the
 * original shell command, an `ecotokens` command that is not a wrapper
 * (`ecotokens gain`, `ecotokens filter --debug -- …`), or a wrapper-like
 * command that cannot be peeled without changing what it runs (a trailing
 * `&& …`, a `$(…)` in double quotes), which is then kept whole.
 */
function peelEcotokensWrapper(cmd: string): string {
  let s = cmd.trim()
  for (let inner = parseWrapper(s); inner !== null; inner = parseWrapper(s)) s = inner
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
      // A simple `ecotokens …` CLI call (gain, search, a direct filter
      // invocation) is not wrapped; anything chained to it, or a layer that
      // cannot be peeled safely, is wrapped whole so its output still goes
      // through masking.
      const raw = args.command.trim()
      const inner = isEcotokensCmd(raw) ? peelEcotokensWrapper(raw) : raw
      if (isEcotokensCmd(inner) && isSimpleCommand(inner)) {
        if (inner !== raw) args.command = inner
        return
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
