# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.5.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.5.0
[0.4.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.4.0
[0.3.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.3.0
[0.2.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.2.0
[0.1.0]: https://github.com/hansipie/ecotokens/releases/tag/v0.1.0
