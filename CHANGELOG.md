# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.25.2] - 2026-07-29

### Changed

- **Model catalog**: refreshed the built-in Anthropic, OpenAI, and Google models and their pricing; added `claude-fable-5`, replaced the GPT entries with the `gpt-5.6` family, and updated the Gemini 3 models.
- **Default model**: changed the model used for cost calculations from `claude-sonnet-4-6` to `claude-sonnet-5`.
- **Version**: bumped the crate to `0.25.2`.

### Fixed

- **Project-scoped duplicate detection**: `ecotokens_duplicates` now excludes symbols from unrelated repositories stored in the shared index and only reports duplicate groups from the caller's current project.

## [0.25.1] - 2026-07-29

Follow-up to the 0.25.0 review: a second full-codebase pass that found the native-tool masking gap left open by the previous round. All fixes ship with `cargo fmt`, `cargo clippy -- -D warnings`, and the full test suite green.

### Security

- **Native tool masking**: `Read`/`Grep`/`Glob` results reached the model and the metrics store completely unmasked — unlike the Bash path, no code path applied `masking::mask` to them. A `Read` of a `.env`/`.pem` file, or a `grep` matching a credential line, leaked verbatim into both the injected context and the persisted `content_before`. Masking now runs at the post-hook dispatcher, the single choke point for those tools (and, for Gemini, whose `deny` + `reason` replaces the tool result outright, this keeps the secret out of the model's context entirely).
- **MCP path traversal**: `ecotokens_outline` built a path straight from client input with no containment check, so `{"path": "/etc"}` or `../../../../.ssh` returned file contents from outside the indexed project. Paths are now canonicalized (resolving `..` *and* symlinks) and rejected unless they sit under the project root, matching the scoping `ecotokens_search` already applied.
- **Debug log**: logged hook payloads are masked per JSON string (masking the serialized line would let `[^\s\n]+` patterns run past a closing quote and corrupt neighbouring fields), and the file is created `0600` — a pre-existing world-readable log is tightened on next write.
- **Codex stderr**: `codex_bash_output_text` read only `output`/`stdout`, so a secret surfacing solely on stderr never entered `run_filter_pipeline_with_cwd` — the only place masking runs for that agent — and was never redacted.
- **Debug tracing**: `--debug` printed the original command to stderr before any masking; both the original and the rewritten form (which embeds it shell-quoted) are now masked, closing the one path where a secret passed as a literal argument could leak.

### Fixed

- **Search crash on `top_k=0`**: tantivy's `TopDocs::with_limit` asserts `limit >= 1`, so `--top_k 0` (or an MCP client sending `top_k: 0`) panicked and took down the whole process, including the long-lived MCP server. Returns an empty result set instead, which is what the final truncation produced anyway.
- **Game crash on short terminals**: `group_bounds()` returned the raw playfield when no formation enemy was alive, and `field()` has zero height on a terminal two rows tall — `spawn_snake`'s modulo then divided by zero. Both extents are now clamped to the same minimum the computed branch already guaranteed.
- **`ecotokens clear` data loss**: pruning read every row, filtered in memory, then replaced the whole table — so any interception appended by a concurrent session between the read and the write (a window spanning the interactive confirmation prompt) was silently destroyed. Rows are now deleted by primary key via `delete_ids`, leaving concurrently written rows untouched.

### Changed

- **Semantic search (HNSW)**: the 0.25.0 `OnceLock` only helped a *reused* index instance, but `search_index()` reloaded from disk on every call — so the MCP server, which handles one call per request, still deserialized the vector file and rebuilt the graph on every single query. Indices are now cached per process, keyed on index directory and invalidated on the mtime of `hnsw_index.bin`.
- **Docs**: `README.md` and `docs/hook-filter-metrics-flow.md` now state that excluding a command opts out of secret masking as well as filtering (ecotokens never sees that output, so it cannot mask it), and that exclusions match by prefix.
- **Version**: bumped the crate to `0.25.1`.

## [0.25.0] - 2026-07-11

Broad code-review hardening pass across the codebase. All fixes ship with `cargo fmt`, `cargo clippy -- -D warnings`, and the full test suite green.

### Added

- **`ecotokens game`**: a Space Invaders mini-game where every filtered command spawns an enemy whose strength scales with the tokens it saved. Commands present at startup form the classic marching formation; commands filtered live while the game runs spawn as free-roaming snakes.

### Security

- **Secret masking**: fixed five broken patterns in `src/masking/patterns.rs` that let real secrets leak through unmasked — AWS access keys now use `[A-Z0-9]{16}` (was base32 `[A-Z2-7]`), Anthropic API keys drop the unstable `AA` suffix anchor, HuggingFace tokens accept variable length (`{34,}`), Bearer tokens keep `=` mid-token, and `.env` matching allows spaces around `=`. Added word boundaries to the Twilio pattern and relaxed the Azure AD client-secret 4th-character constraint to cut false negatives/positives.
- **Self-update**: `ecotokens update` now passes a version reconstructed from parsed integers to `cargo install --version`, so a spoofed GitHub API response cannot inject arguments.
- **Watcher teardown**: `session-end` verifies a stored watcher PID still belongs to an ecotokens process (via `/proc/<pid>/cmdline`, `ps` fallback) before sending `SIGTERM`, avoiding killing a recycled PID.
- **Model cache**: `CandleProvider` sanitizes user-supplied model IDs into a single safe path component (neutralizing `..` and other path-unsafe characters) before building the cache directory.

### Fixed

- **Semantic search (HNSW)**: the ANN graph is now built once per index instance (cached in a `OnceLock`) instead of being rebuilt on every `search()` call, and a dimension guard rejects heterogeneous/zero-length vectors instead of feeding them to `DistCosine`.
- **`outline` traversal**: directory walking now uses the `ignore` crate, so it no longer follows symlink cycles into a stack overflow and respects `.gitignore` (no more descending into `node_modules`, `.git`, build artifacts).
- **`trace` line numbers**: `find_callers`/`find_callees` report real file line numbers (offset by the symbol's start line) instead of a 0-based index into comment-stripped source; added an identifier-boundary check so short symbol names (`f`, `get`) no longer produce false-positive matches.
- **Duplicate detection**: union-find uses iterative path compression plus union-by-size (no stack overflow on large inputs), and a length-based pre-filter skips pairs that provably cannot reach the similarity threshold.
- **Crash safety**: `tokens/counter.rs` falls back to the character heuristic instead of `expect()`-panicking (and poisoning the static) when the exact tokenizer fails to load; `tui/gain.rs` no longer panics on zero-width columns or multi-byte path truncation.
- **Command filters**: corrected several dispatch/parsing bugs — `docker run psql-image` is no longer misrouted to the `ps` filter (exact-token matching), `cargo +nightly build` and absolute-path invocations are recognized, `gh pr list` reads `STATE` from the correct column, cargo-test failure sections reset between suites, and false-positive markers were tightened for cpp (`linking`), go subtests, vitest (`×`), and curl progress lines. Git status now surfaces merge-conflict/`typechange` entries; `diff` keeps git headers; markdown ToC skips headings inside fenced code blocks; `ls -l` symlink lines filter by entry name.
- **Hooks**: removed the permanently-dead symbol-enrichment path in the grep post-hook; `read` handler falls back to the target file's directory when the working directory was deleted; consistent newline framing on passthrough output.
- **Session store**: `cleanup_dead` keeps entries with live sessions during the `increment → register_watcher` window, and `is_pid_running` treats Linux zombie processes as not running.
- **Config integrity**: `install` refuses to overwrite settings files that contain invalid JSON (instead of silently replacing them with `{}`); `atomic_write` uses a per-process counter to guarantee unique temp names; `debug.log` rotates at 10 MB.
- **Metrics**: post-hook agent→hook-type mapping records dedicated `Gemini`/`Qwen`/`Codex` PostToolUse variants; date-bounded reports exclude items with unparseable timestamps; SQLite connections set `busy_timeout` for multi-process contention; `by_family` aggregation uses a stable `CommandFamily::as_str()` key.
- **Atomic writes**: the search module (`hnsw`, `index`, `embed`) now uses `atomic_write` for all index metadata files.

### Changed

- **MCP server**: settings are cached at construction instead of reloaded from disk on every tool call; numeric parameter deserialization uses `TryFrom` so out-of-range values error instead of silently truncating on 32-bit targets.
- **Embeddings**: L2 normalization clamps the norm away from zero to prevent NaN vectors from entering the index; the Ollama embedding client is reused per thread.
- **Watcher**: a debounce batch triggers at most one project reindex (instead of one per changed file), and log timestamps now include the date.
- **`.env`/legacy config**: only legacy externally-tagged embed providers migrate to Candle at load time — an explicit `"type": "none"` is preserved.
- **Version**: bumped the crate to `0.25.0`.

## [0.24.1] - 2026-06-29

### Changed

- **CLI completion**: refactored `Period` enum to derive `clap::ValueEnum` and `Default`, enabling shell completion support for the `--period` option (values: `all`, `today`, `week`, `month`).
- **Version**: bumped the crate to `0.24.1`.

## [0.24.0] - 2026-06-18

### Added

- **Semantic manifest**: added `semantic_manifest.json` to track `{mtime, chunk_ids[]}` per file, enabling exact pruning of orphaned HNSW vectors during incremental reindexing. Includes one-time bootstrap for existing indexes without a manifest.

### Changed

- **Model pricing**: updated pricing for `gpt-5.5` ($2.50→$5.00 input, $10.00→$30.00 output), `gemini-2.5-flash` ($0.10→$0.30 / $0.40→$2.50), `gemini-3-flash-preview` ($0.10→$0.50 / $0.40→$3.00), and `mistral-medium-3.5` ($0.50→$1.50 / $1.50→$7.50); `docs/models.md` resynced with the 12 models currently defined.

### Fixed

- **User config**: added `atomic_write()` (temp file + rename) in `src/config/mod.rs` and switched `settings.rs`, `session_store.rs`, and `install.rs` to use it, preventing config file truncation on crash mid-write.

## [0.23.1] - 2026-06-19

### Added

- **Semantic index**: added `semantic_manifest.json` to precisely track `chunk_ids` per file and remove orphaned HNSW vectors during incremental reindexing.

### Changed

- **Version**: bumped the crate to `0.23.1`.
- **Model pricing**: updated pricing for `gpt-5.5`, `gemini-2.5-flash`, `gemini-3-flash-preview`, and `mistral-medium-3.5`; `docs/models.md` is resynced with the models actually defined.
- **Watcher / indexing**: `watch_directory` now receives `EmbedProvider` explicitly and delegates to `index_directory` through `IndexOptions`, removing ad hoc schema and symbol writes.
- **Install / uninstall**: normalized CLI output per target with grouped sections (`Install ...`, `Uninstall ...`) and aligned `ok`, `removed`, `skip`, and `note` rows.
- **Documentation**: the README now shows the grouped output from `ecotokens install` and `ecotokens uninstall`.

### Fixed

- **User config**: replaced non-atomic `fs::write` calls with `atomic_write` in settings, the session store, and installation code to avoid truncating config files if the process stops during a write.
- **Codex PostToolUse**: the bash post-hook now accepts Codex responses as direct JSON strings while preserving compatibility with legacy object fields `output` and `stdout`.

## [0.23.0] - 2026-06-15

### Added

- **Ollama embedding provider**: new `ollama` provider for semantic search that delegates embedding computation to a local or remote Ollama instance via `POST /api/embeddings`; compatible with all Ollama models including `qwen3-embedding:latest` (2560 dim), `nomic-embed-text`, etc.
  - `ecotokens config --embed-provider ollama` enables the provider with the default model `qwen3-embedding:latest`
  - `ecotokens config --embed-url URL` configures the base URL (default: `http://localhost:11434`)
  - `ecotokens config --embed-model MODEL` changes the model without changing the provider
  - Vectors returned by Ollama are L2-normalized automatically (Ollama does not normalize)
  - Silent failure if Ollama is unreachable -> BM25 fallback, consistent with the Candle provider
  - Provider or vector-dimension changes -> automatic HNSW index rebuild (existing behavior, now functional for Ollama)
- **`--embed-url`**: new CLI flag on `ecotokens config` to configure the Ollama provider URL

## [0.22.0] - 2026-06-15

### Added

- **`ecotokens doctor`**: new diagnostic command that checks the local setup without mutating any file — reports PATH availability, config readability, hook and MCP registration status for Claude Code, Gemini CLI and Qwen Code, and metrics database reachability; human-readable output by default, machine-readable with `--json` (closes #84, co-authored by @Chris79OG)
  - No hooks at all (`pre=false, post=false`) is classified as `Error`; partial hook presence as `Warning`
  - Gracefully handles a missing home directory for each agent path

### Fixed

- **Codex plugin manifest**: `shortDescription` and `longDescription` in `CODEX_PLUGIN_MANIFEST` incorrectly claimed a `SessionStart` hook was installed — corrected to reflect actual behaviour (PreToolUse/PostToolUse only); the auto-watch comment and user-facing message now accurately state that Codex has no `SessionEnd` equivalent, so the start/stop cycle cannot be completed (closes #97)

## [0.21.0] - 2026-05-29

### Added

- **Codex support**: `ecotokens install --target codex` installs a plugin in `~/.codex/plugins/ecotokens/` and registers the MCP server in `~/.codex/config.toml` under `[mcp_servers.ecotokens]` — Codex joins Claude Code, Gemini CLI, and Qwen Code with an automatically configured MCP server (auto-watch hook not implemented yet)
- **Hermes Agent support**: generated Python plugin in `~/.hermes/plugins/ecotokens/` via `ecotokens install --target hermes` — intercepts the `transform_terminal_output` and `transform_tool_result` hooks and calls `filter-output` as a subprocess
  - **`--enable-plugin`**: Hermes install flag that adds `ecotokens` directly to `plugins.enabled` in `~/.hermes/config.yaml` (no dependency on the `hermes` CLI) — creates the file if missing, preserves existing keys, idempotent
  - **Hermes auto-watch**: the plugin's `on_session_start` and `on_session_end` hooks automatically start and stop `ecotokens watch --background` for each Hermes session — same behavior as Claude Code and Qwen Code; enable with `ecotokens auto-watch enable`
  - **Per-family filtering for Hermes tools**: `hermes-tool:<name>` labels are automatically mapped to the appropriate filter family — `read_file`/`list_directory` -> `fs`, `search_files`/`find_files` -> `grep`, `browser_snapshot`/`web_fetch` -> `network`, `run_python_code` -> `python`, others -> `generic`
  - **Hermes plugin environment variables**: `ECOTOKENS_BIN`, `ECOTOKENS_HERMES_MIN_CHARS` (minimum threshold, default 2000 chars), `ECOTOKENS_HERMES_TIMEOUT` (subprocess timeout, default 10 s)
  - **Separate Hermes metrics**: `HermesTransformTerminalOutput` and `HermesTransformToolResult` types in `HookType` — the `--hook-type` flag on `filter-output` lets the plugin attribute them correctly; visible separately in `ecotokens gain`
- **`filter-output` subcommand**: new subcommand that reads captured tool output from stdin, applies filtering, and records metrics — enables post-hoc processing for agent outputs such as Hermes
- **Per-agent metrics**: new `by_agent` field in `Report` — metrics are aggregated by agent (`claude`, `gemini`, `qwen`, `pi`, `hermes`, `codex`, `cli`) in addition to the global total

### Changed

- **`model_pricing` externalized to `pricing.json`**: pricing is no longer serialized in `config.json`. Only user overrides (entries missing from or modified relative to the built-in catalog) are persisted in `~/.config/ecotokens/pricing.json`. Transparent migration: overrides present in an old `config.json` are automatically carried over on the next `save()`.
- **`filter-output`**: renamed the internal `returncode` parameter to `exit_code` for consistency with the CLI and Hermes plugin

### Fixed

- **`ecotokens gain`**: commands launched from a temporary path outside a git repository are now grouped under `[undefined]` instead of appearing as `/tmp/...` projects; git repositories created in a temporary directory remain attributed to their git root
- **`HnswIndex::search`**: replaced `parallel_insert` (non-deterministic, parallel threads) with sequential insertions — fixes the `hnsw_build_search_cosine` test that intermittently failed on `x86_64-unknown-linux-musl`

## [0.20.1] - 2026-05-11

### Fixed

- **Pi extension**: `spawnSync` used the process current directory instead of the watched project directory — `ctx.cwd` is now passed correctly to the `session-start` and `session-end` hooks
- **`auto-watch`**: the confirmation message now mentions Pi alongside Claude Code and Qwen Code
- Silent cleanup: removed four pre-existing compilation warnings (unused fields and imports)

## [0.20.0] - 2026-05-08

### Added

- **Shell completions**: new `ecotokens completions SHELL` subcommand — generates a native completion script for `bash`, `zsh`, `fish`, `powershell`, or `elvish` via `clap_complete`

### Changed

- **`ecotokens gain` — improved diff view**:
  - Visual BEFORE/AFTER header with an inline progress bar and savings percentage
  - Numbered section separators (`--- section 1/3  l.N ---`) replacing unreadable `@@ @@` markers
  - Automatic truncation of homogeneous sequences longer than 15 lines (`... +N lines omitted ...`) to avoid 800-line red diffs
  - New **SplitRaw** mode (`[d]` cycles `Details -> Diff -> SplitRaw`): 50/50 split panel view — BEFORE in red (o/l) / AFTER in green (Shift+O / Shift+L) — useful for radical transformations where the unified diff is noisy

## [0.19.0] - 2026-05-02

### Added

- **Semantic search (feature 009)**: dual BM25+vector retrieval with score fusion (`0.4 x BM25 + 0.6 x cosine`) — results now indicate their source through the `retrieval_source` field (`bm25` | `vector` | `both`)
- **`EmbedProvider::Candle`**: zero-config local embedding provider based on [Candle](https://github.com/huggingface/candle) — `sentence-transformers/all-MiniLM-L6-v2` model (384 dim) downloaded automatically via HuggingFace Hub; becomes the only embedding provider (replaces Ollama and LmStudio)
- **Optional GPU support for Candle**: building with `--features cuda` (NVIDIA) or `--features metal` (Apple Silicon) automatically enables GPU acceleration; CPU is used by default when no GPU feature is enabled or the device is unavailable
- **HNSW index** (`hnsw_index.bin`): ANN vector index persisted with bincode and rebuilt in memory on each search (< 1 s for < 20 k vectors); metadata stored in `hnsw_meta.json` (model, dimension, vector count, date)
- **Symbolic chunking**: Rust/Python/JS/TS/C/C++ files are split into tree-sitter symbol chunks (one function = one chunk); files without tree-sitter support fall back to 50-line windows
- **Incremental embedding**: unchanged chunks keep their vectors between indexing runs; only new or modified chunks are submitted to the provider
- **Model-change detection**: the HNSW index is automatically rebuilt when the embedding model changes, without rebuilding the BM25 index
- **Automatic migration** `embeddings.json` -> `hnsw_index.bin`: runs silently on first launch after the update

### Fixed

- **Watcher — `.gitignore` support**: `reindex_single_file` ignored `.gitignore` during incremental reindexing of modified files; files excluded by `.gitignore` are now ignored just like during full indexing

### Changed

- **`EmbedProvider`**: removed the `Ollama` and `LmStudio` variants — Candle is now the only embedding backend; existing configs with `"type": "ollama"` or `"type": "lm_studio"` are automatically migrated to Candle on load through the internal `Legacy` variant
- **CLI `ecotokens config`**: `--embed-provider` now accepts only `candle` and `none` (removed `ollama`, `lmstudio`); `--embed-url` is removed (Candle does not use an external service)
- `EmbedProvider`: the default variant changes from `None` to `Candle { model: "sentence-transformers/all-MiniLM-L6-v2" }` — existing configs without `embed_provider` automatically inherit the Candle provider
- `SearchResult`: added optional `line_end` and `retrieval_source` fields; `file_path` is now extracted directly from the tantivy document instead of derived from the chunk key

## [0.18.0] - 2026-04-30

### Added

- **Expanded LLM pricing**: price table expanded from 5 to 36 models covering Anthropic (Claude Haiku/Sonnet/Opus 4.x-4.7), OpenAI (GPT-4o, GPT-4.1, GPT-5, o1, o3, o4-mini), Google (Gemini 2.0/2.5), DeepSeek (V3, V4), Mistral (Large/Small), Meta Llama (3.3/4), and Alibaba Qwen (qwen3.5/3.6) — input and output prices per million tokens
- **`claude-haiku-4-5`**: updated price from 0.80 -> 1.00 $/1M input, 4.00 -> 5.00 $/1M output
- **`claude-opus-4-7`**: new model added (5.00 $/1M input, 25.00 $/1M output)
- **`ecotokens config --model MODEL`**: new CLI option to set the default model used in gain reports; shows the list of available models when the value is empty or unknown
- **`ecotokens config`**: now displays `default_model` in text output
- **`ecotokens gain`**: now uses `settings.default_model` as fallback (instead of the hardcoded `"sonnet"` constant)

## [0.17.0] - 2026-04-28

### Added

- **MCP stdio server**: exposes the search, outline, symbol, and trace engines as an MCP server (`rmcp`, stdio transport) via `ecotokens mcp-server`; `ecotokens install` automatically registers the server in `~/.claude/settings.json`
- Background logging is now conditional on the global `--debug` flag

### Fixed

- `ecotokens uninstall` now removes all ecotokens traces from `~/.claude/settings.json`: PreToolUse, PostToolUse, SessionStart, and SessionEnd hooks plus the MCP server entry
- Post-filter steps replace output only when the token count actually decreases; generic byte truncation respects UTF-8 boundaries; settings JSON parsing fallback is hardened; numeric MCP deserializers are deduplicated

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
- `bash` tool calls intercepted in-process: `event.input.command` rewritten to pipe through `ecotokens filter` (equivalent to Claude Code PreToolUse)
- `read`/`grep`/`find`/`ls` tool results piped through `ecotokens hook-post` for outline-based compression (equivalent to PostToolUse)
- `session_start` / `session_shutdown` hooks wired to auto-watch lifecycle
- `/gain` and `/eco-search` slash commands exposed via `registerCommand`
- `ecotokens install --target pi` and `ecotokens uninstall --target pi`
- Extension source embedded in the binary via `include_str!("pi_extension.ts")`

## [0.13.1] - 2026-04-05

### Fixed

- `CONN_INIT_LOCK` (`OnceLock<Mutex>`) serializes `open_conn` to avoid `SQLITE_BUSY` on `PRAGMA journal_mode=WAL` during concurrent migrations
- `read_to_string` now treats `NotFound` as "already migrated" to handle the TOCTOU window between the `migrating_path.exists()` check and the actual read

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
