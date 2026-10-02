# Local models

[← Back to the README](../README.md)

Optional features that use a model running on your machine (Ollama): AI summarization and text rewrite.

## AI summarization (optional)

When enabled, large command outputs (> ~2500 tokens) are summarized by a local Ollama model instead of being truncated. Falls back to generic filtering if Ollama is unavailable or times out.

Enable via install:

```bash
ecotokens install --ai-summary-model llama3.2:3b
```

Or update the config file directly (`~/.config/ecotokens/config.json`):

```json
{
  "ai_summary_enabled": true,
  "ai_summary_model": "llama3.2:3b"
}
```

Ollama must be running locally. The model is called with a 3-second timeout to avoid blocking the model.

## Rewrite (local text transformation)

`ecotokens rewrite` paraphrases, retones, adjusts reading level, or translates prose using your
already-configured local model - entirely on your machine, with zero API cost. It's not a token
*saving* feature (it transforms text rather than compressing it, and is excluded from savings
reporting), but it reuses the same local-model plumbing as AI summarization.

```bash
echo "Please fix the login bug ASAP, it's blocking everyone." | ecotokens rewrite --mode tone --to formal
ecotokens rewrite --mode reading-level --to grade-8 --file notes.md
ecotokens rewrite --mode translate --to fr --file notes.md
echo "$TEXT" | ecotokens rewrite --mode paraphrase --json
```

| Mode | `--to` | Notes |
|------|--------|-------|
| `paraphrase` | forbidden | Rewords while preserving meaning |
| `tone` | required, free-form | e.g. `plain`, `formal`, `friendly` |
| `reading-level` | required, free-form | e.g. `grade-8`, `expert` |
| `translate` | required, language code/name | e.g. `fr`, `french`; a same-language target is a no-op |

Fenced code, inline code, URLs, emails, numbers, and dates in the input are preserved
byte-identically. Code-shaped or predominantly non-prose input is refused rather than transformed.
On any model failure (unreachable, timeout, empty/truncated/corrupted response), the command
**fails open**: it prints the original input unchanged and still exits `0`, so it's always safe to
drop into a pipeline. Long documents are split on structure-aware boundaries (paragraphs, then
sentences, then whitespace - never inside a fenced code block, table, or list) and reassembled;
if any chunk fails, the whole document falls back untouched rather than emitting a partial mix.

Available identically from the CLI, as the `ecotokens_rewrite` MCP tool (agents can call it
mid-session), and - opt-in only - as an automatic pipeline stage (see below).

### Diff audit trail (optional)

Every transformation can be saved as a masked unified diff, so changes stay reviewable:

```bash
echo "$TEXT" | ecotokens rewrite --mode paraphrase --save-diff   # force on for one run
echo "$TEXT" | ecotokens rewrite --mode paraphrase --no-save-diff  # force off, overrides config
```

Diffs go to `rewrite_diff_dir` (default: OS temp dir), are `0600`-permissioned, existing secret
patterns are masked on both sides before diffing, and old diffs are pruned to `rewrite_diff_retention`
(default 50, `0` disables pruning). Off by default. A write failure never blocks the transformed
output - it just warns on stderr.

### Automatic pipeline transformation (optional, off by default)

With `rewrite_auto_enabled = true`, qualifying prose flowing through the normal filter pipeline is
rewritten automatically before being handed to the agent - e.g. auto-translating command output.
This is the one rewrite setting that adds latency to every qualifying interception, so it's
deliberately off by default and gated behind several safeguards: only content classified as prose
is touched (code, stack traces, diffs, and structured data always pass through untouched), content
carrying a masked secret is never sent to the model, short content below `rewrite_auto_min_tokens`
is skipped, and the stage has its own stricter timeout (`rewrite_auto_timeout_ms`) separate from
`rewrite_timeout_ms`. Because rewriting can *expand* text where compression only shrinks it, any
resulting token cost is tracked separately and shown as `rewrite_overhead_tokens` in `ecotokens gain`
and `ecotokens gain --json` - never hidden inside the normal savings figures.

### Configuration

Update `~/.config/ecotokens/config.json` directly, or pass `--model`/`--save-diff`/`--no-save-diff`
per invocation:

```json
{
  "rewrite_model": "llama3.2:3b",
  "rewrite_url": "http://localhost:11434",
  "rewrite_timeout_ms": 30000,
  "rewrite_context_tokens": 8192,
  "rewrite_truncation_ratio": 0.5,
  "rewrite_save_diff": false,
  "rewrite_diff_dir": null,
  "rewrite_diff_retention": 50,
  "rewrite_auto_enabled": false,
  "rewrite_auto_mode": "translate",
  "rewrite_auto_target": "fr",
  "rewrite_auto_min_tokens": 500,
  "rewrite_auto_timeout_ms": 2000
}
```

`rewrite_model` falls back to `ai_summary_model`, then to `llama3.2:3b`, and `rewrite_url` must
resolve to localhost - the endpoint is validated before any request is issued, and text is never
transmitted anywhere else unless you opt in to [Jev judgments](jev-integration.md).
