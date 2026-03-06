# CLI Contract: ecotokens

**Date**: 2026-02-21 | **Branch**: `001-token-companion`

Interface publique du binaire `ecotokens` — toutes les sous-commandes, leurs entrées,
sorties et codes de retour.

---

## Commande racine

```
ecotokens [OPTIONS] <SUBCOMMAND>
```

**Options globales**:
| Option | Type | Description |
|--------|------|-------------|
| `--json` | flag | Sortie machine-readable JSON sur stdout (désactive TUI) |
| `--no-tui` | flag | Forcer la sortie texte plat même sur TTY |
| `--debug` | flag | Mode debug : affiche input/output/tokens pour chaque opération |
| `--version` | flag | Affiche la version et quitte |
| `--help` | flag | Affiche l'aide |

**Règle TUI** : l'interface ratatui s'active automatiquement si `stdout` est un TTY et qu'aucun des flags `--json` ou `--no-tui` n'est présent. Dans tous les autres cas, sortie texte plat.

**Codes de retour**:
| Code | Signification |
|------|--------------|
| 0 | Succès |
| 1 | Erreur d'usage (argument invalide) |
| 2 | Erreur d'exécution (commande échouée, I/O) |
| 3 | Erreur de configuration (config corrompue) |

---

## `ecotokens hook`

Sous-commande appelée par le hook Claude Code PreToolUse.

**Entrée** : JSON sur stdin

```json
{
  "tool_input": {
    "command": "<commande bash originale>"
  }
}
```

**Sortie** : JSON sur stdout

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "updatedInput": {
      "command": "ecotokens filter -- <commande bash originale>"
    }
  }
}
```

**Comportement** : Réécrit la commande pour la faire passer par `ecotokens filter`.
Si la commande est dans la liste d'exclusion, retourne la commande originale sans modification.

---

## `ecotokens filter -- <COMMAND> [ARGS...]`

Exécute une commande, filtre son output, retourne l'output filtré sur stdout.

**Entrée** : commande et ses arguments après `--`

**Sortie stdout** : output filtré (texte) ou JSON si `--json`

**Sortie stderr** : erreurs de l'outil lui-même (pas de la commande interceptée)

**Comportement debug** (avec `--debug`) :
```
[ecotokens debug] command: git status
[ecotokens debug] tokens_before: 142
[ecotokens debug] tokens_after: 38
[ecotokens debug] savings: 73.2%
[ecotokens debug] mode: filtered
[ecotokens debug] duration_ms: 12
```

**Sortie JSON** (avec `--json`) :
```json
{
  "output": "<output filtré>",
  "tokens_before": 142,
  "tokens_after": 38,
  "savings_pct": 73.2,
  "mode": "filtered",
  "duration_ms": 12
}
```

---

## `ecotokens gain [OPTIONS]`

Affiche le rapport d'économies de tokens.

**Options** :
| Option | Type | Défaut | Description |
|--------|------|--------|-------------|
| `--period` | enum | `all` | `all` / `today` / `week` / `month` |
| `--by-project` | flag | false | Détailler par projet Git |
| `--by-command` | flag | false | Détailler par famille de commande |
| `--history` | flag | false | Lister les dernières interceptions |
| `-n` | u32 | 20 | Nombre d'entrées pour `--history` |
| `--model` | string | `claude-sonnet-4-6` | Modèle de référence pour le calcul de coût USD |

**Sortie TTY (ratatui)** — dashboard interactif, `q` pour quitter :
```
┌─ ecotokens savings report ─────────────────────────────────────┐
│ Period: all time          Model: claude-sonnet-4-6             │
├────────────────────────────────────────────────────────────────┤
│ Interceptions   Tokens before   Tokens after   Savings         │
│     1 247           487 320         98 441      79.8%          │
│                                                                │
│ Cost avoided: $1.17                                            │
├─ By command family ────────────────────────────────────────────┤
│ git    ████████████████████░░░░  82.3%  (834 interceptions)   │
│ cargo  ██████████████████░░░░░░  74.1%  (289 interceptions)   │
│ fs     █████████████████████░░░  91.2%  (124 interceptions)   │
├─ Daily savings (last 14 days) ─────────────────────────────────┤
│ ▁▃▅▄▆▇█▅▃▆▇█▅▄  tokens saved                                  │
└────────────────────────────────────────────────────────────────┘
```

**Sortie texte plat** (si stdout non-TTY ou `--no-tui`) :
```
ecotokens savings report (all time)
────────────────────────────────────
Total interceptions : 1 247
Tokens before       : 487 320
Tokens after        :  98 441
Savings             : 79.8%
Cost avoided        : $1.17  (model: claude-sonnet-4-6 @ $3.00/1M input)

