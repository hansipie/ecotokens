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

**Temps d'installation (SC-002)** :
- `cargo install --path .` : ~16 secondes (build from source)
- `ecotokens install` (hook) : < 1 seconde
- Objectif SC-002 (< 5 min) : ✅ validé

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
ecotokens gain --period week
ecotokens gain --json
```

Pour diagnostiquer une interception :
```bash
ecotokens filter --debug -- git status
```

## TUI interactive

Les commandes suivantes ouvrent une interface terminal interactive (TTY requis) :

| Commande | Description | Contrôles |
|----------|-------------|-----------|
| `ecotokens outline src/` | Arbre navigable des symboles | ↑↓ ou j/k pour naviguer, q/Esc pour quitter |
| `ecotokens trace callers <sym>` | Graphe des callers d'un symbole | q/Esc pour quitter |
| `ecotokens index` | Barre de progression animée | q/Esc après 100% |
| `ecotokens gain` | Dashboard auto-rafraîchi | Mise à jour toutes les secondes, q/Esc pour quitter |

En dehors d'un TTY (script, pipe, `--json`), toutes les commandes retombent automatiquement
sur une sortie texte plat sans interaction.

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

## Mode MCP (navigation structurelle)

`ecotokens` expose un serveur MCP natif qui permet à Claude d'appeler directement
les outils de recherche et de navigation sans passer par bash.

### Activation

```bash
# Option 1 : installation automatique (opt-in)
ecotokens install --with-mcp

# Option 2 : ajout manuel dans ~/.claude/settings.json
```

```json
{
  "mcpServers": {
    "ecotokens": {
      "command": "ecotokens mcp",
      "type": "stdio"
    }
  }
}
```

### Indexer votre projet

```bash
# Indexer le répertoire courant (BM25 + symboles tree-sitter)
ecotokens index

# Indexer un répertoire spécifique
ecotokens index --path /chemin/vers/projet

# Réinitialiser l'index
ecotokens index --reset
```

### Outils MCP disponibles

| Outil | Description |
|-------|-------------|
| `ecotokens_search` | Recherche BM25 dans le codebase indexé |
| `ecotokens_outline` | Liste les symboles d'un fichier ou répertoire |
| `ecotokens_symbol` | Retourne le source d'un symbole par ID stable |
| `ecotokens_trace_callers` | Trouve qui appelle un symbole donné |
| `ecotokens_trace_callees` | Trouve ce qu'appelle un symbole donné |

### Utilisation depuis la CLI

```bash
# Recherche sémantique
ecotokens search "gestion des erreurs" --top-k 5
ecotokens search "authentication" --json

# Outline d'un fichier ou répertoire
ecotokens outline src/filter/git.rs
ecotokens outline src/ --kinds fn,struct
ecotokens outline src/ --depth 1 --json

# Lookup d'un symbole par ID stable
ecotokens symbol "git.rs::filter_git#fn"

# Graphe d'appels
ecotokens trace callers filter_git
ecotokens trace callees filter_git --depth 2
ecotokens trace callers filter_git --json
```

### IDs de symboles stables

Le format d'un ID de symbole est `{fichier}::{nom}#{kind}` :

```
src/filter/git.rs::filter_git#fn
src/config/settings.rs::Settings#struct
lib.rs::greet#fn
```

### Architecture MCP

```
Claude Code (avec MCP configuré)
    │
    │ (tool call JSON-RPC stdio)
    ▼
ecotokens mcp
    ├── ecotokens_search   → tantivy BM25 index
    ├── ecotokens_outline  → tree-sitter AST symbols
    ├── ecotokens_symbol   → source snippet lookup
    ├── ecotokens_trace_callers → graphe d'appels inverse
    └── ecotokens_trace_callees → graphe d'appels direct
```

Les outils MCP lisent l'index local (`~/.config/ecotokens/index/`) — **aucune
connexion réseau requise**.

## Providers d'embeddings (précision sémantique)

Par défaut, `ecotokens search` utilise BM25 (recherche lexicale). Pour activer la
recherche sémantique via embeddings, configurez un provider local :

```bash
# Ollama (nécessite modèle nomic-embed-text téléchargé)
ecotokens config --embed-provider ollama --embed-url http://localhost:11434

# LM Studio (format OpenAI-compatible, modèle nomic-embed-text-v1.5)
ecotokens config --embed-provider lmstudio --embed-url http://localhost:1234

# Désactiver les embeddings
ecotokens config --embed-provider none

# Vérifier la configuration courante
ecotokens config
```

Une fois configuré, relancez `ecotokens index` pour calculer et stocker les embeddings.
Les résultats de `ecotokens search` seront re-scorés (50% BM25 + 50% cosine similarity).

> **Note** : Les embeddings nécessitent la feature `embeddings` à la compilation :
> `cargo build --release --features embeddings`
> Le binaire standard (sans cette feature) reste < 20 MB et utilise BM25 seul.

## Variables d'environnement

| Variable | Description |
|----------|-------------|
| `ECOTOKENS_CONFIG` | Chemin alternatif pour config.json |
| `ECOTOKENS_DEBUG` | Équivalent à `--debug` si défini |
| `ECOTOKENS_NO_FILTER` | Désactive tout filtrage si défini (utile pour les tests) |
