# Code Review Report

**Date**: 2026-04-28
**Workspace**: /var/home/hansi/Code/ecotokens
**Files reviewed**: 64
**Total findings**: 17 (🔴 0 critical · 🟡 17 improvements · 🔵 0 nitpicks)

---

## Executive Summary

La base de code est bien structurée, avec une couverture de tests solide et une séparation claire des responsabilités entre les modules. Aucun point critique ni nitpick n'est présent dans l'état actuel du working tree — de nombreuses corrections ont déjà été apportées dans le branch courant. Les 17 améliorations restantes concernent principalement la robustesse de l'index incrémental (les symboles supprimés ne sont pas nettoyés par le watcher), les limites codées en dur à 10 000 symboles dans les modules trace/duplicates, et la détection de callers/callees basée sur une simple correspondance textuelle qui génère des faux positifs.

---

## Findings by file

### `src/mcp/server.rs`

| Severity | Line | Finding |
|----------|------|---------|
| 🟡 | 136 | `ecotokens_duplicates` retourne `format!("Error: {e}")` (texte brut) en cas d'erreur, contrairement aux cinq autres handlers qui retournent du JSON (`json!({"error": …})`). Un client MCP attendant du JSON recevra une chaîne non parseable. |
| 🟡 | 157 | `git_root()` dupliqué à l'identique dans `src/main.rs:1322`. Devrait être un helper partagé dans `src/config/mod.rs` ou un module utilitaire. |

---

### `src/metrics/store.rs`

| Severity | Line | Finding |
|----------|------|---------|
| 🟡 | 185 | `DB_INIT_LOCK…lock().unwrap()` panique si le mutex est empoisonné (thread précédent en panique). Préférer `.lock().unwrap_or_else(\|e\| e.into_inner())`. |
| 🟡 | 339 | `filter_map(\|r\| r.ok())` dans `read_from` : les erreurs de désérialisation SQLite sont silencieusement ignorées. Des entrées corrompues disparaissent sans trace dans les logs. |

---

### `src/search/index.rs`

| Severity | Line | Finding |
|----------|------|---------|
| 🟡 | 71 | Indexation incrémentale : `writer.delete_all_documents()` puis réinsertion de **tous** les fichiers. C'est O(n) sur l'ensemble du projet à chaque appel, même pour un seul fichier modifié. |

---

### `src/daemon/watcher.rs`

| Severity | Line | Finding |
|----------|------|---------|
| 🟡 | 117 | `index.writer(50_000_000)` ouvert pour chaque fichier modifié : 50 MB de buffer alloués à chaque event. Pour des modifications fréquentes (sauvegarde automatique), cela peut épuiser la RAM. |
| 🟡 | 123 | `writer.delete_term(term)` supprime uniquement les chunks BM25 (`kind="bm25"`) du fichier. Les documents de symboles (`kind="symbol"`) ne sont pas nettoyés lors du reindex incrémental : les symboles supprimés ou renommés persistent dans l'index indéfiniment. |

---

### `src/trace/callers.rs`

| Severity | Line | Finding |
|----------|------|---------|
| 🟡 | 31 | `TopDocs::with_limit(10_000)` codé en dur : pour un projet avec > 10k symboles, des callers sont manqués silencieusement. |
| 🟡 | 33 | `call_pattern = format!("{symbol_name}(")` : détection textuelle naïve. Matche aussi les occurrences dans des commentaires, des string literals, ou des noms de méthodes avec un suffixe (ex: `other_fn_name(`). |

---

### `src/trace/callees.rs`

| Severity | Line | Finding |
|----------|------|---------|
| 🟡 | 35 | Même limite `TopDocs::with_limit(10_000)` codée en dur que dans `callers.rs`. |
| 🟡 | 112 | Même faux-positif textuel : `format!("{known}(")` matche dans commentaires/strings. |

---

### `src/search/query.rs`

