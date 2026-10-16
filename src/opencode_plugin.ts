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

/** True for the characters bash splits words on; a newline ends the command. */
function isBlank(c: string | undefined): boolean {
  return c === " " || c === "\t"
}

/** Skip blanks in `s` from `i`. */
function skipBlank(s: string, i: number): number {
  while (i < s.length && isBlank(s[i])) i++
  return i
}

/**
 * Strip the blanks and newlines around `s`, the only characters bash ignores
 * there. Unlike `trim()`, a NBSP or CR is kept: bash glues it to the word next
 * to it, so dropping it would change what runs.
 */
function trimSeparators(s: string): string {
  return s.replace(/^[ \t\n]+|[ \t\n]+$/g, "")
}

/**
 * Parse an exact `bash -c '<inner>'` command starting at `s[i]` and return
 * `<inner>`: nothing but blanks and newlines may follow the quoted token
 * (`bash -c 'ls' extra` sets `$0`, `bash -c 'ls' && …` chains a command).
 */
function parseBashC(s: string, i: number): string | null {
  if (!s.startsWith("bash -c ", i)) return null
  const tok = parseQuoted(s, skipBlank(s, i + "bash -c ".length))
  return tok && trimSeparators(s.slice(tok[1])) === "" ? tok[0] : null
}

/**
 * Parse one `ecotokens filter [--agent A] [--cwd C] -- bash -c '<inner>'`
 * layer — exactly the shape the hook and the plugins emit — and return
 * `<inner>`. Anything else (other options, a marker inside a comment, commands
 * before the marker, text after the quoted token) is not a wrapper: null.
 * Neither is a layer clap would reject (a repeated option, an invalid value):
 * its inner command never runs.
 * Tokens are separated by blanks only: a newline between them would split the
 * original into several commands, so peeling it would change what runs.
 */
function parseWrapper(cmd: string): string | null {
  const s = trimSeparators(cmd)
  if (!s.startsWith("ecotokens filter ")) return null
  let i = "ecotokens filter ".length
  const seen = new Set<string>()
  for (;;) {
    i = skipBlank(s, i)
    if (s.startsWith("-- ", i)) return parseBashC(s, i + "-- ".length)
    const opt = ["--agent ", "--cwd "].find((o) => s.startsWith(o, i))
    if (!opt || seen.has(opt)) return null
    seen.add(opt)
    i = skipBlank(s, i + opt.length)
    // Option value: a quoted token or a bare word of characters bash passes
    // through as is (no glob, brace or tilde expansion), non-empty and not
    // starting with `-` (clap would read `--agent --help` as a missing value).
    let value: string
    if (s[i] === "'" || s[i] === '"') {
      const tok = parseQuoted(s, i)
      if (!tok) return null
      value = tok[0]
      i = tok[1]
    } else {
      value = /^[A-Za-z0-9_.\/:@%+=-]*/.exec(s.slice(i))![0]
      i += value.length
    }
    if (value === "" || value.startsWith("-")) return null
    // `'/tmp'--` is a single word to bash, not a value then `--`.
    if (!isBlank(s[i])) return null
  }
}

/** True when `s` is an `ecotokens` invocation (`ecotokens-foo` is not). */
function isEcotokensCmd(s: string): boolean {
  return s === "ecotokens" || /^ecotokens[ \t]/.test(s)
}

/** True when `s` has no shell operator, substitution or newline. */
function isSimpleCommand(s: string): boolean {
  return !/[;&|<>`$()\n\\]/.test(s)
}

/**
 * `ecotokens` subcommands that may run bare instead of under another
 * `ecotokens filter`: they mask what they print (`filter`, `filter-output`),
 * print output that was masked before it was stored (`show`) or print only
 * metrics (`gain`). Any other subcommand — `search` and `symbol` print indexed
 * source unmasked — is still wrapped so its output goes through masking.
 */
const BARE_SUBCOMMANDS = ["filter", "filter-output", "show", "gain"]

/** True when `s` is a simple `ecotokens` call to one of BARE_SUBCOMMANDS. */
function runsBare(s: string): boolean {
  return isEcotokensCmd(s) && isSimpleCommand(s) && BARE_SUBCOMMANDS.includes(s.split(/[ \t]+/)[1] ?? "")
}

/**
 * Peel `ecotokens filter … -- bash -c '<inner>'` layers (see parseWrapper) and
 * bare `bash -c '<inner>'` layers (see parseBashC, which the plugin emits for
 * a bare command) until a command that is neither is reached, and return it:
 * the original shell command, an `ecotokens` command that is not a wrapper
 * (`ecotokens gain`, `ecotokens filter --debug -- …`), or a wrapper-like
 * command that cannot be peeled without changing what it runs (a trailing
 * `&& …`, a `$(…)` in double quotes), which is then kept whole. A command
 * that is not a wrapper comes back unchanged. Leading blanks of each inner
 * command are dropped, as bash ignores them. The caller always runs the
 * result under a new `bash -c`, so dropping the peeled shells does not move
 * the command to the caller's shell.
 */
function peelOnce(s: string): string | null {
  return parseWrapper(s) ?? parseBashC(trimSeparators(s), 0)
}

function peelEcotokensWrapper(cmd: string): string {
  let s = cmd
  for (let inner = peelOnce(s); inner !== null; inner = peelOnce(s)) s = inner.replace(/^[ \t]+/, "")
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
      // A simple `ecotokens …` call that is safe bare (gain, show, a direct
      // filter invocation) is not wrapped; any other subcommand, anything
      // chained to it, or a layer that cannot be peeled safely, is wrapped
      // whole so its output still goes through masking.
      // Checks ignore the blanks and newlines around the command, but what
      // runs keeps every byte (a trailing `\` + newline is a continuation).
      const original = args.command
      const inner = peelEcotokensWrapper(original)
      // A call peeled out of wrappers keeps its inner shell: if run bare in the
      // caller's shell, its globs and aliases could expand differently.
      if (runsBare(trimSeparators(inner))) {
        if (inner !== original) args.command = `bash -c ${shQuote(inner)}`
        return
      }
      // `--cwd ''` (or `--cwd -x`) makes clap reject the wrapper and the peel
      // skip it, so a new layer would grow on every resubmission.
      const cwd = directory
      const cwdOpt = cwd && !cwd.startsWith("-") ? ` --cwd ${shQuote(cwd)}` : ""
      args.command = `ecotokens filter --agent opencode${cwdOpt} -- bash -c ${shQuote(inner)}`
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
