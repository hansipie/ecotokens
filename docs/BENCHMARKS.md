# Benchmarks

Real-world token savings measured on a developer workstation running ecotokens from **2026-03-06 to 2026-03-12**.

## Global Results

| Metric | Value |
|--------|-------|
| Total hook executions | 2 594 |
| Tokens before filtering | 5 209 201 |
| Tokens after filtering | 515 022 |
| **Tokens saved** | **4 694 179** |
| **Overall reduction** | **90.1 %** |
| Avg. reduction (when active) | 65.7 % |
| Commands with savings | 327 / 2 594 (12.6 %) |

> The passthrough mode (where outputs are too small to compress) still contributes to the overall score because ecotokens only compresses outputs that are worth compressing — small outputs are returned as-is at zero overhead.

## Savings by Command Family

| Family | Executions | Tokens before | Tokens after | Saved | Reduction |
|--------|-----------|--------------|-------------|-------|-----------|
| generic | 1 934 | 2 685 276 | 357 647 | **2 327 629** | 86.7 % |
| git | 195 | 2 083 638 | 49 045 | **2 034 593** | 97.6 % |
| fs | 96 | 189 842 | 44 099 | **145 743** | 76.8 % |
| cargo | 242 | 165 308 | 51 864 | **113 444** | 68.6 % |
| cpp | 77 | 77 687 | 5 932 | **71 755** | 92.4 % |
| grep | 34 | 5 893 | 4 880 | 1 013 | 17.2 % |
| python | 16 | 1 557 | 1 555 | 2 | 0.1 % |

**Git commands are the biggest win** with a 97.6 % reduction — `git diff`, `git log`, and `git status` outputs are often massive and compress extremely well.

## Processing Modes

| Mode | Count | Description |
|------|-------|-------------|
| passthrough | 1 521 | Output too small to compress — returned as-is |
| summarized | 747 | Output rewritten by the AI summarizer (Ollama) |
| filtered | 326 | Output trimmed by a family-specific rule-based filter |

## Top 5 Single-Command Savings

| Command | Before | After | Saved | Reduction |
|---------|--------|-------|-------|-----------|
| `git -C /var/home/hansi/Code/ecotokens diff --staged` | 1 682 049 | 782 | **1 681 267** | 100.0 % |
| `strings target/release/ecotokens` | 971 965 | 493 | **971 472** | 99.9 % |
| `strings target/release/ecotokens` | 971 965 | 493 | **971 472** | 99.9 % |
| `git -C /var/home/hansi/Code/ecotokens status --short` | 104 658 | 939 | **103 719** | 99.1 % |
| `git checkout 001-token-companion` | 103 277 | 966 | **102 311** | 99.1 % |

A single `git diff --staged` on a large working tree saved **1.68 million tokens** — equivalent to roughly 10× the context window of most LLMs.

## Methodology

Metrics are recorded by the ecotokens hook in `~/.config/ecotokens/metrics.db`. Each entry captures:
- `tokens_before` / `tokens_after` — token counts estimated via character heuristic (or tiktoken when the `exact-tokens` feature is enabled)
- `command_family` — detected from the command name
- `mode` — `passthrough`, `filtered`, or `summarized`