| Severity | Line | Finding |
|----------|------|---------|
| 🟡 | 40 | `query_parser.parse_query(&opts.query)?` : les caractères spéciaux Tantivy (`:`, `+`, `"`, etc.) dans la requête provoquent une erreur remontée à l'utilisateur. Devrait avoir un fallback en recherche littérale. |
| 🟡 | 88 | Mélange BM25 + sémantique (`0.5 * bm25_score + 0.5 * sem_score`) sans normalisation du score BM25. Le score BM25 peut être >> 1.0 et dominer le score cosinus normalisé [0,1] (limitation documentée en commentaire mais non corrigée). |

---

### `src/search/symbols.rs`

| Severity | Line | Finding |
|----------|------|---------|
| 🟡 | 52 | TypeScript/TSX parsé avec `tree_sitter_javascript`. Les constructions TS spécifiques (interfaces, types génériques, decorators) ne sont pas reconnues comme déclarations. |

---

### `src/duplicates/detect.rs`

| Severity | Line | Finding |
|----------|------|---------|
| 🟡 | 51 | `TopDocs::with_limit(10_000)` : même limite silencieuse que trace. |
| 🟡 | 103 | Algorithme pairwise O(n²) : avec 1 000 symboles → ~500k comparaisons. Pas de barre de progression ni d'avertissement de durée. |

---

### `src/metrics/report.rs`

| Severity | Line | Finding |
|----------|------|---------|
| 🟡 | 54 | `Settings::load()` appelé à chaque appel de `pricing_usd_per_1m`, lequel est appelé depuis `aggregate`. Dans le TUI (reload toutes les 10 s), cela lit le fichier config à chaque refresh. Devrait recevoir les settings en paramètre. |

---

### `src/main.rs`

| Severity | Line | Finding |
|----------|------|---------|
| 🟡 | 1314 | `glob_matches` implémentation simpliste : `*.rs` est supporté via `ends_with`, mais `src/**/*.rs` ou `**/mod.rs` ne fonctionnent pas. Aucun avertissement si un pattern non supporté est passé via `--include`/`--exclude`. |
| 🟡 | 2046 | `DefaultHasher` pour le fingerprint du fichier log : non-stable entre versions Rust. Le nom du fichier log peut changer après une mise à jour, laissant des fichiers `.log` orphelins dans `~/.config/ecotokens/`. |

---

### `src/abbreviations/mod.rs`

| Severity | Line | Finding |
|----------|------|---------|
| 🟡 | 83 | `text.split("``\`")` pour détecter les blocs de code : si le texte contient un nombre impair de ` ``` ` (bloc de code non fermé), le flag `in_code` se désynchronise et des portions de texte normal seront traitées comme code. |

---

## Consolidated recommendations

### 🟡 Improvements — should fix

Ordonnées par impact (priorité décroissante) :

1. **[File: src/daemon/watcher.rs, l.123]** Lors du reindex incrémental d'un fichier, supprimer aussi les documents `kind="symbol"` du fichier en plus des chunks BM25. Sans ça, les symboles supprimés restent dans l'index et polluent les résultats trace/outline.

2. **[File: src/search/index.rs, l.71]** Revoir la stratégie d'indexation incrémentale pour ne réindexer que les fichiers modifiés (comparer par timestamp ou hash), plutôt que de supprimer et réinsérer la totalité du corpus.

3. **[File: src/daemon/watcher.rs, l.117]** Réduire le buffer du writer pour le reindex incrémental (ex: 8–16 MB) ou réutiliser un writer persistant pour éviter d'allouer 50 MB par événement fichier.

4. **[File: src/trace/callers.rs:31 + callees.rs:35 + duplicates/detect.rs:51]** Remplacer `TopDocs::with_limit(10_000)` par une constante nommée configurable, et émettre un warning si le nombre de résultats atteint la limite.

5. **[File: src/trace/callers.rs:33 + callees.rs:112]** Améliorer la détection de call en excluant les lignes qui sont des commentaires (`//`, `#`, `--`) ou dans des string literals, afin de réduire les faux positifs.

6. **[File: src/mcp/server.rs, l.136]** Dans `ecotokens_duplicates`, remplacer `format!("Error: {e}")` par `json!({"error": e.to_string()}).to_string()` pour aligner le format d'erreur avec les cinq autres handlers MCP.

