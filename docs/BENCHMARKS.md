# Benchmarks

Real-world token savings measured on a developer workstation running ecotokens from **2026-03-06 to 2026-03-19**.

## Global Results

| Metric | Value |
|--------|-------|
| Total hook executions | 4 129 |
| Tokens before filtering | 7 490 128 |
| Tokens after filtering | 776 043 |
| **Tokens saved** | **6 714 085** |
| **Overall reduction** | **89.6 %** |
| Avg. reduction (when active) | 64.8 % |
| Commands with savings | 531 / 4 129 (12.9 %) |

> The 87 % of commands that pass through unchanged (passthrough mode) still contribute to the overall score because ecotokens only compresses outputs that are worth compressing — small outputs are returned as-is at zero overhead.

## Savings by Command Family

| Family | Executions | Tokens before | Tokens after | Saved | Reduction |
|--------|-----------|--------------|-------------|-------|-----------|
| generic | 2 834 | 4 667 326 | 525 326 | **4 142 000** | 88.7 % |
| git | 401 | 2 146 597 | 73 865 | **2 072 732** | 96.6 % |
| cargo | 324 | 274 031 | 67 535 | **206 496** | 75.4 % |
| fs | 279 | 244 478 | 82 093 | **162 385** | 66.4 % |
| cpp | 107 | 93 947 | 8 002 | **85 945** | 91.5 % |
| network | 3 | 43 570 | 1 226 | **42 344** | 97.2 % |
| grep | 102 | 14 602 | 12 421 | 2 181 | 14.9 % |
| python | 46 | 3 805 | 3 803 | 2 | ~0 % |
| gh | 33 | 1 772 | 1 772 | 0 | 0 % |

**Git commands are the biggest win** at 96.6 % reduction — `git diff`, `git log`, and `git status` outputs are often massive and compress extremely well.

## Processing Modes

| Mode | Count | Description |
|------|-------|-------------|
| summarized | 2 078 | Output rewritten by the AI summarizer (Ollama) |
| passthrough | 1 521 | Output too small to compress — returned as-is |
| filtered | 530 | Output trimmed by a family-specific rule-based filter |

## Top 5 Single-Command Savings

| Command | Before | After | Saved | Reduction |
|---------|--------|-------|-------|-----------|
| `git diff --staged` | 1 682 049 | 782 | **1 681 267** | 100.0 % |
| `strings SynologyDrive.app/bin/…` | 1 556 886 | 204 | **1 556 682** | 100.0 % |
| `strings target/release/ecotokens` | 971 965 | 493 | **971 472** | 99.9 % |
| `strings target/release/ecotokens` | 971 965 | 493 | **971 472** | 99.9 % |
| `git status --short` | 104 658 | 939 | **103 719** | 99.1 % |

A single `git diff --staged` on a large working tree saved **1.68 million tokens** — equivalent to roughly 10× the context window of most LLMs.

## Methodology

Metrics are recorded by the ecotokens hook in `~/.config/ecotokens/metrics.jsonl`. Each entry captures:
- `tokens_before` / `tokens_after` — token counts estimated via character heuristic (or tiktoken when the `exact-tokens` feature is enabled)
- `command_family` — detected from the command name
- `mode` — `passthrough`, `filtered`, or `summarized`