By command family:
  git    : 82.3% savings (834 interceptions)
  cargo  : 74.1% savings (289 interceptions)
  fs     : 91.2% savings ( 124 interceptions)
```

**Sortie JSON** (avec `--json`) :
```json
{
  "period": "all",
  "total_interceptions": 1247,
  "tokens_before": 487320,
  "tokens_after": 98441,
  "savings_pct": 79.8,
  "cost_avoided_usd": 1.17,
  "model_ref": "claude-sonnet-4-6",
  "by_family": {
    "git": { "interceptions": 834, "savings_pct": 82.3 },
    "cargo": { "interceptions": 289, "savings_pct": 74.1 },
    "fs": { "interceptions": 124, "savings_pct": 91.2 }
  }
}
```

---

## `ecotokens install`

Installe le hook dans `~/.claude/settings.json` et crée la configuration par défaut.

**Comportement** :
1. Vérifie que `~/.claude/settings.json` existe (crée si absent)
2. Ajoute le hook `PreToolUse` pour le matcher `Bash`
3. Crée `~/.config/ecotokens/config.json` avec les valeurs par défaut
4. Affiche un récapitulatif de ce qui a été fait

**Idempotent** : peut être exécuté plusieurs fois sans duplication.

**Sortie** :
```
✓ Hook installed in ~/.claude/settings.json
✓ Config created at ~/.config/ecotokens/config.json
✓ ecotokens is active — no further configuration needed.
```

---

## `ecotokens uninstall`

Retire le hook de `~/.claude/settings.json`.

**Comportement** : Supprime uniquement l'entrée ecotokens du bloc hooks. Ne supprime pas `~/.config/ecotokens/`.

---

## `ecotokens config [OPTIONS]`

Affiche ou modifie la configuration.

| Option | Description |
|--------|-------------|
| `--show` | Affiche la configuration actuelle |
| `--exclude <pattern>` | Ajoute un pattern d'exclusion |
| `--include <pattern>` | Retire un pattern d'exclusion |
| `--threshold-lines <n>` | Modifie le seuil de résumé en lignes |
| `--threshold-bytes <n>` | Modifie le seuil de résumé en octets |
| `--masking` / `--no-masking` | Active/désactive le masquage |

---

## `ecotokens index [OPTIONS]` *(P3)*

Indexe le codebase courant pour la recherche sémantique.

| Option | Description |
|--------|-------------|
| `--path <dir>` | Répertoire à indexer (défaut: git root courant) |
| `--reset` | Supprime et recrée l'index |

---

## `ecotokens search <QUERY>` *(P3)*

Recherche des extraits pertinents dans le codebase indexé.

**Sortie** : extraits de code avec score de pertinence et chemin de fichier.

---

## `ecotokens outline [OPTIONS] <PATH>` *(P3)*

Liste les symboles d'un fichier ou d'un répertoire sans transmettre le source complet.

**Arguments** :
| Argument | Type | Description |
|----------|------|-------------|
| `<PATH>` | string | Fichier ou répertoire à inspecter |

**Options** :
| Option | Type | Défaut | Description |
|--------|------|--------|-------------|
| `--kinds <kinds>` | string | tous | Filtrer par kind : `fn,struct,impl,enum` (séparé par virgules) |
| `--depth` | u32 | 1 | Profondeur de récursion pour les répertoires |

**Sortie TTY (ratatui)** — liste scrollable, `↑↓` pour naviguer, `Enter` pour voir le snippet, `q` pour quitter :
```
┌─ outline: src/filter/git.rs ───────────────────────────────────┐
│ ▶ fn  filter_status       line  12–  45                        │
│   fn  filter_diff         line  47–  98                        │
│   fn  truncate_diff_hunk  line 100– 122                        │
└────────────────────────────────────────────── 3 symbols ───────┘
```

**Sortie texte plat** (si stdout non-TTY ou `--no-tui`) :
```
src/filter/git.rs
  fn filter_status       [line 12–45]
  fn filter_diff         [line 47–98]
  fn truncate_diff_hunk  [line 100–122]
```

**Sortie JSON** (avec `--json`) :
```json
[
  { "id": "src/filter/git.rs::filter_status#fn", "kind": "fn", "name": "filter_status",
    "line_start": 12, "line_end": 45, "file": "src/filter/git.rs" }
]
```

---

## `ecotokens symbol <ID>` *(P3)*

Retourne le source complet d'un symbole par son ID stable.

**Arguments** :
| Argument | Type | Description |
|----------|------|-------------|
| `<ID>` | string | ID stable du symbole (ex: `src/filter/git.rs::filter_status#fn`) |

**Sortie** : extrait source du symbole avec contexte (±2 lignes).

