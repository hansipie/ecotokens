# Benchmarks

Real-world token savings measured on a developer workstation running ecotokens over the **last month, from 2026-09-20 to 2026-10-20**.

## Global Results

| Metric | Value |
|--------|-------|
| Total hook executions | 10 581 |
| Tokens before filtering | 11 059 097 |
| Tokens after filtering | 4 250 103 |
| **Tokens saved** | **6 808 994** |
| **Overall reduction** | **61.6 %** |
| Avg. reduction (when active) | 46.6 % |
| Commands with savings | 4 697 / 10 581 (44.4 %) |

> Outputs that are too small to compress are returned as-is at zero overhead; they count in the executions but not in the savings. For comparison, the whole history (2026-03-06 to 2026-10-20, 45 505 executions) reaches 86.9 %, and the last three months 64.5 %: the large `grep` scans that dominated the early corpus are rarer now.

## Savings by Command Family

| Family | Executions | Tokens before | Tokens after | Saved | Reduction |
|--------|-----------:|--------------:|-------------:|------:|----------:|
| generic | 5 679 | 5 361 665 | 2 112 913 | **3 248 752** | 60.6 % |
| fs | 1 237 | 2 629 077 | 1 145 802 | **1 483 275** | 56.4 % |
| native_read | 776 | 1 100 031 | 234 757 | **865 274** | 78.7 % |
| cargo | 338 | 511 048 | 49 594 | **461 454** | 90.3 % |
| grep | 894 | 623 294 | 326 792 | **296 502** | 47.6 % |
| python | 625 | 358 227 | 162 597 | **195 630** | 54.6 % |
| git | 816 | 329 747 | 146 759 | **182 988** | 55.5 % |
| network | 57 | 110 785 | 46 508 | **64 277** | 58.0 % |
| gh | 139 | 29 385 | 20 563 | **8 822** | 30.0 % |
| js | 18 | 5 674 | 3 654 | **2 020** | 35.6 % |
| container | 2 | 164 | 164 | **0** | 0.0 % |

**generic commands are the biggest win** over this period with 3 248 752 tokens saved. `cargo` has the highest reduction (90.3 %).

## Processing Modes

| Mode | Count | Description |
|------|------:|-------------|
| summarized | 5 884 | Output rewritten by the AI summarizer |
| filtered | 3 767 | Output trimmed by a family-specific rule-based filter |
| rewritten | 930 | Output transformed by the local text rewrite stage |

## Top 5 Single-Command Savings

| Command | Before | After | Saved | Reduction |
|---------|-------:|------:|------:|----------:|
| `ls -la`, `wc -l`, `head -30` and `tail -30` on the ecotokens `debug.log` file | 273 883 | 11 701 | **262 182** | 95.7 % |
| `grep` of the `jev` lines in `debug.log`, piped to `json.tool` | 35 075 | 1 033 | **34 042** | 97.1 % |
| Hermes `browser_exec` tool call | 35 052 | 1 026 | **34 026** | 97.1 % |
| `sed -n 1,80p ~/.codex/config.toml` (keys redacted), `codex --version`, `ls ~/.codex` | 25 890 | 370 | **25 520** | 98.6 % |
| Hermes `read_file` tool call | 25 808 | 1 029 | **24 779** | 96.0 % |

The largest single interception saved **262 182 tokens** from one command. The largest Git example saved **23 072 tokens**.

## Methodology

Metrics are recorded by the ecotokens hook in `~/.config/ecotokens/metrics.db`. Each entry captures:
- `tokens_before` / `tokens_after` - token counts estimated via character heuristic, or tiktoken when the `exact-tokens` feature is enabled
- `command_family` - detected from the command name
- `mode` - `passthrough`, `filtered`, or `summarized`
- `duration_ms` - hook processing time for the interception

## Session handoff hook latency

Process wall time of the `SessionStart` hooks, release build, Linux, 40 runs each (see [handoff.md](handoff.md) and `docs/TEST-PLAN.md` section K):

| Scenario | p50 | p90 |
|----------|-----|-----|
| existing `session-start` hook (reference) | 3.8 ms | 4.1 ms |
| `hook-handoff`, feature on, nothing saved | 4.0 ms | 4.4 ms |
| `hook-handoff`, 200 saved files, none for this directory | 9.0 ms | 9.5 ms |
| `hook-handoff`, one handoff injected | 9.2 ms | 10.4 ms |
| `hook-handoff`, 40 pending handoffs (short list) | 10.8 ms | 12.4 ms |
| `ecotokens handoff write`, 5 MB transcript | 124 ms | n/a |