7. **[File: src/search/query.rs, l.40]** Ajouter un fallback : si `parse_query` échoue (caractères spéciaux), ré-essayer avec la requête échappée via `QueryParser::escape` ou une `TermQuery` littérale.

8. **[File: src/metrics/store.rs, l.185]** Remplacer `.lock().unwrap()` par `.lock().unwrap_or_else(|e| e.into_inner())` pour ne pas paniquer si un thread précédent a planté pendant une écriture DB.

9. **[File: src/metrics/store.rs, l.339]** Logger les erreurs de désérialisation ignorées dans `read_from` (au moins via `eprintln!`) pour faciliter le diagnostic de corruption.

10. **[File: src/metrics/report.rs, l.54]** Passer `settings: &Settings` en paramètre à `pricing_usd_per_1m` au lieu de charger le fichier config à chaque appel.

11. **[File: src/search/symbols.rs, l.52]** Ajouter `tree_sitter_typescript` pour les extensions `.ts`/`.tsx` afin d'extraire correctement les interfaces, types et decorators TypeScript.

12. **[File: src/main.rs, l.1314]** Utiliser la crate `glob` pour implémenter `glob_matches` et supporter les patterns `**/*.rs`, ou documenter explicitement que seuls `*.ext` et nom exact sont supportés.

13. **[File: src/main.rs, l.2046]** Remplacer `DefaultHasher` par un hash déterministe stable (ex: `fnv` ou une constante de seed fixée via `SipHasher` avec seed 0) pour garantir la stabilité des noms de fichiers log entre versions.

14. **[File: src/abbreviations/mod.rs, l.83]** Rendre la détection de bloc de code plus robuste : compter les occurrences de ` ``` ` par ligne ou utiliser un split avec état pour éviter la désynchronisation sur les blocs non fermés.

15. **[File: src/duplicates/detect.rs, l.103]** Ajouter une barre de progression ou un warning de durée estimée pour l'algorithme O(n²) quand `n > 500`.

16. **[File: src/mcp/server.rs, l.157]** Déplacer `git_root()` dans un module partagé (ex: `src/config/mod.rs`) et dédupliquer les deux implémentations identiques.

17. **[File: src/search/query.rs, l.88]** Normaliser le score BM25 (ex: diviser par le score max observé) avant de le mélanger avec le score cosinus pour un re-ranking équitable.

---

## Files with no findings

- `src/abbreviations/dictionary.rs`
- `src/config/mod.rs`
- `src/config/settings.rs`
- `src/daemon/mod.rs`
- `src/duplicates/mod.rs`
- `src/duplicates/proposals.rs`
- `src/duplicates/staleness.rs`
- `src/filter/ai_summary.rs`
- `src/filter/aws.rs`
- `src/filter/cargo.rs`
- `src/filter/config_file.rs`
- `src/filter/container.rs`
- `src/filter/cpp.rs`
- `src/filter/db.rs`
- `src/filter/fs.rs`
- `src/filter/generic.rs`
- `src/filter/gh.rs`
- `src/filter/git.rs`
- `src/filter/go.rs`
- `src/filter/js.rs`
- `src/filter/markdown.rs`
- `src/filter/mod.rs`
- `src/filter/network.rs`
- `src/filter/python.rs`
- `src/hook/post_handler.rs`
- `src/install.rs`
- `src/lib.rs`
- `src/masking/mod.rs`
- `src/masking/patterns.rs`
- `src/mcp/mod.rs`
- `src/mcp/tools.rs`
- `src/search/embed.rs`
- `src/search/mod.rs`
- `src/search/outline.rs`
- `src/search/text_docs.rs`
- `src/tokens/counter.rs`
- `src/tokens/mod.rs`
- `src/trace/mod.rs`
- `src/tui/gain.rs`
- `src/tui/mod.rs`
- `src/tui/outline.rs`
- `src/tui/progress.rs`
- `src/tui/trace.rs`
- `src/tui/watch.rs`
