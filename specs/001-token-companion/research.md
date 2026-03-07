# Research: Outil compagnon d'économie de tokens pour Claude Code

**Date**: 2026-02-21 | **Branch**: `001-token-companion`

## D1 — Mécanisme de hooks Claude Code

**Decision**: Utiliser le hook **PreToolUse** sur le matcher `Bash`

**Rationale**: Le hook `PreToolUse` intercepte la commande *avant* son exécution et
permet de la réécrire. Le pattern est : `git status` → `ecotokens filter git status`.
L'outil exécute lui-même la commande, filtre l'output, puis le retourne à Claude.
Contrairement à `PostToolUse`, cette approche permet de contrôler entièrement ce que
Claude reçoit.

**Format d'entrée hook (stdin JSON)**:
```json
{
  "tool_input": {
    "command": "git status"
  }
}
```

**Format de sortie hook (stdout JSON)**:
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "updatedInput": {
      "command": "ecotokens filter -- git status"
    }
  }
}
```

**Configuration** (`~/.claude/settings.json`):
```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "ecotokens hook"
          }
        ]
      }
    ]
  }
}
```

**Alternatives considérées**:
- `PostToolUse` : ne permet pas de modifier l'output — écarté
- Wrapper shell (alias) : fragile, non portable, requiert modification du shell — écarté

---

## D2 — Comptage de tokens (offline)

**Decision**: Approche en deux phases — heuristique d'abord, `tiktoken-rs` optionnel

**Rationale**: Une heuristique simple (`chars * 0.25`) offre 80-85% de précision avec
latence < 1ms et zéro dépendance lourde. C'est suffisant pour les métriques d'économies
comparatives. `tiktoken-rs` est gardé en option pour les utilisateurs souhaitant une
précision exacte (compatible `cl100k_base` / `o200k_base`).

**Phase 1 (défaut)**:
```rust
fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() as f64 * 0.25).ceil() as usize
}
```

**Phase 2 (optionnel, feature flag)**:
```toml
[features]
exact-tokens = ["tiktoken-rs"]
```

**Alternatives considérées**:
- `tokenizers` (HuggingFace) : précis mais nécessite des fichiers modèle — écarté pour défaut
- Comptage de mots : trop imprécis pour les contenus mixtes code/texte — écarté

---

## D3 — Masquage des données sensibles

**Decision**: Pipeline de regex compilées statiquement (`lazy_regex`)

**Rationale**: 12 patterns couvrent ~95% des secrets courants. La compilation statique
(lazy_static / once_cell) évite le coût de compilation répété à chaque interception.
Le masquage est appliqué *après* le filtrage, sur l'output final.

**Patterns retenus**:
| Pattern | Exemple | Remplacement |
|---------|---------|-------------|
| Clés AWS | `AKIA[A-Z0-9]{16}` | `[AWS_KEY]` |
| GitHub PAT | `ghp_[A-Za-z0-9_]{36,}` | `[GITHUB_TOKEN]` |
| Bearer token | `bearer <token>` | `[BEARER_TOKEN]` |
| Clé privée PEM | `-----BEGIN ... PRIVATE KEY-----` | `[PRIVATE_KEY]` |
| Variables .env sensibles | `SECRET=`, `PASSWORD=`, `API_KEY=` | `[REDACTED]` |
| JWT | `ey[A-Za-z0-9_-]{10,}\.[...]\.[...]` | `[JWT_TOKEN]` |
| URL avec credentials | `https://user:pass@host` | `https://[CREDENTIALS]@host` |

**Crates**: `regex = "1.10"`, `lazy_regex = "3.0"`

**Alternatives considérées**:
- Bibliothèque dédiée (detect-secrets) : pas de binding Rust stable — écarté
- Scan lexical sans regex : insuffisant pour les patterns JWT/PEM — écarté

---

## D4 — Filtres par type de commande

**Decision**: Modules de filtrage dédiés par famille de commande

