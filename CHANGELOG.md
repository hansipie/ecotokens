# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.13.0] - 2026-04-02

### Added
- SQLite metrics backend (`metrics.db`) via `rusqlite` with bundled SQLite

### Changed
- Metrics storage migrated from JSONL to SQLite with schema/index initialization on open
- Default metrics path changed from `metrics.jsonl` to `metrics.db`
- Legacy `metrics.jsonl` is migrated automatically and preserved as `metrics.jsonl.migrated`

### Fixed
- Read paths now trigger legacy migration too: `ecotokens gain` can migrate existing JSONL data even when no SQLite file exists yet
- Integration/performance tests updated for SQLite-backed metrics store

## [0.12.0] - 2026-04-02

### Security
- Secret scanner expanded from 9 to 41 patterns — now covers GCP, Azure AD, DigitalOcean, Anthropic, OpenAI, HuggingFace, GitHub (fine-grained PAT, app token, OAuth, refresh), GitLab (PAT, deploy token), Slack app-token, Twilio, SendGrid, npm, PyPI, Databricks, HashiCorp TF, Pulumi, Postman, Grafana (API key, cloud token, service account), Sentry (user + org), Shopify (access token + shared secret)
- AWS pattern extended to cover `ASIA`, `ABIA`, `ACCA`, `A3T…` prefixes in addition to `AKIA`
- Stripe pattern extended to cover `rk_` keys and `prod` environment

### Docs
- Secret redaction reference table moved from README to [`docs/secret-patterns.md`](docs/secret-patterns.md)
- Added [`docs/self-interference-analysis.md`](docs/self-interference-analysis.md) — analysis of the race condition between ecotokens hooks and the pre-commit hook when developing ecotokens itself

## [0.11.0] - 2026-04-01

### Added
- `ecotokens update` command — checks GitHub Releases for the latest version and installs it via `cargo install ecotokens`; supports `--check` flag to report availability without installing

## [0.10.0] - 2026-03-28

### Added
- Slack token (`xox[bpoa]-…`) and Stripe secret key (`sk_live/test_…`) masking patterns — redacted before any content reaches the model

### Security
- Hook handlers now cap stdin at 10 MB (`MAX_STDIN_BYTES`) — oversized payloads are passed through unchanged instead of consuming unbounded memory
- Ollama AI summary URL is validated to point to localhost only (SSRF guard)
- Indexer skips files larger than 50 MB to prevent memory exhaustion

### Changed
- Gain TUI: `DetailMode` simplified from a 3-cycle (Split / Diff / Log) to a 2-cycle toggle (Details / Diff)
- Gain TUI: view switching is now contextual — `p` switches to the project view (from family), `f` switches back to the family view (from project); the generic `b` toggle is removed
- Gain TUI: log-panel selection index is clamped to the actual item count via `log_item_count`
- Gain TUI: key-repeat events filtered out (only `KeyEventKind::Press` is handled)

## [0.9.0] - 2026-03-26

### Added
- Claude Code `PostToolUse` hook support for native tool result optimization
- Specialized handlers for `Read`, `Grep`, and `Glob` native tool results
- `ecotokens hook-post` command for tool result interception
- `CommandFamily::NativeRead` for tracking Read tool interceptions in metrics

## [0.8.0] - 2026-03-23

### Added
- Qwen Code support (PreToolUse, MCP, session hooks)
- `ecotokens install qwen` and `ecotokens uninstall qwen` commands
- `tiktoken-rs` integration (cl100k_base) for exact token counting (feature-flagged `exact-tokens`, falls back to heuristic by default)
- Co-authored by Qwen-Coder for Alibaba Cloud Qwen-Coder ecosystem support

### Fixed
- TUI cleanup order: `disable_raw_mode` now executed after `LeaveAlternateScreen` to prevent terminal corruption
- Clear terminal and restore cursor around symbol selection view

### Docs
- Added Product Hunt featured badge to README (#30)
- Fixed malformed del tags in auto-watch section of README (#29)

## [0.7.0] - 2026-03-22

### Added
- C/C++ indexing support: `.c`, `.h`, `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hh`, `.hxx` files are now indexed and watched
- Symbol extraction for C/C++: functions, structs, enums, typedefs, classes, and namespaces via `tree-sitter-c` and `tree-sitter-cpp`

## [0.6.0] - 2026-03-21

### Added
- `ecotokens auto-watch enable|disable` command — starts/stops `watch --background` automatically on Claude Code session open/close (Claude Code only)
- Multi-watcher support: one independent watcher per watched directory, tracked in `sessions.json`
- Parallel Claude sessions on the same path share a single watcher; sessions on different paths get independent watchers
- Per-path log files (`watch_<sanitized_path>.log`) replacing the single `watch.log`
- Log files are deleted automatically when their watcher stops
- "Since N days" indicator in gain stats panel (#22)

### Changed
- Replaced `BackgroundState` / `watch-bg.json` with `SessionStore` / `sessions.json` for watcher state management
- `watch --stop` and `watch --status` now operate on all active watchers (or a specific path via `--path`)
- Negative token gains (filter output larger than raw input) now fall back to masked raw content, eliminating `--X%` display artifacts
- Gain history displays `+X%` instead of `--X%` for negative savings

### Fixed
- Filter pipeline no longer records negative savings: if `tokens_after > tokens_before`, masked content is returned instead
- Scroll in Diff mode and history_scroll clamped to content bounds (#24)

### Docs
- Reorganized assets and docs into dedicated directories (#21)
- Added Benchmarks section with key indicators to README (#20)
- Added CHANGELOG and BENCHMARKS files (#19)

## [0.5.0] - 2026-03-19

### Added
- `ecotokens clear` command to reset metrics, with `--project "(unknown)"` support (#16)

### Changed
- Updated `Cargo.toml` description to mention Gemini CLI
- Added `gemini` keyword for crates.io discoverability
- Limited keywords to 5 for crates.io compliance (#18)

### Docs
- Added CI, crates.io version and MIT license badges to README
- Added Contributing section linking to `CONTRIBUTING.md`

## [0.4.0] - 2026-03-17

### Added
- `--history` flag for the `gain` command (#15)

### Fixed
- Period filtering in the TUI gain view (#15)
- Family savings now computed from token counts instead of averaging percentages

## [0.3.0] - 2026-03-14

### Changed
- Removed MCP server and VS Code support — CLI-only architecture (#12)

### Docs
- Clarified CLI-only architecture in hook-flow documentation (#13)

## [0.2.0] - 2026-03-14

### Added
- Code duplicate detection tool (#11)
- Banner and project images in README (#10)
- Demo GIF in README (#9)
- Quick install and hook flow guide in README (#8)

## [0.1.0] - 2026-03-12

### Added
- Initial release: hook-based token filtering for Claude Code and Gemini CLI
- Specialized filters for git, cargo, python (pytest, pip, ruff, uv), C/C++
- TUI dashboard (`ecotokens gain`) with sparkline and by-project view
- Vector search with embeddings (Ollama / LMStudio)
- `ecotokens install` / `uninstall` / `config` commands
- MIT license

[0.13.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.13.0
[0.12.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.12.0
[0.11.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.11.0
[0.10.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.10.0
[0.9.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.9.0
[0.8.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.8.0
[0.7.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.7.0
[0.6.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.6.0
[0.5.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.5.0
[0.4.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.4.0
[0.3.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.3.0
[0.2.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.2.0
[0.1.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.1.0
