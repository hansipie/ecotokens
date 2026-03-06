# Data Model: Outil compagnon d'économie de tokens pour Claude Code

**Date**: 2026-02-21 | **Branch**: `001-token-companion`

## Entités

### Interception

Représente une commande interceptée par le hook, avec les métriques d'économie associées.

| Champ | Type | Contraintes | Description |
|-------|------|-------------|-------------|
| `id` | UUID v4 | NOT NULL, unique | Identifiant unique |
| `timestamp` | ISO 8601 | NOT NULL | Date/heure d'interception |
| `command` | string | NOT NULL, ≤ 4096 chars | Commande bash originale |
| `command_family` | enum | NOT NULL | `git` / `cargo` / `fs` / `markdown` / `config_file` / `generic` |
| `git_root` | string \| null | nullable | Répertoire racine Git du projet |
| `tokens_before` | u32 | NOT NULL, > 0 | Tokens estimés avant filtrage |
| `tokens_after` | u32 | NOT NULL, > 0 | Tokens estimés après filtrage |
| `savings_pct` | f32 | 0.0–100.0 | `(1 - after/before) * 100` |
| `mode` | enum | NOT NULL | `filtered` / `passthrough` / `summarized` |
| `redacted` | bool | NOT NULL | true si masquage appliqué |
| `duration_ms` | u32 | NOT NULL | Temps de traitement en ms |

**Règles de validation**:
- `tokens_after` ≤ `tokens_before` (sinon mode `passthrough` forcé)
- `savings_pct` = 0 si mode `passthrough`
- `duration_ms` ≤ 200 (alerte si dépassé)

**Stockage**: ligne JSON dans `~/.config/ecotokens/metrics.jsonl`

---

### Profil de filtre

Règles de transformation associées à une famille de commande.

| Champ | Type | Contraintes | Description |
|-------|------|-------------|-------------|
| `family` | enum | NOT NULL, unique | `git` / `cargo` / `fs` / `markdown` / `config_file` / `generic` |
| `max_lines` | u32 | NOT NULL, défaut 500 | Seuil de résumé |
| `max_bytes` | u32 | NOT NULL, défaut 51200 | Seuil en octets (50 Ko) |
| `rules` | vec\<Rule\> | NOT NULL, ≥ 1 | Liste de règles ordonnées |

**Rule** (sous-entité de Profil de filtre):
| Champ | Type | Description |
|-------|------|-------------|
| `name` | string | Identifiant lisible de la règle |
| `pattern` | regex | Pattern de détection (ligne ou bloc) |
| `action` | enum | `keep` / `drop` / `truncate` / `replace` |
| `replacement` | string \| null | Texte de remplacement si `replace` |

---

### Configuration

Paramètres utilisateur machine-wide.

| Champ | Type | Défaut | Description |
|-------|------|--------|-------------|
| `exclusions` | vec\<string\> | `[]` | Commandes exclues du filtrage (glob) |
| `summary_threshold_lines` | u32 | `500` | Seuil de résumé (lignes) |
| `summary_threshold_bytes` | u32 | `51200` | Seuil de résumé (octets) |
| `masking_enabled` | bool | `true` | Activation du masquage secrets |
| `exact_token_counting` | bool | `false` | Utiliser tiktoken-rs (plus précis) |
| `debug` | bool | `false` | Mode debug persistant (override --debug) |
| `default_model` | string | `"claude-sonnet-4-6"` | Modèle de référence pour calcul coût USD |
| `model_pricing` | map\<string, ModelPrice\> | (table statique) | Override prix par modèle |
| `embed_provider` | EmbedProvider | `None` | Provider d'embeddings pour recherche sémantique |

**ModelPrice** (sous-entité) :
| Champ | Type | Description |
|-------|------|-------------|
| `input_usd_per_1m` | f64 | Prix par million de tokens en entrée |
| `output_usd_per_1m` | f64 | Prix par million de tokens en sortie |

**EmbedProvider** (enum) :
| Variante | Champs | Description |
|----------|--------|-------------|
| `None` | — | BM25 seul (défaut, aucune dépendance externe) |
| `Ollama` | `url: String` | Ollama local (ex: `http://localhost:11434`) |
| `LmStudio` | `url: String` | LM Studio local (ex: `http://localhost:1234`) |

**Stockage**: `~/.config/ecotokens/config.json`
**Validation**: `summary_threshold_lines` ∈ [10, 10000] ; `summary_threshold_bytes` ∈ [1024, 1048576]

---

### Rapport d'économies

Agrégation calculée à la volée à partir de `metrics.jsonl`.