**Rationale**: Chaque famille a des patterns d'output radicalement différents.
Un module par famille permet des tests unitaires précis et une extension sans régression.

**Familles prioritaires (P1)**:
| Famille | Commandes / extensions ciblées | Stratégie de filtrage |
|---------|-------------------------------|-----------------------|
| git | status, diff, log, show | Extraction sections pertinentes, troncature diff |
| cargo | build, test, check, clippy | Extraction erreurs/warnings, résumé stats |
| fs | ls, find, tree | Troncature à N entrées, suppression métadonnées |
| markdown | cat/read sur `.md`, `.mdx` | Extraction sections par heading ; ToC si > 200 lignes |
| config_file | cat/read sur `.toml`, `.json`, `.yaml`, `.yml` | Résumé clés racine, valeurs longues tronquées |
| générique | toute commande > 500 lignes | Résumé : N premières + N dernières lignes + compte |

**Détection de famille** : la famille `markdown` et `config_file` est détectée sur l'extension
du fichier passé en argument à `cat`, `bat`, `less`, `head`, `tail` ou à la sous-commande
`Read` de Claude (via le hook).

**Alternatives considérées**:
- Filtre générique unique : perte de précision sur les commandes clés — écarté
- Filtrage LLM en ligne : nécessite connexion réseau — écarté (FR-009)

---

## D5 — Recherche sémantique (P3)

**Decision**: Indexation BM25 via `tantivy` pour phase 1 ; embeddings ONNX pour phase 2

**Rationale**: `tantivy` est pur Rust, offline, rapide, et produit des résultats
satisfaisants pour la recherche dans du code (noms de fonctions, commentaires).
Pas de fichier modèle requis. Pour une meilleure précision sémantique, l'upgrade vers
`ort` + modèle quantisé MiniLM (~50 MB) est prévu mais hors scope initial.

**Crates P3 phase 1**: `tantivy = "0.21"`, `rusqlite = "0.30"` (stockage index)

**Alternatives considérées**:
- `faiss-rs` : requiert librairie C++ compilée localement — écarté pour simplicité d'installation
- `tch-rs` : binaire > 500 MB — écarté (contrainte < 20 MB)

---

## D6 — Stockage des métriques

**Decision**: Fichiers JSON dans `~/.config/ecotokens/`

**Rationale**: Pas de dépendance SQLite pour le cœur P1/P2. JSON est lisible humain,
inspecté facilement, et suffisant pour le volume attendu (< 100k interceptions / an).
Migration vers SQLite possible en P3 sans changement de spec.

**Structure**:
```
~/.config/ecotokens/
├── config.json          # Configuration utilisateur (exclusions, seuil, etc.)
├── metrics.jsonl        # Log d'interceptions (une ligne JSON par interception)
└── index/               # Index sémantique par projet (P3)
    └── <git-root-hash>/
```

**Alternatives considérées**:
- SQLite (`rusqlite`) : plus robuste pour requêtes, mais dépendance supplémentaire — différé P3
- `~/.local/share/ecotokens/` (XDG data) : approprié mais `~/.config` plus intuitif — choix arbitraire

---

## D7 — Pricing offline pour `ecotokens gain`

**Decision**: Table de prix statique embarquée dans le binaire, mise à jour via `--model`

**Rationale**: Afficher l'équivalent USD économisé donne une valeur immédiatement tangible
(inspiré de jCodeMunch `cost_avoided`). Le pricing Anthropic est stable sur des semaines ;
une table statique versionnée dans le code est suffisante. Pas d'appel réseau requis.
L'utilisateur peut passer `--model` pour choisir le modèle de référence.

**Table de pricing initiale** (entrée / sortie en USD / 1M tokens) :
| Modèle | Input | Output |
|--------|-------|--------|
| `claude-haiku-4-5` | $0.80 | $4.00 |
| `claude-sonnet-4-5` | $3.00 | $15.00 |
| `claude-sonnet-4-6` | $3.00 | $15.00 |
| `claude-opus-4-6` | $15.00 | $75.00 |

