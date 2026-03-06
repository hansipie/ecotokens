# Quickstart: ecotokens

**Date**: 2026-02-21 | Guide développeur pour démarrer sur le projet

## Prérequis

- Rust stable ≥ 1.75 (`rustup update stable`)
- Claude Code installé
- Linux x86_64

## Installation (utilisateur final)

```bash
# Option 1 : depuis les releases GitHub
curl -L https://github.com/ecotokens/ecotokens/releases/latest/download/ecotokens-linux-x86_64 \
  -o ~/.local/bin/ecotokens && chmod +x ~/.local/bin/ecotokens

# Activer le hook Claude Code (one-time)
ecotokens install

# Vérifier que tout fonctionne
ecotokens gain
```

C'est tout — `ecotokens` est maintenant actif dans toutes vos sessions Claude Code.

## Développement (contributeurs)

```bash
# Cloner et builder
git clone https://github.com/ecotokens/ecotokens
cd ecotokens
cargo build

# Lancer les tests (TDD — les tests existent avant l'implémentation)
cargo test

# Lancer les tests avec couverture
cargo llvm-cov

# Build release statique (musl)
cargo build --release --target x86_64-unknown-linux-musl
```

## Utilisation quotidienne (pas d'action requise)

Le hook est actif automatiquement. Utilisez Claude Code normalement.

Pour consulter les économies :
```bash
ecotokens gain
ecotokens gain --period week --by-command
ecotokens gain --json
```

Pour diagnostiquer une interception :
```bash
ecotokens filter --debug -- git status
```

Pour exclure une commande du filtrage :
```bash
ecotokens config --exclude "cat *"
```

## Architecture en un coup d'œil

```
Claude Code
    │
    │ (hook PreToolUse)
    ▼
ecotokens hook          ← lit stdin JSON, réécrit la commande
    │
    │ (command rewrite)
    ▼
ecotokens filter -- <cmd>
    ├── exécute la commande originale
    ├── applique le filtre famille (git/cargo/fs/generic)
    ├── masque les données sensibles
    ├── enregistre les métriques
    └── retourne l'output filtré sur stdout
    │
    ▼
Claude reçoit l'output filtré (≥60% tokens économisés)
```

## Structure des fichiers de config

```
~/.config/ecotokens/
├── config.json          # Configuration utilisateur
└── metrics.jsonl        # Log des interceptions (append-only)
```

## Variables d'environnement

| Variable | Description |
|----------|-------------|
| `ECOTOKENS_CONFIG` | Chemin alternatif pour config.json |
| `ECOTOKENS_DEBUG` | Équivalent à `--debug` si défini |
| `ECOTOKENS_NO_FILTER` | Désactive tout filtrage si défini (utile pour les tests) |
