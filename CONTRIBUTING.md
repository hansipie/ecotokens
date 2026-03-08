# Contributing to ecotokens

Thank you for taking the time to contribute. This guide explains how to set up your environment, write code that passes review, and submit a pull request.

---

## Table of contents

- [Prerequisites](#prerequisites)
- [Getting started](#getting-started)
- [Project structure](#project-structure)
- [Development workflow](#development-workflow)
- [Writing tests](#writing-tests)
- [Code quality checklist](#code-quality-checklist)
- [Submitting a pull request](#submitting-a-pull-request)
- [Commit message format](#commit-message-format)

---

## Prerequisites

- **Rust stable ≥ 1.75** (no nightly features — the project enforces this)
- `cargo`, `rustfmt`, `clippy` (all included with `rustup`)

```bash
rustup update stable
rustup component add rustfmt clippy
```

---

## Getting started

```bash
git clone https://github.com/hansipie/ecotokens.git
cd ecotokens
cargo build
cargo test
```

All tests must pass on a clean clone before you make any change.

---

## Project structure

```
src/
  main.rs           # CLI entry point — commands, keyboard handlers
  filter/           # Output filters per command family (git, cargo, python, …)
  metrics/          # Token store and report aggregation
  search/           # BM25 + symbolic index, query, outline, trace
  tui/              # Ratatui panels (gain, outline, trace, watch, progress)
  hook/             # PreToolUse hook handler
  install/          # Hook + MCP registration in ~/.claude/settings.json
  config/           # User settings (embed provider, thresholds, exclusions)
  masking/          # PII / secret redaction before filtering
  daemon/           # File watcher for live index updates
  mcp/              # MCP server (JSON-RPC over stdio)
  trace/            # Call graph (callers / callees)
  tokens/           # Token counting (estimate + optional tiktoken)

tests/
  filter/           # Unit tests for each filter family
  metrics/          # Store and report tests
  search/           # Index, query, outline, symbols, embed, watcher tests
  tui/              # TUI rendering tests (TestBackend)
  trace/            # Call graph tests
  hook/             # Hook handler tests
  config/           # Settings load/save tests
  masking/          # PII pattern tests
  integration/      # End-to-end, install, MCP, perf, fault tests
```

---

## Development workflow

### 1. Run the full test suite

```bash
cargo test
```

### 2. Run a specific test file

```bash
cargo test --test gain_tui_test
cargo test --test git_test
```

### 3. Lint

```bash
cargo clippy -- -D warnings
```

No clippy warnings are allowed. Fix or explicitly `#[allow(...)]` with a comment explaining why.

### 4. Format

```bash
cargo fmt
```

`rustfmt` is non-negotiable. CI will reject unformatted code.

### 5. Quick pre-push check

```bash
cargo fmt --check && cargo clippy -- -D warnings && cargo test
```

---

## Writing tests

### Where to add tests

| What you changed | Where to add tests |
|------------------|--------------------|
| A filter (`filter/git.rs`) | `tests/filter/git_test.rs` |
| A TUI panel (`tui/gain.rs`) | `tests/tui/gain_test.rs` |
| Metrics logic | `tests/metrics/` |
| Search / index | `tests/search/` |
| Install / config | `tests/integration/install_test.rs` |
| New CLI command | `tests/integration/end_to_end_test.rs` |

### Rules

1. **Every new public function must have at least one test.** Prefer multiple small tests over one large one.
2. **Test names describe the expected behavior**, not the implementation:
   - `git_diff_removes_index_lines` ✓
   - `test_filter` ✗
3. **TUI tests use `TestBackend`** — never spin up a real terminal. See `tests/tui/gain_test.rs` for the pattern.
4. **Integration tests must be hermetic** — use `tempfile::tempdir()` for any filesystem work, never write to `$HOME`.
5. **No `unwrap()` in test setup** — use `expect("context message")` so failures are readable.
6. **Test both the happy path and edge cases**: empty input, single item, maximum size, invalid data.

### TUI test pattern

```rust
use ratatui::backend::TestBackend;
use ratatui::Terminal;

#[test]
fn my_panel_renders_title() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_my_panel(frame, frame.area(), &data))
        .unwrap();
    let content: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(content.contains("Expected Title"), "{content:?}");
}
```

### Filter test pattern

```rust
#[test]
fn cargo_build_keeps_error_lines() {
    let input = "   Compiling foo v0.1.0\nerror[E0308]: type mismatch\n";
    let output = filter_cargo("cargo build", input);
    assert!(output.contains("error[E0308]"));
    assert!(!output.contains("Compiling foo"));
}
```

---

## Code quality checklist

Before opening a pull request, verify each item:

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -- -D warnings` passes with zero warnings
- [ ] `cargo test` — all tests pass
- [ ] New code has tests (see rules above)
- [ ] No `unwrap()` on paths that can realistically fail at runtime — use `?` or handle the error
- [ ] No `unsafe` code without a documented safety comment
- [ ] No nightly-only features (`#![feature(...)]`)
- [ ] Public functions and types have doc comments if their purpose is not immediately obvious
- [ ] Secrets and PII are never logged or stored (use `masking::mask()` before recording content)

---

## Submitting a pull request

1. **Fork** the repository and create a branch from `main`:
   ```bash
   git checkout -b feat/my-feature
   ```
2. Make your changes, following the workflow above.
3. Push and open a pull request against `main`.
4. The PR description must explain **what** changed and **why**.
5. Link any related issue with `Closes #N`.

PRs are merged when:
- All CI checks pass (fmt, clippy, tests)
- At least one review approval

---

## Commit message format

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <short description>

[optional body]
```

**Types:** `feat`, `fix`, `docs`, `refactor`, `test`, `chore`

**Examples:**

```
feat(filter): compress cargo test output by removing passing lines
fix(tui): prevent panic when project list is empty
test(filter): add edge cases for git diff with binary files
chore: add MIT license file
```

- Use the **present imperative** tense ("add", not "added" or "adds")
- Keep the subject line under 72 characters
- One logical change per commit — split unrelated changes into separate commits
