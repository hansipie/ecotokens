# Study: show the model used at that instant for each filtered row in `ecotokens gain`

Status: study only, nothing implemented. Line numbers come from outlines and are approximate (±5). Points marked **NOT VERIFIED** must be checked before implementation.

## 1. Current state of the gain TUI

- **File:** `src/tui/gain.rs` (~1400 lines). Entry point `render_gain` (~l.202). Modes: `GainMode` Family/Project (l.30), `DetailMode` Details/Diff/SplitRaw (l.22), `SparklineMode`.
- **Data:** a `Vec<Interception>` fully loaded by `metrics::store::read_all()`, sorted by `timestamp ASC`. There is no per-row query. Filtering happens in Rust (`matches_project` l.93, family filter ~l.150-190, `aggregate*` in `src/metrics/report.rs`).
- **Filtered rows list:** the "log panel" (`render_log_panel` l.1249, then `render_log_panel_inner` l.634). It filters on the selected family or project, reverses the order, then applies scroll and selection (`log_selected` / `log_scroll`).
- **Row rendering:** plain text, no ratatui `Table` or columns (~l.1313-1319):
  `format!("{ts:<16}  {cmd:<30}  {:>6} → {:>6}  {sign}{:.1}%")`
  `ts` is cut to 16 characters, `cmd` goes through `truncate_cmd(command, 30)` (l.830). No width is computed: an overlong line is cut by the terminal.
- **Detail panel** (`render_detail_inner` l.700, `render_diff_panel` l.957): the best place for a long value such as the full model name.
- **Aggregated report:** `Report` (`report.rs:42`) has `by_family`, `by_project`, `by_agent` (via `HookType::agent_label`, `store.rs:43`). There is no `by_model`. `gain --json` serialises `Report`. No per-row JSON output exists.

## 2. What is stored today

- **Struct `Interception`** (`src/metrics/store.rs:149`): id, timestamp, command, command_family, git_root, tokens_before/after, savings_pct, mode, redacted, duration_ms, content_before/after, hook_type. The last three have `#[serde(default)]`. There is no `session_id` and no `model`.
- **SQLite:** table `interceptions` in `~/.config/ecotokens/metrics.db`, schema in `create_schema` (`store.rs:240`).
- **Migrations:** there is **no** schema versioning (no `PRAGMA user_version`) and no `ALTER TABLE`, only `CREATE TABLE IF NOT EXISTS` and the JSONL→SQLite migration (`migrate_from_jsonl_if_needed`, l.291). Adding a column needs a conditional `ALTER TABLE interceptions ADD COLUMN model TEXT` (checked via `PRAGMA table_info`) in `open_conn` (l.267), guarded by `DB_INIT_LOCK` and `busy_timeout` (already there for multi-process races, see v0.13.1).
- **Three places to change:** the hard-coded SELECT in `read_from` (l.418), `row_to_interception` (l.345) and `insert_one` (l.377, 14 placeholders).
- **Write points** (all via `Interception::new(...)` + `.with_hook_type(..)` + `append_to`):
  - `src/filter/mod.rs:299` and `:440` (Bash filter pipeline, run as an `ecotokens filter` subprocess by `cmd_filter`, `main.rs:596`).
  - `src/hook/post_handler.rs:281` (`record_post_metrics`, PostToolUse of native Read/Grep/Glob tools, plus Gemini/Qwen/Codex).
  - `src/rewrite/mod.rs:540` (local rewrite, outside savings aggregates).
  - Pi, OpenCode, Hermes use TS/Python extensions that call back `ecotokens filter` or a subcommand with `--hook-type`.