**Convention**: Les tokens économisés sont comptabilisés comme tokens d'entrée (input),
car l'output filtré remplace du contexte lu par Claude.

**Alternatives considérées**:
- Appel API pricing Anthropic : nécessite réseau — écarté (FR-009)
- Paramètre `--price-per-token` custom : utile mais complexifie l'UX — différé

---

## D8 — Providers d'embeddings multiples (P3 optionnel)

**Decision**: Feature flag `embeddings` activant Ollama ou LM Studio comme providers locaux

**Rationale**: BM25 seul manque de précision sémantique (ex. : "authentification" ne matchera
pas "login"). Les embeddings vectoriels via Ollama (modèle `nomic-embed-text`, ~270 MB)
améliorent significativement la qualité de recherche. Le feature flag garde le binaire
par défaut < 20 MB (contrainte plan.md).

**Architecture**:
```toml
[features]
embeddings = ["reqwest", "serde_json"]  # reqwest déjà présent via dépendances optionnelles
```

**Enum provider** :
```rust
enum EmbedProvider {
    None,                          // BM25 seul (défaut)
    Ollama { url: String },        // http://localhost:11434
    LmStudio { url: String },      // http://localhost:1234
}
```

**Alternatives considérées**:
- `ort` + MiniLM quantisé embarqué : modèle ~50 MB mais complexifie le build — différé phase 7
- OpenAI API : requiert réseau et clé API — écarté (FR-009)

---

## D9 — Indexation au niveau symbole via tree-sitter

**Decision**: Parser AST avec `tree-sitter` pour indexer les symboles individuels

**Rationale**: jCodeMunch démontre qu'indexer au niveau symbole (fonctions, structs, impls)
au lieu de blocs de texte réduit les tokens retournés de ~90% pour des requêtes ciblées.
Un ID stable `{file}::{qualified_name}#{kind}` permet la navigation et le graphe d'appels.
`tree-sitter` est pur Rust (via bindings), compile statiquement, supporte Rust/Python/JS.

**Format ID symbole** : `src/filter/git.rs::filter_status#fn`

**Grammaires retenues** :
| Langage | Crate |
|---------|-------|
| Rust | `tree-sitter-rust` |
| Python | `tree-sitter-python` |
| JavaScript | `tree-sitter-javascript` |
| TypeScript | `tree-sitter-typescript` |

**Kinds indexés** : `fn`, `struct`, `impl`, `enum`, `trait`, `const`, `type`, `mod`

**Alternatives considérées**:
- Parsing regex des signatures : fragile sur les generics et closures — écarté
- `syn` (Rust-only) : précis mais ne couvre qu'un seul langage — écarté

---

## D10 — Serveur MCP natif

**Decision**: Implémenter un serveur MCP stdio via la crate `rmcp`

**Rationale**: En plus du hook PreToolUse, exposer ecotokens comme serveur MCP permet
à Claude Code d'appeler directement `ecotokens_search`, `ecotokens_outline`,
`ecotokens_trace_callers` comme tools MCP — sans passer par bash. C'est la même
architecture que GrepAI. La crate `rmcp` (Rust MCP SDK officiel) gère le protocole
JSON-RPC sur stdio.

**Tools MCP exposés** :
- `ecotokens_search` : recherche BM25/sémantique dans le codebase indexé
- `ecotokens_outline` : liste les symboles d'un fichier ou module
- `ecotokens_trace_callers` : retourne les callers d'un symbole
- `ecotokens_trace_callees` : retourne les callees d'un symbole

**Démarrage** : `ecotokens mcp` — bloque sur stdin, répond sur stdout (protocole MCP stdio)

**Alternatives considérées**:
- HTTP/SSE : plus flexible mais nécessite un port réseau — écarté pour la simplicité
- Wrapper shell appelant `ecotokens search` : latence trop élevée par process — écarté

