# Configuration

[← Back to the README](../README.md)

Settings live in `~/.config/ecotokens/config.json`. Show them with `ecotokens config`.

```bash
ecotokens config           # show all settings (text)
ecotokens config --json    # show all settings (JSON)
```

Output includes:

```bash
hook_installed        : true
debug                 : false
debuglog              : false
price_input_usd_per_mtok  : unset
price_output_usd_per_mtok : unset
exclusions            : []
embed_provider        : candle model=sentence-transformers/all-MiniLM-L6-v2
ai_summary_enabled    : false
ai_summary_model      : llama3.2:3b (default)
ai_summary_url        : http://localhost:11434 (default)
abbreviations_enabled : false
```

### Debug mode

Enable the global debug mode to see detailed interception logs and enable background logging for the `watch` command:

```bash
ecotokens config --debug true
ecotokens config --debug false
```

This updates the `debug` field in `~/.config/ecotokens/config.json`.

### Debug file logging

Enable structured per-hook logging to a file for deeper tracing of what ecotokens intercepts:

```bash
ecotokens config --debuglog true
ecotokens config --debuglog false
```

When enabled, every hook invocation appends a JSONL entry to `~/.config/ecotokens/debug.log`:

```json
{"ts":"2026-05-08T12:00:00Z","uid":"a1b2c3d4","cmd":"git status","phase":"input","data":{...}}
{"ts":"2026-05-08T12:00:00Z","uid":"a1b2c3d4","cmd":"git status","phase":"output","data":{...}}
```

Each entry contains a short `uid` to correlate the input and output phases of the same invocation. Distinct from `--debug` (which prints to stderr) - `--debuglog` writes silently to disk and survives across sessions.

Logged payloads go through the same secret masking as intercepted output, and the file is created `0600` (owner-only). It still contains file contents and command output, so review it before attaching it to a bug report.

### Price for cost calculations

`gain` has no built-in price list. Enter the price of the model you use, in USD per million tokens:

```bash
ecotokens gain price --input 3 --output 15   # set both
ecotokens gain price --input 2.5             # update only the input price
ecotokens gain price                         # show the current prices
```

The saved-token cost uses the input price. Values must be finite and non-negative. This is separate from `ecotokens router price`, which sets the Jev price.

## Word abbreviations *(optional)*

```bash
ecotokens abbreviations enable    # transform narrative text + inject model instruction
ecotokens abbreviations list      # show the active dictionary
ecotokens abbreviations disable   # back to default
```

When enabled, a post-processing pass replaces full words with shorter forms in the narrative parts of tool outputs (code blocks between triple backticks are preserved). A matching `additionalContext` payload is emitted at `SessionStart` so the model adopts the same abbreviations in its own responses.

Extend or override the built-in dictionary via a separate `~/.config/ecotokens/abbreviations.json` file:

```json
{
  "function": "func",
  "repository": "repo"
}
```

The feature flag stays in `~/.config/ecotokens/config.json`:

```json
{
  "abbreviations_enabled": true
}
```

The full list of built-in abbreviations is in [abbreviations.md](abbreviations.md).