- **Decisive architecture point:** for Bash, the PreToolUse hook (`src/hook/handler.rs:327 handle_with_agent`) reads the stdin JSON but only extracts `tool_input.command` and `cwd`. It rewrites the command to `ecotokens filter --agent <a> [--cwd <c>] -- bash -c '<cmd>'`. The metric is written later by another process that does **not** see the hook payload. Any session data (model, session id) must therefore travel in the rewritten command (new flag), through a session state file/DB, or be recovered at display time. Any new flag must be peeled by `parse_wrapper` / `peel_ecotokens_wrapper` (`handler.rs` l.110-194), otherwise wrappers stack up (regression class of #126). `PostHookInput` (`post_handler.rs:69`) only reads tool_name, tool_input, tool_response, cwd.

## 3. Where "the model at that instant" can come from

**In the repo (verified)**
- No notion of the session model in metrics or hooks. `src/router/*` (`model_alias`/`model_label`) only maps a Jev size (tiny/everyday/large/hardest) to an alias; it never reads the effective model. It is a *suggested* model, not a *used* one.
- `src/handoff/hook.rs` and `handoff/mod.rs` already read `session_id` and `transcript_path` from the SessionStart JSON, and `handoff/transcript.rs::extract` already parses the JSONL. Reusable infrastructure (per-session JSON records under `~/.config/ecotokens/handoff/sessions/<id>.json`).

**Claude Code**
- **Transcript JSONL (verified on a real local file):** each `type:"assistant"` line carries `message.model` (e.g. `claude-opus-5-5` and `claude-haiku-5-5` in the same session), plus `timestamp`, `sessionId`, `isSidechain`, `requestedModel`, `advisorModel`, `effort`, `cwd`. Most reliable and precise source; mid-session switches and sub-agents are visible. Field names may change between Claude Code versions, so parse defensively.
- **Hook payload:** `session_id`, `transcript_path`, `cwd` are present (verified by existing code). **NOT VERIFIED:** a `model` field in the SessionStart payload. If it exists it only gives the initial model, not `/model` switches. The statusline payload likely has `model.id`/`display_name` (**NOT VERIFIED**).
- **Env vars / settings.json:** `ANTHROPIC_MODEL` and the `model` key give configuration, not the effective model (aliases, `/model`, auto-routing, sub-agents). Do not use as truth. **NOT VERIFIED** that Claude Code exports the model to hook processes.

**Other harnesses**
- **Hermes:** the generated Python script has `on_session_start(session_id, model, platform)` and `on_session_end(..., model, ...)` (verified), but not in `transform_terminal_output`/`transform_tool_result` (they receive `**_`, model possibly available there, **NOT VERIFIED**). The model would have to be remembered at session start in shared state.
- **Pi, OpenCode, Codex, Gemini, Qwen:** no model usage in the repo. Pi (`pi.on("session_start", (event, ctx))`) and OpenCode probably expose the model through their context (**NOT VERIFIED**). Codex/Gemini/Qwen hook payloads must be inspected (**NOT VERIFIED**). `NULL` / `-` until wired.

## 4. Design options

### A. `model` column written at write time (+ `ALTER TABLE` migration)
- Pros: trivial and instant display, works even if the transcript is deleted or moved, exposable in JSON and aggregates, truly "at that instant".
- Cons: the model must reach the `filter` subprocess (new flag, or `filter` reads the transcript, which needs `session_id`/`transcript_path`); a first real schema migration is needed; insert/select/row mapper/struct change; Pi/Hermes/Codex paths.
- Parsing cost: read the last few KB of the JSONL (seek from the end, last non-sidechain assistant line). Stay within the hook latency budget, bound the read and fail silently.

### B. Store only `session_id` (+ `agent`), resolve the model at display time from the transcript
- Pros: no parsing in the hook, most precise a posteriori (mid-session switches and sub-agents).
- Cons: a deleted transcript (Claude Code cleanup) gives `-`, the TUI must locate and parse potentially large JSONL files (per-session index/cache), Pi/Codex/Hermes have no equivalent transcript, and a schema column is needed anyway.

### C. Hybrid (recommended)
`model` and `session_id` columns written on the fly, with a small "current session model" cache (JSON file in the style of `handoff/sessions/<id>.json`).
- SessionStart, or the first hook of the session, stores the model (payload field if present, else the last assistant model in the transcript). Each PreToolUse/PostToolUse refreshes it by reading the bounded transcript tail if the file is newer than the cache.
- The value reaches the `filter` subprocess through a flag or an env var (`ECOTOKENS_MODEL`), or `filter` re-reads the cache via `--session`.
- Old rows stay `NULL` and display `-`. The TUI can offer an optional backfill via B.

### Common trade-offs
- **Accuracy:** a transcript gives the model of the last assistant response, not necessarily the one that issued the current tool call. Sub-agents (`isSidechain:true`) run on another model (haiku in the sample), so we must decide whether to follow the sidechain or the main thread, and the hook does not always say which agent made the call (**NOT VERIFIED**). A `/model` switch between two calls is caught with a cache freshness of about a second.
- **Privacy:** a model name is not a secret and stays on the machine (local `metrics.db`). Check that `masking` does not alter it. Do not copy transcript content; store only the id, never a transcript path.
- **Backward compatibility:** `#[serde(default)] pub model: Option<String>` in `Interception` plus a conditional schema migration. Legacy JSONL and old rows give `None`, shown as `-`.

### TUI impact
- **Decision: display the model only in the Details panel** (`render_detail_inner`, `src/tui/gain.rs` ~l.700), with the full name (e.g. `claude-opus-5-5`), as a "Model" line next to the other fields of the selected row. The log panel row (~74 columns) is left untouched: no new column, no width computation, no narrow-terminal handling. A row without a model shows `-`.
- Navigation: a key to filter/group by model (like `GainMode::toggle`, new mode or cycle) and a `by_model` line in the report. `Report` is deserialised with `#[serde(default)]`, so adding `by_model: HashMap<String, FamilyStats>` with `#[serde(default)]` stays compatible with existing JSON consumers but changes `gain --json` output (document it, add a regression test).
- Empty state: check rendering on empty data (project rule n°13).

### Tests to add (per `docs/TEST-PLAN.md`, sections E and H; existing tests in `tests/metrics/{store_test,report_test}.rs` and `tests/tui/gain_test.rs`)
- **E:** `Interception` round trip with/without `model`; opening an existing DB without the column (idempotent `ALTER TABLE`, concurrent open); legacy JSONL without `model`; `aggregate` with `by_model` (old rows in a `-`/`unknown` bucket); `gain` vs `gain --json` consistency; unchanged ordering and totals.
- **H:** detail panel rendering with model present/absent (`-`); log panel row unchanged; empty data; navigation without regression (selection and scroll).
- **Hook:** transcript parsing (last assistant line, sidechain, truncated/absent/huge file, invalid JSON); new flag peeled correctly by `parse_wrapper` (no wrapper stacking); no secret leak.
- **Project rules:** `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`; sections B/G if `src/filter/*`, C/D/G if `src/hook/*`, E/H for metrics and TUI.

### Docs to update (in English)
`docs/TUI.md` (columns/keys), `docs/hook-filter-metrics-flow.md` (metric flow and model provenance), README if `gain --json` changes, `docs/harness-feature-matrix.md` (model support per harness), `docs/model-router.md` (suggested vs used model), changelog.

## 5. Recommendation and estimate

Approach C, in stages:
1. **Claude Code only** (top project harness priority): `ALTER TABLE ADD COLUMN model TEXT` migration + `Option<String>` in `Interception`; a bounded, error-tolerant `current_model(transcript_path)`; PreToolUse/PostToolUse read it and pass it to `filter` (new flag peeled by `parse_wrapper`); full model name shown in the Details panel only (log panel unchanged).
2. `by_model` in `Report` / `gain --json`, model filter in the TUI.
3. Hermes (model available at `on_session_start`), then Pi/OpenCode/Codex after checking their payloads.
4. Optional: backfill old rows from transcripts.

**Rough estimate:** stage 1 ≈ 1 to 1.5 days (schema + migration + hook + flag + TUI + E/H and hook tests + docs); stage 2 ≈ 0.5 day; stage 3 ≈ 0.5 day per harness; backfill ≈ 0.5 to 1 day.

**Risks**
- (a) No schema migration exists yet, so the first `ALTER TABLE` is the most delicate point (multi-process races, old databases, rollback to an older version reading a wider schema; the explicit SELECT in `read_from` protects the latter).
- (b) Added latency in the PreToolUse hook (transcript read, must be bounded).
- (c) The transcript format is not contractual: parse defensively, default to `-`.
- (d) Regression trap in `peel_ecotokens_wrapper` (see #126).
- (e) "Model at that instant" is approximate with sub-agents and `/model`.
- (f) The NOT VERIFIED points above (SessionStart `model` field, Codex/Gemini/Qwen/Pi/OpenCode payloads, model in Hermes transform hooks): capture a real payload sample before choosing between a payload field and the transcript.

## Key files
`src/tui/gain.rs`, `src/metrics/store.rs`, `src/metrics/report.rs`, `src/filter/mod.rs`, `src/hook/handler.rs`, `src/hook/post_handler.rs`, `src/main.rs`, `src/handoff/hook.rs`, `src/handoff/transcript.rs`, `src/router/mod.rs`, `src/install.rs`.
