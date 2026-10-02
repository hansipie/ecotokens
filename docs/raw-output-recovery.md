# Raw output recovery

ecotokens filters command output to save tokens, and it keeps errors and important lines. When you still need the
whole output, you can get it back.

## How it works

When filtering shrinks an output noticeably, ecotokens saves the full output and appends one line to the filtered text:

```text
[ecotokens] Full output saved: ecotokens show 3fa9c1d2
```

Print the saved output with:

```bash
ecotokens show 3fa9c1d2
```

The id is 8 lowercase hex characters. `show` exits with status 1 and a message on stderr when the id is unknown, expired
or malformed.

The hint is added only when filtering saves at least **20%** of the tokens. Short outputs and outputs that were not
reduced are left untouched, so the hint never costs tokens where there is nothing to recover.

It applies wherever the shared filter pipeline runs: `ecotokens filter`, `ecotokens filter-output`, and the Codex
post-tool hook.

## What is stored

Only the **secret-masked** text is written to disk. The original, unmasked output never reaches the store, so `show`
cannot reintroduce a secret that filtering hid: a masked token stays `[REDACTED]`. The trade-off is that the recovered
text is faithful to what the model could have seen, not byte-for-byte identical to the raw output when it contained
secrets. See [secret-patterns.md](secret-patterns.md) for what is masked.

| Property | Value |
|----------|-------|
| Location | `<config dir>/ecotokens/raw/<id>.txt` (`~/.config/ecotokens/raw/` on Linux) |
| Permissions | `0600` on Unix |
| Content | Secret-masked full output |
| Size cap | 2 MiB per entry; longer output is cut on a character boundary and ends with `…[truncated at 2097152 bytes]` |

## Retention

Each time a new output is saved, the store is pruned:

- entries older than `raw_recovery_retention_days` (default **7**) are deleted;
- beyond `raw_recovery_max_entries` (default **200**), the oldest entries are deleted.

A limit of `0` disables that rule. `ecotokens clear --all` also deletes every saved output. The other `clear` filters
(`--before`, `--family`, ...) only apply to recorded interceptions and leave saved outputs alone.

## Settings

Set these in `~/.config/ecotokens/config.json`:

```json
{
  "raw_recovery_enabled": true,
  "raw_recovery_retention_days": 7,
  "raw_recovery_max_entries": 200
}
```

| Key | Default | Meaning |
|-----|---------|---------|
| `raw_recovery_enabled` | `true` | Save outputs and print the hint. When `false`, nothing is written and no hint is shown. |
| `raw_recovery_retention_days` | `7` | Delete entries older than this many days. `0` disables age pruning. |
| `raw_recovery_max_entries` | `200` | Keep at most this many entries, newest first. `0` disables count pruning. |

## Failure behaviour

Recovery is a convenience and never a failure. If the store cannot be located or written (read-only config directory,
full disk), the filtered output is returned as usual, without a hint.

## Known limits

- The hint costs about 15 tokens. It is added after metrics are recorded, so it is not counted in `tokens_after` in
  `ecotokens gain`.
- Only the shared filter pipeline saves outputs. Native-tool hooks that do not go through it (for example the
  Read/Grep/Glob interception) do not produce a recoverable id.
