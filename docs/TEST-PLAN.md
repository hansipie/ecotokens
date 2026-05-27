# ecotokens Test Plan

## Goal

Validate that ecotokens reduces token-heavy tool output without losing critical debugging information, leaking secrets, or breaking agent integrations.

## Success criteria

A release candidate is acceptable when all of the following are true:

1. `cargo test` passes on a clean checkout.
2. `cargo clippy -- -D warnings` passes.
3. Core end-to-end scenarios succeed for Claude Code and any target touched by the release.
4. No regression is observed in filtering quality, metrics collection, install or uninstall behavior, indexing, or MCP output.
5. Sensitive values remain masked in all tested paths.
6. Performance remains within expected bounds for interactive use.

## Current baseline

As of this plan, the repository already contains broad automated coverage across filters, hooks, install flow, search, duplicates, masking, TUI, metrics, watcher, and integration tests.

Reference commands:

```bash
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

## Scope

### In scope

- PreToolUse and PostToolUse filtering behavior
- Native tool result compression for Read, Grep, Glob and equivalent integrations
- Metrics collection and gain reporting
- Secret masking and failure safety
- Hook installation and removal
- Search, outline, symbol, trace, duplicates, watch, and MCP features
- TUI rendering and navigation behavior
- Performance for common interactive workflows

### Out of scope

- Third-party agent bugs outside ecotokens control
- Network availability of remote services unrelated to core functionality
- Long-running benchmarks on every PR

## Risks to control

- Important error lines removed by compression
- Secret redaction gaps
- Hook install or uninstall corrupting existing agent settings
- Metrics drift: savings shown but not actually representative
- Search index becoming stale or inconsistent after file changes
- TUI regressions on small terminal sizes
- Feature interactions, for example abbreviations plus filter assertions

## Test levels

### 1. Unit tests

Purpose: verify deterministic logic in isolation.

Primary areas:
- Filter family behavior by command type
- Token counting
- Secret masking patterns
- Search ranking helpers, vector similarity, chunking, symbol extraction
- TUI rendering helpers
- Config validation and defaults

Key rule:
- Every new public behavior gets at least one focused test near the relevant module family.

### 2. Integration tests

Purpose: validate command-level behavior and cross-module flows.

Primary areas:
- CLI commands
- Install and uninstall
- Filter fallbacks on malformed input
- Watch daemon lifecycle
- Semantic search and MCP JSON output
- Gain history and JSON reports

Key rule:
- Use hermetic temp directories and isolated config homes.

### 3. End-to-end manual QA

Purpose: validate real agent behavior that automated tests cannot fully prove.

Primary areas:
- Claude Code installation
- Hook execution from real agent sessions
- Native tool result interception in actual workflows
- Multi-target compatibility for Gemini CLI, Qwen Code, and Pi when changed by the release

## Test environment matrix

### Platforms

Minimum release gate:
- Linux x86_64

Recommended before tagged release:
- Linux x86_64
- macOS
- Windows or WSL for install and path behavior sanity checks

### Agent targets

Test according to release impact:

- Claude Code: always
- Gemini CLI: when install, hook, MCP, or native tool behavior changes
- Qwen Code: when install, hook, MCP, or native tool behavior changes
- Pi: when extension or post-tool behavior changes

### Build variants

- Default build
- `exact-tokens` feature build when token-counting code changes

Suggested commands:

```bash
cargo build
cargo test
cargo test --features exact-tokens
cargo build --features exact-tokens
```

## Release test checklist

### A. Smoke checks

Run on every change before merge:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

Expected result:
- All commands exit with code 0.

### B. Filter quality regression suite

Objective: ensure compression keeps the useful signal.

Representative commands to exercise with `ecotokens filter --`:
- `git diff`
- `git status`
- `cargo build`
- `cargo test`
- `python -m pytest`
- `ruff check .`
- `npm test`
- `tsc --noEmit`
- `docker images`
- `kubectl get pods -A`
- `grep -R TODO src`
- `find .`
- `ls -R`
- `curl <json-endpoint>`

Verify for each:
- Short outputs pass through unchanged or nearly unchanged
- Long outputs are materially smaller
- Error messages, assertion failures, and stack traces remain visible
- Structured summaries still include file names, counts, and critical identifiers
- UTF-8 text is not corrupted

### C. Native post-tool hook validation

Objective: ensure tool-result interception still works.

Scenarios:
- Read a large source file through the agent and confirm outline-style compression
- Run grep through the agent and confirm result trimming
- Run glob or list-directory through the agent and confirm noisy directories are removed
- Repeat with malformed or empty tool results and confirm safe passthrough

Verify:
- Output is filtered only when appropriate
- Metrics are recorded under the expected family
- Unknown tool payloads do not panic

### D. Install and uninstall safety

Objective: ensure target settings are edited safely and idempotently.

Scenarios:
- Fresh install
- Repeated install
- Uninstall after install
- Uninstall when settings already lack ecotokens
- Install with pre-existing third-party hooks and MCP entries

Verify:
- ecotokens hook entries appear exactly once
- Third-party entries remain intact
- MCP server registration is added and removed correctly
- Session hooks or post hooks are not duplicated

### E. Metrics and gain reporting

Objective: ensure savings data is trustworthy and readable.

Scenarios:
- Record a few filtered commands
- Check `ecotokens gain`
- Check `ecotokens gain --history`
- Check `ecotokens gain --json`
- Check period filters: `today`, `week`, `month`

Verify:
- Totals increase after filtered runs
- By-family and by-project grouping looks correct
- JSON output is valid and stable enough for automation
- Cost avoided is non-negative when savings exist

### F. Search and code-intelligence features

Objective: ensure indexed workflows remain reliable.

Scenarios:
- `ecotokens index`
- `ecotokens search <query>`
- `ecotokens outline <path>`
- `ecotokens symbol <id>`
- `ecotokens trace callers <symbol>`
- `ecotokens trace callees <symbol>`
- `ecotokens duplicates`
- `ecotokens watch`
- `ecotokens mcp-server`

Verify:
- Index builds on a clean repo
- Search returns relevant snippets and file paths
- Search on unindexed directories fails cleanly with a useful error
- Outline and symbol IDs are stable enough for follow-up lookup
- Trace output includes meaningful caller or callee edges
- Duplicate detection returns expected groups and valid JSON when requested
- Watch reindexes changed files and ignores excluded or binary files
- MCP responses remain valid JSON

### G. Security and resilience

Objective: confirm ecotokens fails safely.

Test inputs:
- AWS, GitHub, OpenAI, Anthropic, Stripe, Slack, bearer, JWT, URL credential patterns
- Invalid JSON that looks almost valid
- Huge outputs
- Empty outputs
- Binary files
- Missing files
- Unreadable commands
- Commands failing with non-zero exit codes

Verify:
- Secrets are always redacted
- Failures never cause panic
- Original or safe fallback content is returned when filtering fails
- Large outputs are handled within threshold and timeout expectations

### H. TUI checks

Objective: ensure user-visible terminal screens stay usable.

Screens:
- Gain
- Watch
- Outline
- Trace
- Progress

Verify:
- Screen renders without panic on empty data
- Headers and key labels appear
- Scrolling behaves correctly
- Narrow terminal widths remain readable enough to diagnose state

### I. Performance checks

Objective: keep ecotokens interactive.

Minimum scenarios:
- Large `git diff`
- Large grep result
- Gain report with a populated metrics store
- Initial index on a medium repository
- Incremental reindex after one file change

Measure:
- End-to-end filter latency
- Gain report latency
- Initial indexing time
- Incremental update time
- Output size reduction percentage

Suggested lightweight gate:
- Track p50 and p90 latency for filter flows during perf tests
- Flag regressions if they materially worsen interactive feel

## Priority matrix

### P0: must test before every release

- `cargo test`
- `cargo clippy -- -D warnings`
- Install and uninstall safety for affected targets
- Filter quality on core command families
- Secret masking regressions
- Gain JSON and history output
- Search and watch if code-intelligence modules changed

### P1: test when related code changes

- Gemini CLI, Qwen Code, Pi integrations
- Duplicates detection
- Abbreviations feature
- MCP server behavior
- TUI interaction details
- Exact-token feature build

### P2: periodic or pre-major-release checks

- Cross-platform validation
- Large real-world corpus testing
- Extended latency benchmarking
- Compatibility testing against multiple agent versions

## Test data strategy

Maintain a small, reusable corpus for regression tests:

1. Source files with rich symbol structure
2. Large diffs with both signal and noise
3. Build logs containing warnings plus errors
4. Test outputs with tracebacks and summaries
5. JSON payloads, including large arrays and large objects
6. Directory listings with noisy paths like `node_modules` and `target`
7. Secret-containing fixtures covering every redaction family
8. Binary or non-indexable files

Prefer fixtures that are:
- small enough for fast CI
- realistic enough to reproduce real filtering tradeoffs
- stable enough to avoid snapshot churn

## Suggested manual QA script

Run this before a tagged release or after hook-related changes:

1. Build and install the current binary locally.
2. Install ecotokens into Claude Code.
3. Open a real agent session in a temp repo.
4. Trigger:
   - a large file read
   - a grep with many matches
   - a noisy directory listing
   - a failing build or test command
5. Confirm filtered output quality in the actual agent conversation.
6. Inspect `ecotokens gain --history`.
7. Uninstall and verify settings are clean.
8. Repeat on any additional target touched by the release.

## Exit criteria for release

Ship only when:
- automated gates pass
- required manual QA for impacted targets passes
- no P0 defect remains open
- documentation stays consistent with the shipped behavior

## Reporting template

Use this simple template for release QA notes:

```text
Release candidate:
Commit:
Tester:
Date:

Automated checks:
- cargo fmt --check: PASS/FAIL
- cargo clippy -- -D warnings: PASS/FAIL
- cargo test: PASS/FAIL
- exact-tokens build/tests: PASS/FAIL or N/A

Manual checks:
- Claude Code install/uninstall: PASS/FAIL
- Post-tool Read/Grep/Glob: PASS/FAIL
- Gain dashboard/history/json: PASS/FAIL
- Search/index/watch: PASS/FAIL
- Secret masking spot-check: PASS/FAIL
- Other affected targets: PASS/FAIL or N/A

Notes:
- Regressions found:
- Follow-up actions:
```

## Future improvements

- Add golden-file regression fixtures for real command outputs
- Add cross-platform CI for install-path validation
- Add automated end-to-end harnesses against supported agent targets where feasible
- Track benchmark history over time instead of ad hoc perf checks
