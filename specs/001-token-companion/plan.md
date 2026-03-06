# Implementation Plan: Outil compagnon d'économie de tokens pour Claude Code

**Branch**: `001-token-companion` | **Date**: 2026-02-21 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-token-companion/spec.md`

## Summary

Construire `ecotokens` — un binaire Rust CLI installé une fois sur la machine qui
s'intègre dans Claude Code via le mécanisme de hooks pour intercepter les outputs de
commandes bash, les filtrer/compresser afin de réduire la consommation de tokens LLM
d'au moins 60%, tout en enregistrant les métriques d'économies et en masquant les
données sensibles. Installation unique, zéro configuration quotidienne, activation
automatique via hook `PreToolUse`.

## Technical Context

**Language/Version**: Rust stable (≥ 1.75, no nightly)
**Primary Dependencies**:
- `clap` — parsing CLI args
- `serde` / `serde_json` — sérialisation JSON (hooks I/O + config + métriques)
- `regex` — détection et masquage de données sensibles
- `dirs` — résolution du répertoire home pour config/métriques machine-wide
- `ratatui = { version = "0.29", features = ["crossterm"] }` — widgets TUI (tableaux,
  sparklines, barres de progression, arbres) ; crossterm est tiré en dépendance transitive

**Dépendances P1 (ajout) — fichiers texte** :
- Aucune nouvelle crate : parsing Markdown par regex (crate `regex` déjà présente), TOML via
  `toml = "0.8"` (déjà disponible si nécessaire), JSON via `serde_json` (déjà présent)

**Dépendances P3 — indexation et navigation** :
- `tantivy = "0.21"` — index BM25 fulltext offline
- `tree-sitter = "0.22"` — parsing AST multi-langages
- `tree-sitter-rust` — grammaire Rust
- `tree-sitter-python` — grammaire Python
- `tree-sitter-javascript` — grammaire JavaScript
- `tree-sitter-typescript` — grammaire TypeScript
- `rmcp` — Rust MCP SDK (serveur MCP stdio, protocole JSON-RPC)
- `notify = "6"` — file watcher cross-platform pour le daemon

**Dépendances optionnelles (feature flags)** :
- `tiktoken-rs` — comptage exact de tokens (feature `exact-tokens`, désactivé par défaut ;
  heuristique chars×0.25 utilisée sinon)

**Feature flags** :
```toml
[features]
exact-tokens = ["tiktoken-rs"]
embeddings = ["reqwest"]          # Providers Ollama / LM Studio
```

**Storage**: Fichiers locaux dans `~/.config/ecotokens/` (JSON pour config, métriques)
**Testing**: `cargo test` (TDD strict — cf. constitution)
**Target Platform**: Linux x86_64 (binaire statique musl)
**Project Type**: CLI tool
**Performance Goals**: Latence d'interception < 50ms pour outputs ≤ 500 lignes / 50 Ko
**Constraints**: Offline-only, binaire statique, < 20 MB, aucun runtime externe
**Scale/Scope**: Machine-wide, tous projets Git, sessions Claude Code parallèles

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Gate I — CLI-First ✅

- Toutes les fonctionnalités (filtrage, rapport, indexation, debug) sont exposées via CLI
- Formats JSON (`--json`) et lisible humain supportés (FR-010)
- stdin/args → stdout ; erreurs → stderr
- **PASS**

### Gate II — Token Efficiency ✅

- La raison d'être du produit est la réduction de tokens : SC-001 cible ≥ 60% d'économies
- Métriques obligatoires pour chaque interception (FR-004)
- Aucune régression tolérée (principe constitutionnel)
- **PASS**

### Gate III — Test-First (NON-NEGOTIABLE) ✅

- TDD strict enforced par la constitution
- Les tests doivent être écrits et validés avant implémentation
- Cycle Red-Green-Refactor obligatoire
- Toutes les tâches d'implémentation seront précédées d'une tâche de test correspondante
- **PASS**

**Constitution Check: 3/3 PASS — Phase 0 autorisée**

## Project Structure

### Documentation (this feature)

```text
specs/001-token-companion/
├── plan.md              # Ce fichier
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   └── cli-schema.md
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
src/
├── main.rs              # Point d'entrée CLI (clap)
├── filter/
│   ├── mod.rs
│   ├── git.rs           # Filtres git status/diff/log
│   ├── cargo.rs         # Filtres cargo build/test
│   ├── fs.rs            # Filtres ls/find
│   ├── markdown.rs      # Extraction sections par heading H1-H3
│   ├── config_file.rs   # Résumé TOML/JSON/YAML par clés racine
│   └── generic.rs       # Résumé générique > 500 lignes
├── masking/
│   ├── mod.rs
│   └── patterns.rs      # Regex patterns : API keys, tokens, .env
├── metrics/
│   ├── mod.rs
│   ├── store.rs         # Persistance JSON dans ~/.config/ecotokens/
│   └── report.rs        # Génération rapport d'économies
├── tokens/
│   ├── mod.rs
│   └── counter.rs       # Comptage approché via tiktoken-rs
├── hook/
│   ├── mod.rs
│   └── handler.rs       # Lecture stdin JSON Claude Code hook, écriture stdout
├── search/
│   ├── mod.rs
│   ├── index.rs         # Indexation sémantique codebase BM25 (P3)
│   ├── symbols.rs       # Indexation symboles AST via tree-sitter (P3)
│   ├── outline.rs       # Génération outline fichier/module (P3)
│   └── text_docs.rs     # Indexation fichiers texte : headings MD, clés TOML/JSON/YAML (P3)
├── trace/
│   ├── mod.rs           # Graphe d'appels : callers/callees (P3)
│   ├── callers.rs       # "Qui appelle ce symbole ?"
│   └── callees.rs       # "Que appelle ce symbole ?"
├── mcp/
│   ├── mod.rs           # Serveur MCP stdio via rmcp (P3)
│   ├── server.rs        # Boucle principale JSON-RPC stdio
│   └── tools.rs         # Handlers tools MCP : search, outline, trace
├── daemon/
│   ├── mod.rs           # Daemon file watcher (P3)
│   └── watcher.rs       # Surveillance fichiers via notify, re-indexation auto
├── tui/
│   ├── mod.rs           # TTY-detect, dispatch TUI vs texte plat
│   ├── gain.rs          # Dashboard ecotokens gain (tableau + sparkline + coût USD)
│   ├── watch.rs         # Panel live ecotokens watch (liste fichiers re-indexés)
│   ├── progress.rs      # Barre de progression pour ecotokens index
│   ├── outline.rs       # Arbre navigable pour ecotokens outline
│   └── trace.rs         # Tableau callers/callees pour ecotokens trace
└── config/
    ├── mod.rs
    └── settings.rs      # Lecture ~/.config/ecotokens/config.json

tests/
├── filter/
│   ├── git_test.rs
│   ├── cargo_test.rs
│   └── fs_test.rs
├── masking/
│   └── patterns_test.rs
├── metrics/
│   └── store_test.rs
├── hook/
│   └── handler_test.rs
├── search/
│   ├── index_test.rs
│   └── symbols_test.rs
├── trace/
│   └── trace_test.rs
└── integration/
    └── end_to_end_test.rs
```

**Structure Decision**: Single project Rust workspace. Chaque domaine fonctionnel est un
module distinct pour permettre le test indépendant (constitution : Test-First). Pas de
workspace multi-crate pour éviter la surcomplication sur un premier livrable.

## Complexity Tracking

Aucune violation de la constitution détectée — section non applicable.