| Champ | Type | Description |
|-------|------|-------------|
| `period` | string | `all` / `today` / `week` / `month` |
| `total_interceptions` | u32 | Nombre total d'interceptions |
| `total_tokens_before` | u64 | Total tokens avant filtrage |
| `total_tokens_after` | u64 | Total tokens après filtrage |
| `total_savings_pct` | f32 | Économie globale en % |
| `cost_avoided_usd` | f64 | Équivalent USD économisé (tokens_saved × prix_input_modèle) |
| `model_ref` | string | Modèle utilisé pour le calcul de coût (ex: `claude-sonnet-4-6`) |
| `by_family` | map\<string, FamilyStats\> | Stats par famille de commande |
| `by_project` | map\<string, ProjectStats\> | Stats par répertoire Git root |

**Note**: Entité calculée, jamais stockée — générée à la demande depuis metrics.jsonl.

**Calcul cost_avoided_usd** :
```
tokens_saved = total_tokens_before - total_tokens_after
cost_avoided_usd = (tokens_saved / 1_000_000) × model_pricing[model_ref].input_usd_per_1m
```

---

### Index codebase (P3)

Représentation locale du code source pour la recherche sémantique.

| Champ | Type | Description |
|-------|------|-------------|
| `git_root` | string | Répertoire racine Git indexé |
| `indexed_at` | ISO 8601 | Date de dernière indexation |
| `file_count` | u32 | Nombre de fichiers indexés |
| `chunk_count` | u32 | Nombre de fragments dans l'index |
| `symbol_count` | u32 | Nombre de symboles AST indexés |

**Stockage**: `~/.config/ecotokens/index/<hash-git-root>/` (index tantivy)

---

### Symbol (P3 — indexation AST)

Représentation d'un symbole de code extrait par tree-sitter.

| Champ | Type | Contraintes | Description |
|-------|------|-------------|-------------|
| `id` | string | NOT NULL, unique | ID stable : `{file_path}::{qualified_name}#{kind}` |
| `file_path` | string | NOT NULL | Chemin relatif depuis git root |
| `qualified_name` | string | NOT NULL | Nom qualifié (ex: `filter_status`, `MyStruct::method`) |
| `kind` | enum | NOT NULL | `fn` / `struct` / `impl` / `enum` / `trait` / `const` / `type` / `mod` / `h1` / `h2` / `h3` / `table` / `key` |
| `line_start` | u32 | NOT NULL | Ligne de début dans le fichier |
| `line_end` | u32 | NOT NULL | Ligne de fin dans le fichier |
| `source_snippet` | string | NOT NULL, ≤ 4096 chars | Extrait du code source |
| `language` | string | NOT NULL | `rust` / `python` / `javascript` / `typescript` / `markdown` / `toml` / `json` / `yaml` |

**Exemple d'ID (code)** : `src/filter/git.rs::filter_status#fn`
**Exemple d'ID (markdown)** : `docs/spec.md::User-Story-3#h2`
**Exemple d'ID (config)** : `Cargo.toml::dependencies#table`

**Stockage** : dans l'index tantivy du projet (`~/.config/ecotokens/index/<hash>/`)

---

### CallEdge (P3 — graphe d'appels)

Représente une relation d'appel entre deux symboles, construite lors de l'indexation AST.

| Champ | Type | Contraintes | Description |
|-------|------|-------------|-------------|
| `caller_id` | string | NOT NULL, FK → Symbol.id | Symbole appelant |
| `callee_id` | string | NOT NULL, FK → Symbol.id | Symbole appelé |
| `file` | string | NOT NULL | Fichier où l'appel est effectué |
| `line` | u32 | NOT NULL | Ligne de l'appel dans le fichier |
| `call_kind` | enum | NOT NULL | `direct` / `method` / `closure` |

**Règles de validation** :
- `caller_id` ≠ `callee_id` (pas d'auto-appel)
- Un `CallEdge` est unique par `(caller_id, callee_id, file, line)`

**Stockage** : dans l'index tantivy du projet, champ `call_edges` sérialisé JSON

---

## Transitions d'état — Interception

```
Commande reçue
     │
     ▼
[Vérification exclusion] ──── dans liste exclusion ──► Passthrough (mode=passthrough, savings=0)
     │
     ▼
[Mesure tokens_before]
     │
     ▼
[Masquage secrets] (si masking_enabled)
     │
     ▼
[Application filtre famille]
     │
     ├── output réduit ──► [Mesure tokens_after] ──► mode=filtered
     │
     ├── output > seuil ──► [Résumé générique] ──► mode=summarized
     │
     └── filtre inopérant (after > before) ──► mode=passthrough
     │
     ▼
[Enregistrement métriques]
     │
     ▼
Output transmis à Claude
```