**Sortie JSON** (avec `--json`) :
```json
{
  "id": "src/filter/git.rs::filter_status#fn",
  "kind": "fn",
  "name": "filter_status",
  "file": "src/filter/git.rs",
  "line_start": 12,
  "line_end": 45,
  "source_snippet": "pub fn filter_status(output: &str) -> FilterResult {\n    ..."
}
```

---

## `ecotokens trace <SUBCOMMAND> <SYMBOL>` *(P3)*

Navigation dans le graphe d'appels.

### `ecotokens trace callers <SYMBOL>`

Retourne tous les sites qui appellent `<SYMBOL>`.

**Arguments** :
| Argument | Type | Description |
|----------|------|-------------|
| `<SYMBOL>` | string | Nom ou ID stable du symbole cible |

**Options** :
| Option | Type | Défaut | Description |
|--------|------|--------|-------------|
| `--depth` | u32 | 1 | Profondeur de récursion dans le graphe |

**Sortie TTY (ratatui)** :
```
┌─ callers of `filter_status` ───────────────────────────────────┐
│ Caller                          File                    Line   │
│ ──────────────────────────────────────────────────────────── │
│ handle_git                      src/hook/handler.rs       87  │
│ dispatch                        src/filter/mod.rs          34  │
│ test_filter_status_clean        tests/filter/git_test.rs   12  │
└────────────────────────────────────────────── 3 callers ───────┘
```

**Sortie texte plat** (si stdout non-TTY ou `--no-tui`) :
```
Callers of `filter_status` (3 found):
  src/hook/handler.rs:87   in `handle_git`
  src/filter/mod.rs:34     in `dispatch`
  tests/filter/git_test.rs:12  in `test_filter_status_clean`
```

**Sortie JSON** (avec `--json`) :
```json
[
  { "caller_id": "src/hook/handler.rs::handle_git#fn",
    "file": "src/hook/handler.rs", "line": 87 }
]
```

### `ecotokens trace callees <SYMBOL>`

Retourne tous les symboles appelés par `<SYMBOL>`.

**Sortie** : même format que `trace callers`, inversé (callee → sites d'appel dans `<SYMBOL>`).

---

## `ecotokens mcp` *(P3)*

Démarre ecotokens en mode serveur MCP (stdio). Bloque jusqu'à fermeture de stdin.

**Usage** : configuré dans `~/.claude/settings.json` comme serveur MCP stdio.

```json
{
  "mcpServers": {
    "ecotokens": {
      "command": "ecotokens",
      "args": ["mcp"]
    }
  }
}
```

**Tools MCP exposés** :
| Tool MCP | Équivalent CLI | Description |
|----------|---------------|-------------|
| `ecotokens_search` | `ecotokens search` | Recherche sémantique/BM25 |
| `ecotokens_outline` | `ecotokens outline` | Symboles d'un fichier/module |
| `ecotokens_symbol` | `ecotokens symbol` | Source d'un symbole par ID |
| `ecotokens_trace_callers` | `ecotokens trace callers` | Callers d'un symbole |
| `ecotokens_trace_callees` | `ecotokens trace callees` | Callees d'un symbole |

**Protocole** : JSON-RPC 2.0 sur stdio (spec MCP 2024-11).

---

## `ecotokens watch [OPTIONS]` *(P3)*

Démarre le daemon de surveillance et re-indexation automatique.

**Options** :
| Option | Type | Défaut | Description |
|--------|------|--------|-------------|
| `--path <dir>` | string | git root courant | Répertoire à surveiller |
| `--daemon` | flag | false | Détacher en arrière-plan (fork) |

**Comportement** :
- Surveille les modifications de fichiers dans `<dir>` via `notify`
- Re-indexe automatiquement les fichiers modifiés (indexation incrémentale)
- En mode `--daemon`, écrit le PID dans `~/.config/ecotokens/watch.pid`

**Sortie TTY** (mode foreground, ratatui alternate screen) :
```
┌─ ecotokens watch ──────────────────────────── /home/user/project ┐
│ Status: WATCHING   Files: 2 347   Symbols: 18 432   q to quit    │
├─ Recent changes ────────────────────────────────────────────────┤
│ 14:32:01  src/filter/git.rs          re-indexed   12ms  3 syms  │
│ 14:31:47  src/hook/handler.rs        re-indexed    8ms  7 syms  │
│ 14:29:12  tests/filter/git_test.rs   re-indexed    5ms  2 syms  │
│                                                                   │
│                                                                   │
└─────────────────────────────────────────── Ctrl-C to stop ───────┘
```

**Sortie texte plat** (si stdout non-TTY, `--no-tui`, ou `--daemon`) :
```
[ecotokens watch] Watching /home/user/project (2 347 files)
[ecotokens watch] src/filter/git.rs modified — re-indexed in 12ms (3 symbols updated)
```
