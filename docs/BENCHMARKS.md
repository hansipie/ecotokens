# Benchmarks

Real-world token savings measured on a developer workstation running ecotokens from **2026-03-06 to 2026-05-27**.

## Global Results

| Metric | Value |
|--------|-------|
| Total hook executions | 19 928 |
| Tokens before filtering | 94 282 087 |
| Tokens after filtering | 5 874 896 |
| **Tokens saved** | **88 407 191** |
| **Overall reduction** | **93.8 %** |
| Avg. reduction (when active) | 65.2 % |
| Commands with savings | 5 735 / 19 928 (28.8 %) |

> The passthrough mode, where outputs are too small to compress, still contributes to the overall score because ecotokens only compresses outputs that are worth compressing. Small outputs are returned as-is at zero overhead.

## Savings by Command Family

| Family | Executions | Tokens before | Tokens after | Saved | Reduction |
|--------|-----------:|--------------:|-------------:|------:|----------:|
| grep | 1 021 | 57 240 989 | 1 857 821 | **55 383 168** | 96.8 % |
| fs | 1 690 | 11 755 125 | 505 181 | **11 249 944** | 95.7 % |
| generic | 10 725 | 12 862 897 | 2 183 123 | **10 679 774** | 83.0 % |
| native_read | 2 666 | 3 367 600 | 609 744 | **2 757 856** | 81.9 % |
| git | 1 745 | 2 988 717 | 296 664 | **2 692 053** | 90.1 % |
| gh | 274 | 2 494 979 | 78 279 | **2 416 700** | 96.9 % |
| network | 101 | 2 051 361 | 43 837 | **2 007 524** | 97.9 % |
| cargo | 846 | 966 193 | 157 574 | **808 619** | 83.7 % |
| python | 428 | 316 662 | 96 570 | **220 092** | 69.5 % |
| cpp | 370 | 228 070 | 39 274 | **188 796** | 82.8 % |
| js | 57 | 9 467 | 6 802 | **2 665** | 28.2 % |
| container | 5 | 27 | 27 | **0** | 0.0 % |

**grep commands are the biggest win** with 55 383 168 tokens saved and a 96.8 % reduction. Search-heavy workflows now dominate the benchmark corpus, especially large `grep` scans across Claude project logs and local workspaces.

## Processing Modes

| Mode | Count | Description |
|------|------:|-------------|
| summarized | 12 670 | Output rewritten by the AI summarizer |
| filtered | 5 735 | Output trimmed by a family-specific rule-based filter |
| passthrough | 1 523 | Output too small to compress, returned as-is |

## Top 5 Single-Command Savings

| Command | Before | After | Saved | Reduction |
|---------|-------:|------:|------:|----------:|
| `grep -rh "role":"user" /var/home/hansi/.claude/projects/-var-home-hansi-Code-ecotokens/00bd66...` | 11 406 673 | 82 783 | **11 323 890** | 99.3 % |
| `grep -rn ecotokens\|v0\.\|Product Hunt\|PH\|roadmap /var/home/hansi/.claude/projects/-var-hom...` | 7 690 957 | 41 748 | **7 649 209** | 99.5 % |
| `grep -rh tiktoken\|qwen\|v0\.8\|v0\.9\|version.*0\.\|CHANGELOG\|feat:\|fix:\|chore: /var/home...` | 6 950 638 | 87 070 | **6 863 568** | 98.7 % |
| `grep -h korben\|Korben\|v0\.\|version\|ecotokens /var/home/hansi/.claude/projects/-var-home-h...` | 4 479 370 | 25 588 | **4 453 782** | 99.4 % |
| `grep -rh HookPost\|hook_post\|PostToolUse\|post_handler\|hook-post\|HookGemini\|HookQwen\|nat...` | 3 612 393 | 43 898 | **3 568 495** | 98.8 % |

The largest single interception saved **11 323 890 tokens** from one command. The largest Git example remains `git diff --staged`, which saved **1 681 267 tokens** on a large staged diff.

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
