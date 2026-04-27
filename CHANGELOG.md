# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.16.0] - 2026-04-26

### Added

- **Configurable embedding model**: `--embed-model` CLI flag lets you choose any model supported by your provider (e.g. `mxbai-embed-large`, `all-minilm`). The model can be set independently of the provider with `ecotokens config --embed-model <name>`. Default values are preserved (`nomic-embed-text` for Ollama, `nomic-embed-text-v1.5` for LM Studio); existing configs without a `model` field are automatically upgraded to the default on load.
- **Gemini CLI AfterTool hook**: intercepts `read_file`, `search_file_content`, and `list_directory` tool results via the `AfterTool` hook using deny+reason substitution — routes through the existing read/grep/glob handlers for the same token savings as Claude Code
- **Qwen Code PostToolUse hook**: intercepts `read_file`, `search_files`, and `list_dir` tool results via the `PostToolUse` hook using `additionalContext` — same compression pipeline as above
- `ecotokens install --target gemini` and `--target qwen` (and `--target all`) now also install the respective post-hooks; `uninstall` removes them
- **`search` — line numbers + context around the match**: results now show `file:line (score)` where the line number points to the first line matching a query term inside the chunk, with `--context N` lines of context above and below (default: 2) instead of the chunk start ([#62](https://github.com/hansipie/ecotokens/issues/62))
- **`search --include` / `--exclude`**: glob flags to restrict or skip results by file type (e.g. `--include "*.rs"`, `--exclude "*.md"`); repeatable; no new dependency ([#63](https://github.com/hansipie/ecotokens/issues/63))
- **`search` — automatic trace augmentation**: when the query matches a symbol name, callers are automatically appended after the BM25 results; opt out with `--no-trace`; JSON output includes a `"callers"` array ([#64](https://github.com/hansipie/ecotokens/issues/64))
- **`search` — project scoping**: when using the global index (no explicit `--index-dir`), results are silently restricted to files that exist under the current git root, preventing hits from other indexed projects

### Changed

- **`search`** — duplicate chunks (same file, same 50-line window, score delta < 0.5) are now deduplicated before display

## [0.15.2] - 2026-04-24

### Fixed

- Hook rewrite: commands containing shell builtins or operators (e.g. `cd /path && …`, `source .env`) no longer fail with `ENOENT` — the rewritten command is now wrapped as `ecotokens filter -- bash -c '…'` so shell builtins, `&&`, `||`, pipes and redirections are all handled by a shell rather than by direct `exec` ([#65](https://github.com/hansipie/ecotokens/issues/65))
- Hook rewrite: commands that redirect output to a file (e.g. `cmd --json > /tmp/out.json`) no longer have ecotokens annotation text injected into the redirect target — `bash -c` handles the redirection internally so ecotokens stdout never reaches the file ([#52](https://github.com/hansipie/ecotokens/issues/52))
- Hook rewrite: heredoc commands (e.g. `python3 << 'EOF'\n…\nEOF`) no longer produce silent empty output — the heredoc stdin was previously consumed by `ecotokens filter` instead of the target interpreter; `bash -c` now handles the heredoc internally ([#54](https://github.com/hansipie/ecotokens/issues/54))

## [0.15.1] - 2026-04-22

### Fixed

- `ecotokens filter`: commands invoked via absolute path (`/usr/bin/git`), venv (`.venv/bin/pytest`), version managers (`~/.cargo/bin/cargo`) or wrappers (`poetry run`, `pipx`) no longer fall back to `Generic` — basename is now extracted from the first token before family matching, aligning with the existing `is_cpp_command()` behaviour
- Added `poetry`, `pipx` → Python ; `jest`, `mocha`, `yarn` → JavaScript family mappings
- Search: results are now restricted to BM25 chunk docs only — symbolic index entries are excluded via a `BooleanQuery` `Must kind=bm25` filter, preventing spurious hits from non-textual documents

## [0.15.0] - 2026-04-17

### Added

- Word abbreviation pass for narrative text, logs and tool-result messages — replaces full words with shorter forms (e.g. `function`→`fn`, `configuration`→`config`, `directory`→`dir`) after masking and family filtering to squeeze extra tokens out of every interception
- Built-in dictionary of 41 safe pairs; users can extend or override via `~/.config/ecotokens/abbreviations.json` (separate from `config.json` which keeps only the feature flag)
- `SessionStart` hook now injects an `additionalContext` instruction listing the active dictionary when abbreviations are enabled, nudging the model to adopt the same abbreviations in its own responses
- New CLI: `ecotokens abbreviations enable | disable | list`
- `ecotokens config` output now reports `abbreviations_enabled`
- New doc: `docs/abbreviations-pipeline.md` — describes the three trigger points (SessionStart, filter cmd, hook-post)

### Changed

- `cmd_session_start` auto-watch logs routed to stderr so stdout stays reserved for the hook JSON payload
- Custom abbreviation pairs moved from `abbreviations_custom` key in `config.json` to a dedicated `~/.config/ecotokens/abbreviations.json` file; legacy `config.json` entries are migrated automatically on next `enable`
- TUI gain panel: `render_detail` and `render_project_detail` refactored into shared `render_detail_inner` to eliminate duplication

### Fixed

- AI summary: structured JSON output (objects and arrays) is now preserved as-is instead of being replaced by a natural-language summary — fixes automation breakage when CLI commands return large JSON payloads (e.g. `--json` flags) above the `ai_summary_min_tokens` threshold (#53)
- Integration tests that assert on filtered output strings now isolate `HOME`/`XDG_CONFIG_HOME` to avoid abbreviation side-effects (e.g. `cpp_test`)

## [0.14.5] - 2026-04-15

### Fixed

- `ecotokens watch`: when a session starts in a subdirectory already covered by an existing watcher, the parent watcher is reused instead of launching a duplicate background watch — a clear skip message is surfaced instead

## [0.14.4] - 2026-04-08

### Fixed

- `ecotokens filter`: commands run outside a git repository (e.g. `cat`, `g++` in `/tmp`) no longer appear under `[undefined]` in the gain dashboard — the working directory is used as project key when `git rev-parse` finds no repo
- `ecotokens filter`: when `--cwd` is not passed (hook without cwd context), falls back to the process working directory for git root detection

## [0.14.3] - 2026-04-08

### Fixed

- Hook handler: `cwd` field from hook input and `ShellToolPayload` is now forwarded to `ecotokens filter --cwd` so project association works correctly for all hook sources

## [0.14.2] - 2026-04-05

### Fixed

- Pi extension: bash commands now pass `--cwd` to `ecotokens filter`, enabling correct `git_root` detection and project association in `ecotokens gain`
- `ecotokens filter` now accepts `--cwd <path>` to set the working directory for git root detection
- `ecotokens gain`: interceptions with `git_root = None` (e.g. bash commands without cwd context) now appear in the `[undefined]` project bucket instead of being silently dropped from the project view

## [0.14.1] - 2026-04-05

### Fixed

- Pi extension: `read` tool results now correctly processed — `tool_response.output` and `tool_response.content` accepted as fallbacks to `tool_response.file.content`
- Pi extension: `read` tool input field `path` accepted as fallback to `file_path` for file path extraction and metrics command naming
- Pi extension: `find` tool results (mapped to Glob) now correctly processed — `tool_response.output` accepted as fallback to `tool_response.filenames`
- Without these fixes, `NativeRead` and `Fs` interceptions from Pi were silently dropped and never visible in `ecotokens gain`

## [0.14.0] - 2026-04-05

### Added

- Pi coding agent support via TypeScript extension installed at `~/.pi/agent/extensions/ecotokens.ts`
- `bash` tool calls intercepted in-process: `event.input.command` rewritten to pipe through `ecotokens filter` (équivalent au PreToolUse de Claude Code)
- `read`/`grep`/`find`/`ls` tool results piped through `ecotokens hook-post` pour compression outline-based (équivalent au PostToolUse)
- `session_start` / `session_shutdown` hooks wired to auto-watch lifecycle
- `/gain` et `/eco-search` slash commands exposés via `registerCommand`
- `ecotokens install --target pi` et `ecotokens uninstall --target pi`
- Extension source embarquée dans le binaire via `include_str!("pi_extension.ts")`

## [0.13.1] - 2026-04-05

### Fixed

- `CONN_INIT_LOCK` (`OnceLock<Mutex>`) sérialise `open_conn` pour éviter `SQLITE_BUSY` sur `PRAGMA journal_mode=WAL` lors de migrations concurrentes
- `read_to_string` traite désormais `NotFound` comme "déjà migré" pour gérer la fenêtre TOCTOU entre le test `migrating_path.exists()` et la lecture effective

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