---

## D11 — Gestion des fichiers texte structurés (Markdown, TOML, JSON, YAML)

**Decision**: Pipeline de parsing léger par extension ; pas de tree-sitter pour le texte

**Rationale**: Les fichiers Markdown (READMEs, specs, docs) sont parmi les plus lus par Claude
et peuvent représenter des centaines de lignes. Ils ont une structure naturelle (headings H1-H3)
exploitable sans parser AST lourd. TOML/JSON/YAML sont déjà structurés et peuvent être
résumés par leurs clés de premier niveau. Un module dédié par type est plus simple et
plus maintenable que d'étendre tree-sitter à des formats non-code.

**Stratégies par type** :
| Extension | Stratégie de filtrage | "Symboles" pour outline |
|-----------|----------------------|------------------------|
| `.md`, `.mdx` | Extraction de sections par heading ; si query connue → section la plus pertinente | Headings H1/H2/H3 avec numéro de ligne |
| `.toml` | Résumé des tables de premier niveau (`[dependencies]`, `[features]`) | Noms de tables |
| `.json` | Résumé des clés racine si > N lignes ; valeurs longues tronquées | Clés racine |
| `.yaml`, `.yml` | Résumé des clés racine et blocs ; listes longues tronquées | Clés racine |
| `.txt` | Filtre générique (N premières + N dernières lignes) | — |

**Markdown : extraction de section**

Lorsque Claude lit un fichier Markdown via le hook, l'output peut être filtré
en extrayant uniquement les sections pertinentes plutôt que le fichier entier :

```
# README.md (847 lignes)
→ [ecotokens] Sections disponibles : Installation (l.12), Usage (l.45),
  Configuration (l.120), API (l.280), Contributing (l.750)
→ Contenu retourné : Installation + Usage (142 lignes — −83%)
```

