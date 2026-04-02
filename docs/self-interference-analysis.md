# Self-Interference: ecotokens Modifying Its Own Source Files During Commit

## Summary

When running `git commit` inside the ecotokens repository, the PreToolUse /
PostToolUse hooks can interfere with the pre-commit hook, causing `cargo fmt
--check` to fail spuriously. The root cause is that ecotokens intercepts its
own Bash tool calls and rewrites files on disk between the `git add` and
`git commit` steps.

## Reproduction

This was observed when committing changes to `src/masking/patterns.rs` and
`tests/masking/patterns_test.rs` from within a Claude Code session that had
ecotokens installed.

```
1. cargo fmt                  # formats source files on disk
2. git add src/... tests/...  # stages the formatted files
                              # ← PostToolUse hook fires here, reads and
                              #   rewrites the staged files on disk
3. git commit                 # triggers pre-commit hook
4.   cargo fmt --check        # FAILS: disk files ≠ staged files
```

The key observation: **`cargo fmt --check` operates on the working tree (disk),
not on the git index (staged files)**. If anything modifies disk files between
`git add` and `git commit`, the check will fail.

## Root Cause

ecotokens' PostToolUse hook processes every tool result returned by Claude Code,
including `Read` tool results. When a Bash command produces output that ecotokens
analyses, it may trigger outline-based compression which re-reads source files.
Those reads can in turn update the working tree representation used by subsequent
tool operations.

Separately, the watcher daemon (`ecotokens watch --background`, started
automatically by the `SessionStart` hook) monitors the project directory for
changes. Any file write during the session can trigger a reindex, which involves
reading source files through the same pipeline.

Both paths can modify file metadata or content between the `git add` and
`git commit` invocations when they are issued as separate Bash tool calls.

## Why It Appeared Intermittent

- When `cargo fmt --check` was run **standalone**, it passed (exit 0) — because
  no hook interception occurred in the gap between a `git add` and the check.
- When issued as part of a multi-step sequence across **separate Bash tool
  invocations**, the PostToolUse hook had a window to fire between steps.

## Workaround

Chain `cargo fmt`, `git add`, and `git commit` in a **single Bash invocation**:

```bash
cargo fmt && git add <files> && git commit -m "..."
```

This eliminates the window between staging and committing during which the hook
can intervene.

## Potential Fix

Two directions:

### 1. Exclude the project's own source tree from PostToolUse rewriting

Add a guard in the PostToolUse handler to skip outline compression for files
located inside the ecotokens source tree itself when the session is running
from within that directory.

### 2. Make the pre-commit hook index-aware

Replace `cargo fmt --check` with a variant that checks only staged files:

```bash
# Check only what is actually staged, not the working tree
git stash --keep-index --include-untracked -q
cargo fmt --check
STATUS=$?
git stash pop -q
exit $STATUS
```

This is the standard solution for pre-commit hooks that need to validate only
what is being committed, regardless of working tree state.

## Impact

- Affects only contributors developing ecotokens itself inside a Claude Code
  session with ecotokens installed.
- Does not affect end users of ecotokens.
- Workaround is reliable: always chain `cargo fmt && git add && git commit`
  in a single command.

## Related

- Pre-commit hook: `.git/hooks/pre-commit`
- PostToolUse handler: `src/hook/read_handler.rs`, `src/hook/post_handler.rs`
- Watch daemon: `src/daemon/`
