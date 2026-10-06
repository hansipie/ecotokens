# Session handoff

Save the state of a long Claude Code session, clear the context, and keep working with the state already loaded. It is a manual alternative to compaction: you decide when, the saved state is a short structured file you can read and edit, and nothing is sent anywhere.

Claude Code only. It does not replace the native compaction (a hook cannot supply a summary); it adds a structured state next to it.

## How it works

```bash
ecotokens handoff on      # installs the SessionStart hook and the two skills
# restart Claude Code once so it loads them
```

In a session that has grown long:

1. Type `/handoff`. The skill saves the objective, the problem, the key files, what was tried and failed, and the next steps, then prints the file path.
2. Type `/clear`. Claude Code cannot be cleared from a skill or a hook, so this step is yours.
3. The new session starts with the saved state already in its context.

If you use `/compact` instead, the same state is added after the native summary.

If several saved states exist for the directory, the new session only receives a short list, and you pick one with `/handoff-load <id>`.

## What is in the file

A Markdown file with fixed sections, stored in `~/.config/ecotokens/handoff/<session id>.md`:

```markdown
# Handoff

## Header
- session: 8c7ad6e1-2210-4fd5-9a87-b8927a101748
- cwd: /home/me/project
- branch: feat/x
- created: 2026-10-04T18:20:11Z
- status: pending

## Objective
Add a manual handoff command that replaces compaction.

## Problem
A hook cannot supply a summary, so the state must be saved by a command.

## Key files
- src/install.rs (modified)
- src/router/hook.rs (read ×3)

## Failed attempts
- [test_or_build] cargo test: exit 101
- [edit_reverted] src/install.rs
- [hypothesis] A single matcher cannot cover startup and clear.

## Next steps
Write the failing tests for the store.
```

It is written in two steps, so a usable file exists even if the second one never happens:

| Step | Who | What |
|------|-----|------|
| `ecotokens handoff write` | ecotokens, no model | key files and mechanical failed attempts read from the session transcript, header |
| `ecotokens handoff set` | the model, through the skill | objective, problem, next steps, abandoned ideas |

Failed attempts come from four sources: commands that ended with a non-zero exit status, failed test or build runs, edits that were reverted or rewritten in the same place (all extracted from the transcript), and ideas the model says it abandoned. Failures of read-only commands such as `grep` or `find` are ignored, and the exit status of a chain (`a && b`) is judged on its last command.

You can edit the file by hand before it is injected. Text you add is kept.

## Commands

| Command | Does |
|---------|------|
| `ecotokens handoff on [--stale-hours N] [--max-chars N]` | installs the hook and the skills, turns the feature on |
| `ecotokens handoff off` | removes the hook and the skills; saved handoffs are kept |
| `ecotokens handoff status [--json]` | setup, thresholds, what is saved for this directory, injection statistics |
| `ecotokens handoff write --session <id>` | step 1 (used by `/handoff`) |
| `ecotokens handoff set --session <id> [--stdin]` | step 2 (used by `/handoff`) |
| `ecotokens handoff list [--all] [--json]` | the saved handoffs of this directory, newest first |
| `ecotokens handoff load [<id>] [--json]` | prints a handoff (the newest pending one of this directory without an id) (used by `/handoff-load`) and marks it consumed |
| `ecotokens handoff clean [--consumed] [--dry-run] [--json]` | removes handoffs and session records past the retention; `--consumed` also removes every consumed handoff, whatever its age |

Every structured command has `--json`. Exit codes: `0` success, `1` operational error, `2` invalid argument.

## Which handoff a new session receives

- Matching is by **working directory**, not by session id (a new session after `/clear` has a different id). The git branch is shown in the header but does not matter, so changing branches mid-task does not lose the handoff.
- Exactly one pending handoff for the directory: it is injected, then marked `consumed`. A consumed handoff is never injected again automatically, but `/handoff-load <id>` still loads it until it is deleted, `handoff_consumed_retention_hours` after it was consumed.
- Several pending handoffs: only a short list is injected and nothing is loaded until you choose.
- A handoff older than the stale threshold is still injected, with a line saying how old it is and asking to verify it.
- `clear` and `compact` always inject. `startup`, `resume` and `fork` inject only if you set `handoff_inject_startup`.

## Settings

In `~/.config/ecotokens/config.json`:

| Key | Default | Meaning |
|-----|---------|---------|
| `handoff_enabled` | `false` | set by `handoff on` / `off` |
| `handoff_max_chars` | `4000` | size limit of a handoff, capped at `9000` |
| `handoff_stale_hours` | `24` | age after which an injected handoff is flagged as stale |
| `handoff_retention_days` | `30` | handoffs and session records older than this are deleted, consumed or not |
| `handoff_consumed_retention_hours` | `48` | consumed handoffs are deleted once they were consumed this long ago (`0`: at the next cleanup) |
| `handoff_inject_startup` | `false` | also inject on `startup`, `resume` and `fork` |

The cap of `9000` exists because Claude Code replaces injected hook output longer than 10,000 characters by a file path and a short preview. When a handoff is too long it is trimmed in this order: oldest failed attempts, lowest-ranked key files, the end of the Problem. The objective and the next steps are never cut.

## Safety

- **Secrets are masked** with the same rules as the rest of ecotokens, when the file is written (including the text the model writes), when it is injected, and in every command output and `--json` field. Only secrets the masking module recognises are masked: see [secret-patterns.md](secret-patterns.md). Do not rely on it for a credential in a format it does not know.
- **Nothing leaves your machine.** The feature makes no network call and no model call.
- **Private files.** The handoff directory is created with mode `0700` and its files with `0600`.
- **Fail-open.** Any error (unreadable file, corrupted handoff, bad input, full disk) ends in a normal session start with nothing injected. A corrupted file is skipped, never injected.
- **Safe ids.** A session id becomes a file name, so only letters, digits, `-` and `_` are accepted.

## Limits

- Matching is by the exact working directory. Starting the next session in a sub-directory of the one that wrote the handoff will not match; `ecotokens handoff list --all` shows every directory, and `/handoff-load <id>` loads one explicitly.
- The transcript is an internal Claude Code format. Extraction is tolerant (a line it does not understand is skipped), and when the transcript is unavailable the file is still written with the extracted fields marked `not provided`.
- A session that started before `handoff on` has no recorded transcript location, so its first `/handoff` has no key files or failed attempts. Later sessions are recorded from their start.
- Two sessions starting in the same instant in the same directory could both receive the same pending handoff.

## Performance

Measured on a release build (Linux, 40 runs):

| Scenario | p50 | p90 |
|----------|-----|-----|
| existing `session-start` hook (reference) | 3.8 ms | 4.1 ms |
| `hook-handoff`, nothing saved (every session start) | 4.0 ms | 4.4 ms |
| `hook-handoff`, one handoff injected | 9.2 ms | 10.4 ms |
| `handoff write` on a 5 MB transcript | 124 ms | |

Starting a session with nothing to inject costs the same as the existing session hook, and injecting a handoff adds about 5 ms: the secret-masking patterns compile in under 2 ms (see [BENCHMARKS.md](BENCHMARKS.md) for how that was reached).
