# Terminal UI views

*Last updated: 2026-09-24*

ecotokens ships several interactive terminal views built with [ratatui](https://ratatui.rs). This page documents each one: how to launch it, what it shows, where the data comes from, and which keys it accepts. The source lives in `src/tui/`, and the entry points are the `cmd_*` functions in `src/main.rs`.

## Overview

| View | Command | Data source | Source file |
| --- | --- | --- | --- |
| [Gain dashboard](#gain-dashboard) | `ecotokens gain` | Metrics SQLite DB | `src/tui/gain.rs` |
| [Jev usage](#jev-usage) | `ecotokens jev` (or `v` in the gain dashboard) | Jev call log (SQLite) | `src/tui/jev.rs` |
| [Invaders game](#invaders-game) | `ecotokens game` | Metrics SQLite DB | `src/tui/game.rs` |
| [Outline](#outline) | `ecotokens outline <path>` | Source files (tree-sitter) | `src/tui/outline.rs` |
| [Trace](#trace) | `ecotokens trace callers\|callees <symbol>` | Tantivy index | `src/tui/trace.rs` |
| [Watch](#watch) | `ecotokens watch` | Tantivy index + file watcher | `src/tui/watch.rs`, `src/tui/progress.rs` |

### Common behavior

- **Interactive terminal only.** Every view opens on the alternate screen in raw mode, and only when stdout is a terminal. When stdout is piped or redirected, the same command prints plain text instead (or JSON with `--json`), and `ecotokens game` exits with an error. The terminal state is always restored on exit, including on panic.
- **Quit keys.** `q`, `Q`, `Esc` and `Ctrl-C` quit every view. Any other key is view-specific.
- **`--period`.** `gain`, `jev` and `game` accept `--period all|today|week|month` (default `all`). It restricts the data to the last 24 hours (`today`), 7 days (`week`), 30 days (`month`), or everything (`all`).
- **Live refresh.** The gain and Jev views reload their data every 10 seconds, and the game picks up new interceptions on the same cadence. The gain dashboard and Jev view show the last refresh time (UTC) in their title.
- **Empty data.** No view crashes on an empty store. Each one shows an explicit placeholder, described in its section.

## Gain dashboard

```bash
ecotokens gain                    # all time
ecotokens gain --period today     # also: week, month
```

Shows how many tokens ecotokens saved, grouped by command family or by project. The `--json` and `--history` flags bypass the TUI and print a report instead.

**Data source.** The metrics store at `~/.config/ecotokens/metrics.db` (SQLite), one row per intercepted command (`Interception`): command, family, project (`git_root`), tokens before/after, mode, duration, agent, and the raw and filtered content. See [hook-filter-metrics-flow.md](hook-filter-metrics-flow.md) for how rows are recorded.

### Layout

```
+------------------------------------------------------------+
| Stats: interceptions, days, tokens, savings, cost avoided   |  4 rows
+------------------------------------------------------------+
| By family (or By project): one gauge per entry              |
+------------------------------+-----------------------------+
| History (or Project history) | Detail / Diff / Before+After |
+------------------------------+-----------------------------+
| Savings sparkline (one column per day)                      |  4 rows
+------------------------------------------------------------+
```

1. **Stats panel.** Title `ecotokens gain - updated HH:MM:SS UTC`. Shows interceptions, days covered, tokens used, tokens saved, savings percentage and cost avoided (`n/a (run: ecotokens gain price --input <usd/Mtok>)` until a price is set). A yellow `Rewrite overhead` line appears when the automatic rewrite pipeline added tokens.
2. **Gauges.** One gauge per command family (`By family`) or per project (`By project`), sorted by savings percentage, highest first. The selected row is bold green with a `▶` marker. When the list does not fit, the title shows a `[position/total]` scroll hint.
3. **History panel** (bottom left). The interceptions of the selected family or project, newest first: timestamp, command (truncated to 30 characters), tokens before and after, and savings. The selected line is highlighted.
4. **Detail panel** (bottom right). The selected interception, rendered in one of three modes (see below). Without an explicit selection it shows the latest interception that actually changed the content.
5. **Sparkline.** Tokens saved per day, one column per day over the available width (at least 14 days). The scale is switchable.

### Modes

**Family vs. project view.** The dashboard starts in family view (global). `p` switches to project view, where each gauge is a git root (interceptions without a git root are grouped under `[undefined]`). In project view, `Enter` drills into the selected project: the view returns to families, filtered on that project, and the title reads `By family · project: <name>`. Pressing `p` again clears the filter and goes back to the global project list.

**Detail modes** (cycled with `d`: Details, then Diff, then Split, then back to Details):

| Mode | Content |
| --- | --- |
| Details | Full command, tokens before → after with percentage, project, mode (`filtered`, `passthrough`, `summarized`, `rewritten`), duration in ms, and the agent that triggered the hook. |
| Diff | `BEFORE` / `AFTER` token header with a reduction bar, then a unified line diff grouped in numbered sections (`section n/N  l.<line>`). Long pure additions or deletions (more than 15 lines) are cut to the first 5 and last 5 lines with an "omitted lines" marker. Binary content shows `Binary content — diff not available.` |
| Split | Two stacked panes: the raw content (`BEFORE`, red) and the filtered content (`AFTER`, green), each with its own scroll. |

**Sparkline scales** (cycled with `s`): `linear` (default), `log` (logarithmic, so a few large days do not flatten the rest) and `capped` (values clipped at the 90th percentile of non-zero days). The active scale is shown in the panel title.

### Keybindings

| Key | Action |
| --- | --- |
| `j` / `u` | Select the next / previous family (family view) or project (project view). Wraps around. Resets the history selection. |
| `k` / `i` | Move the selected line down / up in the History panel. The Detail panel follows the selection. |
| `o` / `l` | Scroll the Detail panel up / down. In Split mode this scrolls the BEFORE pane. |
| `O` / `L` (Shift) | Scroll the AFTER pane, in Split mode only. |
| `d` | Cycle the detail mode: Details → Diff → Split. |
| `s` | Cycle the sparkline scale: linear → log → capped. |
| `p` | From family view, switch to project view (clears any project filter). |
| `f` | From project view, switch back to family view. |
| `Enter` | In project view, open the selected project as a filtered family view. |
| `v` | Open the [Jev usage](#jev-usage) view. Quitting it (or pressing `v` again) returns to the dashboard. |
| `q`, `Q`, `Esc`, `Ctrl-C` | Quit. |

### Empty data

With no interceptions the gauge panel shows `No data yet.`. Before anything is selected the History and Detail panels show a hint (`j u: select a family`). If the selected family or project has no interception with a content difference, the Detail panel says `No interception with differences for this family.` (or `project`).

## Jev usage

```bash
ecotokens jev                     # all time
ecotokens jev --period week
ecotokens jev --json              # print the summary as JSON, no TUI
```

Shows how the optional [Jev integration](jev-integration.md) is being used: how many calls were made, how often ecotokens fell back to its local heuristic, latency, tokens and cost. From `ecotokens jev`, pressing `v` opens the gain dashboard. From the gain dashboard, `v` opens this view.

**Data source.** The Jev call log, a SQLite DB located by the `ECOTOKENS_JEV_DB` environment variable, or otherwise the model router database. Configuration facts (enabled state, URL) come from the ecotokens settings. An unreadable or missing log is treated as empty.

### Layout

```
+------------------------------------------------------------+
| Header: status, URL, calls, success, fallbacks, latency,    |  6 rows
|         tokens in/out, cost                                 |
+--------------------------+---------------------------------+
| By purpose (gauges)      | Recent calls (newest first)      |
|--------------------------|                                  |
| Failures (by error kind) |                                  |
+--------------------------+---------------------------------+
| Calls over time (sparkline)                                 |  4 rows
+------------------------------------------------------------+
```

- **Header.** `Status` is `enabled` (green), `enabled, TYPESAFE_API_KEY missing` (yellow) or `disabled` (grey). Jev counts as enabled when `jev_enabled` or `router_enabled` is set. Then come the total calls, successes, fallbacks to the heuristic (both with percentages), average and p95 latency, tokens in and out, and cost. Cost reads `n/a (no price configured)` when no price is set.
- **By purpose.** One gauge per purpose that has at least one call, sized by its share of total calls. Each row gives the call count, success rate, average latency and tokens in/out.
- **Failures.** Failure counts per error kind, most frequent first, or `No failure.` in green.
- **Recent calls.** One line per call: time, `ok` or `FAIL`, purpose, latency, tokens in/out, the target agent (`-> agent`) when the call came from the router, and the error kind with the HTTP status on failures. The selected line is shown in reverse video.
- **Calls over time.** A sparkline of call counts.

### Keybindings

| Key | Action |
| --- | --- |
| `j` / `Down` | Select the next call in the log. |
| `k` / `Up` | Select the previous call. |
| `l` / `o` | Scroll the log down / up without changing the selection. |
| `v` | Leave this view and open the gain dashboard (when launched from `ecotokens jev`) or return to it (when launched from the dashboard). |
| `q`, `Q`, `Esc`, `Ctrl-C` | Quit this view. |

The selected line always stays visible, whatever the manual scroll position.

### Empty data

With no call recorded for the period, the middle area is replaced by `No Jev call recorded for this period.` with a hint on how to enable Jev (`jev_enabled` plus `TYPESAFE_API_KEY`, or the model router). The header and timeline are still drawn.

## Invaders game

```bash
ecotokens game                    # all recorded commands
ecotokens game --period today
```

A Space Invaders mini-game fed by your own metrics. Each filtered command becomes an enemy, and the more tokens a command saved, the tougher its enemy. It requires an interactive terminal and exits with `ecotokens game requires an interactive terminal` otherwise.

**Data source.** The metrics DB, filtered by `--period`. Passthrough interceptions (no filtering) are skipped. The newest commands fill the first waves.

### Gameplay

- **Formation.** Enemies march in a grid of up to 10 columns and 4 rows per wave, with the strongest on the top rows. When a wave is destroyed, the next one is drawn from the queue. Destroying every queued command wins the game.
- **Enemy level** depends on the tokens the command saved:

  | Level | Sprite | Tokens saved | Hit points | Points |
  | --- | --- | --- | --- | --- |
  | 1 | ` ▿ ` | up to 999 | 1 | 100 |
  | 2 | `‹▼›` | 1,000 to 4,999 | 2 | 200 |
  | 3 | `<◆>` | 5,000 to 19,999 | 3 | 300 |
  | 4 | `«◈»` | 20,000 and more | 4 | 400 |

- **Color = command family** (git, cargo, fs, python, and so on). Damaged enemies are dimmed.
- **Snakes.** Commands filtered while the game is running (picked up by a background reload every 10 seconds) appear as free-roaming snakes (`●` head, `•` body) that slither inside the formation area.
- **Lives.** You start with 3 lives (`♥`). After a hit the cannon blinks while it is briefly invulnerable. Losing all lives shows `GAME OVER`.
- **Screen.** The top line shows score, lives, wave, enemies alive, snakes and queue length. The bottom line shows the last destroyed command (its family color) or the controls. On terminals at least 60 columns wide, a legend sidebar lists families (with counts), the level sprites with their token bands, and the snake. The game needs at least 20x8 cells to render.

### Keybindings

| Key | Action |
| --- | --- |
| `←` / `→` | Move the cannon. |
| `Space` | Fire (at most 4 bullets in flight). |
| `p` / `P` | Pause and resume. |
| `r` | Restart, after `GAME OVER` or `VICTORY`. |
| `q`, `Q`, `Esc`, `Ctrl-C` | Quit. |

### Empty data

With no filtered command for the period, the game shows `No filtered commands yet.` and explains that enemies come from ecotokens interceptions. New interceptions recorded while it is open start the game automatically.

## Outline

```bash
ecotokens outline src/main.rs
ecotokens outline src/ --kinds fn,struct --depth 2
```

Lists the symbols (functions, structs, and so on) of a file or directory, extracted with tree-sitter. Each row shows the symbol kind (cyan), its name, and `file:line` (grey). The list scrolls automatically to keep the selection visible, and the selected row is highlighted with a `> ` marker.

Interactive mode is used when stdout is a terminal. With `--json`, or when piped, the command prints the symbols instead (`file:line kind name` in plain text).

| Key | Action |
| --- | --- |
| `j` / `Down` | Select the next symbol. |
| `k` / `Up` | Select the previous symbol. |
| `q`, `Q`, `Esc`, `Ctrl-C` | Quit. |

Empty data: a single grey row, `No symbols found`.

## Trace

```bash
ecotokens trace callers <symbol>
ecotokens trace callees <symbol> --depth 2
```

Shows the call graph around a symbol, read from the Tantivy index (run `ecotokens index` first, or use `--index-dir`). The panel title is ` callers of <symbol> ` or ` callees of <symbol> `. It is a three-column table: `Name` (30%), `File` (55%) and `Line` (15%).

Interactive mode is used when stdout is a terminal. With `--json`, or when piped, the edges are printed instead (`name file:line`). The table does not scroll or select: the only key is quit (`q`, `Q`, `Esc`, `Ctrl-C`).

Empty data: `No callers found for <symbol>` (or `callees`). A lookup error is printed to stderr and the command exits with status 1.

## Watch

```bash
ecotokens watch                   # foreground, TUI
ecotokens watch --path ./src      # directory to index and watch
ecotokens watch --background      # no TUI, events logged to stdout
ecotokens watch --status          # background watcher status
ecotokens watch --stop            # stop the background watcher
```

Indexes a directory, then keeps the index up to date as files change. The TUI is shown only in the foreground and only on a terminal. `--background` (and the internal auto-watch worker) skip it.

The header is a phase rail, `[● Indexing] → [ Watching]`, which becomes `[✓ Indexing] → [● Watching]` and adds a summary (`N files · N chunks · X.Xs`) once the initial index completes.

### Indexing phase

- A progress gauge titled ` Initial indexing... ` with a percentage (`src/tui/progress.rs`).
- A ` Log ` panel showing the most recent messages. Lines containing `warning` or `Skipping` are yellow, the others grey. Before the first message it reads `Indexing in progress — watching will start next...`.

### Watching phase

- A stats bar with the watched path and the counters `re-indexed`, `ignored` and `errors`.
- An ` Events ` list, newest first: `[timestamp]`, the file path (shortened to its last 50 characters) and a status, green for `re-indexed`, red for `error...`, yellow otherwise.
- Empty data: `No events - waiting for file changes...`.

Both phases show a help line, ` q/Esc: quit  Ctrl-C: stop`.

| Key | Action |
| --- | --- |
| `q`, `Q`, `Esc`, `Ctrl-C` | Quit the watcher. |

## Testing notes

The render functions take a `ratatui::Frame`, so they are unit-tested against a test backend (for example `render_gain`, `render_jev`, `render_outline`, `render_trace`, `render_watch`). Small terminals are covered to make sure rendering never panics. See [TEST-PLAN.md](TEST-PLAN.md) for the TUI checks in the release process.