Claude Code runs matching `SessionStart` hooks in parallel, so a session start waits for the slowest of them, not their sum. When nothing is injected the new hook costs about the same as the existing one.

Injecting used to cost about 98 ms (p90 103 ms). Per-pattern compile timings showed that 4 of the 39 secret-masking patterns, all written with `\w`, took 98% of it: the regex crate reads `\w` as Unicode, which makes bounded repetitions such as `\w{82}` expensive to compile (`pypi-…` 42.7 ms, `github_pat_` 15.3 ms, `AIza` 6.5 ms, `glpat-` 3.9 ms; every other pattern is under 0.25 ms). Writing those four patterns ASCII-only (`(?-u)`) brought the cold pass over all patterns from about 73-100 ms to 1.7 ms, which every ecotokens process that masks pays once. Real credentials of these services are ASCII, so no real token stops being masked. A guard test (`tests/masking/word_class_test.rs`) fails if a pattern using `\w` loses its `(?-u)`.

## Router

Measured on 2026-10-17, Linux, release builds, against the live `api.typesafe.ai` service (see [model-router.md](model-router.md) and `docs/TEST-PLAN.md` sections I and the router rows). The **baseline** is the installed release `0.30.1`, which is the code before the task card; it was measured after the new code was written, but from the unchanged release binary, so it is the pre-change figure. **After** is the policy router. Each pair was run interleaved (old, new, new, old, …) to share the network's mood. The hook was run with `router on --timeout-ms 8000` in a throwaway `HOME`; every run delegated.

### Per message

| Scenario | Baseline p50 | Baseline p90 | After p50 | After p90 | Change (p50 / p90) |
|----------|--------------|--------------|-----------|-----------|--------------------|
| `hook-prompt` end to end, Jev included, 30 runs each | 465 ms | 526 ms | 461 ms | 546 ms | −1% / +4% |
| `hook-prompt`, private folder (gate only, no network), 200 runs | n/a | n/a | 4 ms | 5 ms | new path |
| `hook-prompt`, no API key (earliest exit), 200 runs | 5 ms | 5 ms | 4 ms | 4 ms | unchanged |

The added delay stays inside the budget of the specification (p90 within +10%, median within +15%): asking six questions instead of two in the same request does not make the answer slower, and the local gate costs about 4 ms, which is the process start. With 30 runs the network noise is larger than the difference, so read it as "no measurable change", not as a gain.

### Jev tokens per request

| | Input | Output |
|--|-------|--------|
| Baseline (2 questions) | 552–561 | 66–67 |
| After (6 questions) | 1067–1076 | 212–214 |

The request now costs about **twice the input and three times the output tokens** of the size-only request. Jev has no published price (`router price` stays blank until you set one), so this is shown in tokens: at the 800 ms default timeout the router still mostly stays out of the way, and the figures above were measured with a longer timeout.

### Other scenarios of TEST-PLAN section I

The hook path and `src/main.rs` changed, the filters did not. Same binaries, 15 runs each (3 for the index), p50 / p90 wall time:

| Scenario | Baseline | After |
|----------|----------|-------|
| `filter -- git log -p -n 40` (8.5k chars out) | 89 / 94 ms | 91 / 94 ms |
| `filter -- grep -rn fn src` (39.8k chars out) | 15 / 16 ms | 15 / 16 ms |
| `gain --json` on a copy of a 150 MB metrics store | 296 / 304 ms | 297 / 302 ms |
| initial index of `src/` (embeddings off, 3 runs) | 148 / 148 ms | 147 / 151 ms |
| incremental reindex after one changed file (3 runs) | 24 / 25 ms | 10 / 11 ms |

The incremental figure comes from only three runs on a tiny index and is not a speed-up to rely on; the point is that nothing got slower. Output sizes are identical.

### Hook-free classification quality

The reference set of 43 labelled messages (`tests/router/fixtures/card_reference.json`, run with `cargo test --test router_live_card_test -- --ignored`) gave: nature 37/43 (86%), risk 40/43 (93%). Of the 13 messages labelled sensitive, destructive or security, 7 went to an advanced helper (`router-large`, `router-hardest`, `router-analyst`), 6 were **kept by the main session** because Jev's size answer was below the 0.6 confidence threshold (the first fallback of the task card), and none went to a tiny or everyday helper. The 100% target of the specification is therefore met in the sense that no sensitive job reaches a cheaper model, not in the sense that every one is delegated.
