# ecotokens Constitution

## Core Principles

### I. CLI-First
Every feature MUST expose its functionality via the CLI. The canonical interface is:
stdin/args → stdout ; errors → stderr. Both JSON and human-readable output formats
MUST be supported. No functionality may exist solely as an internal API without a
corresponding CLI entry point.

### II. Token Efficiency
Every feature MUST reduce or measure LLM token consumption. Savings metrics are
mandatory for all filtering/proxy operations. No regression in token efficiency is
tolerated — each release MUST demonstrate equal or better savings than the previous.

### III. Test-First (NON-NEGOTIABLE)
TDD is strictly enforced. Tests MUST be written and approved before any implementation
begins. The Red-Green-Refactor cycle is mandatory. No implementation PR will be merged
without prior test approval. This principle supersedes delivery pressure.

## Constraints

- **Language**: Rust stable (no nightly features)
- **Binary**: Statically linked, no external runtime required
- **Dependencies**: Minimize external crates; justify each addition
- **Platforms**: Linux x86_64 primary; WASM as future target

## Governance

This constitution supersedes all other practices and guidelines. Amendments require
a dedicated PR with explicit justification and impact analysis. All PRs must include
a constitution compliance check verifying the three core principles (CLI-First, Token
Efficiency, Test-First) before merge.

**Version**: 1.0.0 | **Ratified**: 2026-02-21 | **Last Amended**: 2026-02-21
