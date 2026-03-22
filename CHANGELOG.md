# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.7.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.7.0
[0.6.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.6.0
[0.5.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.5.0
[0.4.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.4.0
[0.3.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.3.0
[0.2.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.2.0
[0.1.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.1.0
