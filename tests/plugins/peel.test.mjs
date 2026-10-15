// Wrapper peeling in the OpenCode plugin and the Pi extension, driven through
// their real entry points. Run: node --experimental-strip-types --test tests/plugins/
// (also run by `cargo test` via tests/hook/handler_test.rs).
import { test } from "node:test"
import assert from "node:assert/strict"
import { mkdtempSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"

// Both plugins disable themselves unless `ecotokens --version` succeeds.
const fakeBin = mkdtempSync(join(tmpdir(), "ecotokens-plugin-test-"))
writeFileSync(join(fakeBin, "ecotokens"), "#!/bin/sh\nexit 0\n", { mode: 0o755 })
process.env.PATH = `${fakeBin}:${process.env.PATH}`

const { EcotokensPlugin } = await import("../../src/opencode_plugin.ts")
const { default: piExtension } = await import("../../src/pi_extension.ts")

const opencodeHooks = await EcotokensPlugin({ directory: "/tmp" })
const piHandlers = {}
// Minimal ExtensionAPI: record `on` handlers, accept any other registration.
piExtension(
  new Proxy(
    { on: (name, fn) => (piHandlers[name] = fn) },
    { get: (api, key) => api[key] ?? (() => {}) },
  ),
)

/** Run `command` through each plugin's bash pre-execution hook. */
const plugins = {
  opencode: async (command) => {
    const output = { args: { command } }
    await opencodeHooks["tool.execute.before"]({ tool: "bash" }, output)
    return output.args.command
  },
  pi: async (command) => {
    const event = { toolName: "bash", input: { command } }
    await piHandlers.tool_call(event, { cwd: "/tmp" })
    return event.input.command
  },
}

const layers = (cmd) => cmd.split("ecotokens filter").length - 1

for (const [name, intercept] of Object.entries(plugins)) {
  test(`${name}: wrapper marker inside a comment is not peeled`, async () => {
    const out = await intercept(`ecotokens filter -- echo OK # -- bash -c 'echo SHOULD_NOT_RUN'`)
    assert.ok(out.includes("echo OK #"), out)
  })

  test(`${name}: commands before a wrapper marker are not dropped`, async () => {
    const out = await intercept(`ecotokens filter -- echo FIRST && echo SECOND -- bash -c 'echo ONLY_THIS'`)
    assert.ok(out.includes("echo FIRST && echo SECOND"), out)
  })

  test(`${name}: double-quoted backslash is kept literal`, async () => {
    const out = await intercept(String.raw`ecotokens filter --agent x -- bash -c "printf '%s' 'a\b'"`)
    assert.ok(out.includes(String.raw`a\b`), out)
  })

  test(`${name}: double-quoted \\n is not turned into a newline`, async () => {
    const out = await intercept(String.raw`ecotokens filter --agent x -- bash -c "printf '%s' 'x\ny'"`)
    assert.ok(out.includes(String.raw`x\ny`) && !out.includes("\n"), out)
  })

  test(`${name}: double-quoted substitution is kept whole`, async () => {
    const out = await intercept(`ecotokens filter --agent x -- bash -c "echo $(printf 'a;b')"`)
    assert.ok(out.includes(`"echo $(printf`), out)
  })

  test(`${name}: Rust hook's '"'"' escape is peeled and stays one layer`, async () => {
    // The Rust hook quotes apostrophes as '"'"'; re-intercepting its output
    // must heal to a single wrapper instead of growing a new layer each time.
    let cmd = `ecotokens filter --agent claude --cwd '/tmp' -- bash -c 'echo '"'"'hi'"'"' && ls'`
    for (let round = 1; round <= 3; round++) {
      cmd = await intercept(cmd)
      assert.equal(layers(cmd), 1, `round ${round}: ${cmd}`)
    }
    assert.equal(cmd, `ecotokens filter --agent ${name} --cwd '/tmp' -- bash -c 'echo '\\''hi'\\'' && ls'`)
  })

  test(`${name}: own wrapper is stable when re-intercepted`, async () => {
    const first = await intercept(`echo 'hi' && ls`)
    assert.equal(await intercept(first), first)
  })
}