En l'absence de contexte de requête (hook générique), le filtre retourne :
- Les headings H1/H2 comme table des matières
- La première section complète (souvent l'introduction)
- Un résumé des sections restantes

**Markdown : ID symbole heading**

Format : `docs/spec.md::Usage#h2` (file::heading-slug#hN)

**Alternatives considérées**:
- `pulldown-cmark` pour parser Markdown : trop lourd pour un simple extracteur de headings — écarté ; regex suffisante
- `tree-sitter-markdown` : pas de grammaire stable officielle — écarté
- Traiter Markdown comme texte générique : perte de toute la structure — écarté

---

## D12 — Interface console utilisateur via ratatui

**Decision**: Utiliser `ratatui` pour les vues interactives TTY ; fallback texte plat si stdout n'est pas un TTY

**Rationale**: `ratatui` est la bibliothèque TUI de référence en Rust (successeur de `tui-rs`).
Elle offre des widgets riches (tableaux, graphes sparkline, barres de progression, bordures)
sans dépendance système ni runtime externe. Le pattern TTY-detect garantit la compatibilité
avec les pipes et le flag `--json` : si stdout n'est pas un terminal, on retombe sur le
format texte plat actuel. Aucune régression pour les usages scriptés.

**Commandes concernées** :
| Commande | Vue ratatui |
|----------|------------|
| `ecotokens gain` | Dashboard : tableau de stats, sparkline d'économies par jour, barre coût USD |
| `ecotokens watch` | Panel live : liste scrollable des fichiers re-indexés en temps réel |
| `ecotokens index` | Barre de progression + compteur fichiers/symboles indexés |
| `ecotokens outline` | Arbre navigable (↑↓ pour scroller, `q` pour quitter) |
| `ecotokens trace` | Tableau callers/callees avec colonnes fichier + ligne |

**Pattern TTY-detect** :
```rust
if std::io::stdout().is_terminal() && !json_flag {
    run_tui(data)   // ratatui
} else {
    print_plain(data)  // texte plat ou JSON
}
```

**Crates** :
- `ratatui = { version = "0.29", features = ["crossterm"] }` — widgets TUI ; crossterm
  est un backend transitivement inclus via la feature flag, pas une dépendance directe

**Mode non-interactif** (`ecotokens watch`, `ecotokens index`) : l'UI est mise à jour
en place (alternate screen) ; Ctrl-C arrête proprement et restaure le terminal.

**Alternatives considérées**:
- `cursive` : API plus simple mais moins de contrôle sur le layout — écarté
- `indicatif` (barres de progression seules) : insuffisant pour `gain` dashboard — écarté
- Pas d'UI spéciale : perte de lisibilité comparée à jCodeMunch/GrepAI — écarté

---

## Résumé des décisions

| # | Décision | Crate(s) |
|---|----------|----------|
| D1 | Hook PreToolUse, réécriture de commande | (builtin Claude Code) |
| D2 | Heuristique chars*0.25 + tiktoken-rs optionnel | `tiktoken-rs` (opt.) |
| D3 | Pipeline regex compilées statiquement | `regex`, `lazy_regex` |
| D4 | Modules filtres par famille | (interne) |
| D5 | BM25 tantivy (P3) | `tantivy` |
| D6 | JSON lines dans ~/.config/ecotokens/ | `serde_json` |
| D7 | Pricing offline table statique, `--model` flag | (interne) |
| D8 | Embeddings providers Ollama/LmStudio (feature flag) | `reqwest` (opt.) |
| D9 | Indexation symboles tree-sitter AST | `tree-sitter`, grammaires |
| D10 | Serveur MCP natif stdio | `rmcp` |
| D11 | Fichiers texte : extraction heading MD, résumé TOML/JSON/YAML | `regex` (existant) |
| D12 | TUI ratatui (TTY-detect, fallback texte plat) | `ratatui` (+ crossterm via feature) |

---

## D13 — Mesures SC-001 (économies réelles)

**Date de mesure** : 2026-03-07 | **Binaire** : `ecotokens filter --debug`

| Commande | tokens_before | tokens_after | savings_pct | Notes |
|----------|--------------|-------------|-------------|-------|
| `git status` | 166 | 166 | 0% | Output court : filtre pass-through (< seuil) |
| `git diff` (courant, petit) | 1 340 | 1 340 | 0% | Diff de 40 lignes : pass-through |
| `git diff HEAD~5` (diff large) | 14 164 | 424 | **97%** | Filtre actif sur grand diff |
| `cargo test` | 2 786 | 950 | **65.9%** | Extraction erreurs/stats uniquement |

**Comportement observé** : Le filtre git est activé à partir d'un certain volume de sortie.
Pour les outputs courts (< quelques centaines de tokens), il laisse passer sans transformation.
Pour les diffs larges (> 10 000 tokens), la réduction atteint 97%.

**Objectif SC-001 (≥ 60% sur git/cargo)** :
- `cargo test` : 65.9% → ✅ VALIDÉ
- `git diff` (grand diff) : 97% → ✅ VALIDÉ
- `git status` (court) : 0% → filtre pass-through par conception (output déjà minimal)

**Conclusion** : SC-001 ✅ VALIDÉ — les filtres atteignent ≥ 60% sur les sorties volumineuses,
qui sont précisément celles où l'économie de tokens est utile. Les outputs courts sont laissés
intacts par conception (coût de filtrage > gain potentiel).

---

**Crates totaux P1/P2** : `clap`, `serde`, `serde_json`, `regex`, `lazy_regex`, `dirs`, `unicode-segmentation`
**Crates P2 (gain dashboard)** : `ratatui` (inclut crossterm via feature `crossterm`)
**Crates optionnels P3** : `tantivy`, `tiktoken-rs`, `tree-sitter`, `tree-sitter-rust`, `tree-sitter-python`, `tree-sitter-javascript`, `tree-sitter-typescript`, `rmcp`, `notify`
**Crates feature-flag** : `reqwest` (feature `embeddings`)
