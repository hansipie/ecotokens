---
description: "Task list for ecotokens — outil compagnon d'économie de tokens pour Claude Code"
---

# Tasks: Outil compagnon d'économie de tokens pour Claude Code

**Input**: Design documents from `/specs/001-token-companion/`
**Prerequisites**: plan.md ✅ spec.md ✅ research.md ✅ data-model.md ✅ contracts/ ✅

**Tests**: ⚠️ OBLIGATOIRES — Constitution principe III (Test-First NON-NEGOTIABLE). Les tests
doivent être écrits et ÉCHOUER avant toute implémentation.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Peut tourner en parallèle (fichiers différents, pas de dépendances incomplètes)
- **[Story]**: User story cible (US1 / US2 / US3)

---

## Phase 1: Setup (Infrastructure partagée)

**Purpose**: Initialisation du projet Rust et structure de base

- [X] T001 Créer le projet Rust avec `cargo new --bin ecotokens` à la racine du dépôt
- [X] T002 Configurer `Cargo.toml` avec les dépendances P1/P2 : `clap`, `serde`, `serde_json`, `regex`, `lazy_regex`, `dirs`, `toml`
- [X] T003 [P] Configurer `clippy.toml` et `.rustfmt.toml` (strict mode, Rust stable only)
- [X] T004 [P] Créer la structure de répertoires `src/` conforme à `plan.md` : `filter/`, `masking/`, `metrics/`, `tokens/`, `hook/`, `config/`
- [X] T005 [P] Créer la structure de répertoires `tests/` : `filter/`, `masking/`, `metrics/`, `hook/`, `integration/`, `tokens/`, `config/`, `search/`, `trace/`, `tui/`
- [X] T006 Implémenter le point d'entrée `src/main.rs` avec la structure `clap` (sous-commandes vides : `hook`, `filter`, `gain`, `install`, `uninstall`, `config`)
- [X] T007 [P] Créer `src/config/mod.rs` et `src/config/settings.rs` : struct `Settings` avec tous les champs du data-model, sérialisation JSON, valeurs par défaut
- [X] T008 [P] Créer `src/tokens/mod.rs` et `src/tokens/counter.rs` : fonction `estimate_tokens(text: &str) -> usize` (heuristique chars * 0.25)

**Checkpoint**: `cargo build` réussit, `cargo clippy` sans warnings, structure complète

---

## Phase 2: Fondational (Prérequis bloquants)

**Purpose**: Infrastructure cœur requise par TOUTES les user stories

**⚠️ CRITIQUE**: Aucune user story ne peut démarrer avant la fin de cette phase

- [X] T009 [P] Tests unitaires pour `src/tokens/counter.rs` dans `tests/tokens/counter_test.rs` : vérifier estimation sur texte vide, texte court, texte > 1000 chars — DOIT ÉCHOUER avant T010
- [X] T010 Implémenter `estimate_tokens` dans `src/tokens/counter.rs` jusqu'à ce que T009 passe (Red-Green)
- [X] T011 [P] Tests unitaires pour `src/config/settings.rs` dans `tests/config/settings_test.rs` : lecture config inexistante → valeurs par défaut, lecture config valide, valeurs hors limites rejetées — DOIT ÉCHOUER avant T012
- [X] T012 Implémenter chargement/sauvegarde `Settings` dans `src/config/settings.rs` via `dirs::config_dir()` jusqu'à ce que T011 passe
- [X] T013 [P] Tests unitaires pour `src/masking/patterns.rs` dans `tests/masking/patterns_test.rs` : chaque pattern (AWS key, GitHub PAT, Bearer, PEM, .env secrets, JWT, URL credentials) détecte et masque — DOIT ÉCHOUER avant T014
- [X] T014 Implémenter `src/masking/mod.rs` et `src/masking/patterns.rs` : pipeline de regex `lazy_regex`, fonction `mask(text: &str) -> (String, bool)` jusqu'à ce que T013 passe
- [X] T015 [P] Tests unitaires pour le filtre générique dans `tests/filter/generic_test.rs` : output < seuil → passthrough, output > 500 lignes → résumé N premières + N dernières, output > 50 Ko → résumé — DOIT ÉCHOUER avant T016
- [X] T015b [P] Tests unitaires filtre markdown dans `tests/filter/markdown_test.rs` : fichier < 200 lignes → passthrough, fichier > 200 lignes → ToC headings + première section, heading H2 ciblé → section extraite, fichier sans headings → filtre générique — DOIT ÉCHOUER avant T016b
- [X] T015c [P] Tests unitaires filtre config_file dans `tests/filter/config_file_test.rs` : TOML → liste tables premier niveau + compte clés, JSON > 100 lignes → clés racine + valeurs scalaires, YAML → même stratégie — DOIT ÉCHOUER avant T016c
- [X] T016 Implémenter `src/filter/mod.rs` et `src/filter/generic.rs` : détection famille commande (extension fichier incluse), résumé générique, passthrough jusqu'à ce que T015 passe
- [X] T016b [P] Implémenter `src/filter/markdown.rs` : parsing headings par regex `^#{1,3} `, extraction section par index, génération ToC jusqu'à ce que T015b passe
- [X] T016c [P] Implémenter `src/filter/config_file.rs` : résumé TOML (tables), JSON (clés racine), YAML (clés racine) jusqu'à ce que T015c passe

**Checkpoint**: `cargo test` — tous les tests fondamentaux passent (T009, T011, T013, T015)

---

## Phase 3: User Story 1 — Installation transparente et activation automatique (Priority: P1) 🎯 MVP

**Goal**: Le hook `PreToolUse` intercepte les commandes bash, les filtre, et transmet l'output réduit à Claude — zéro action utilisateur après `ecotokens install`.

**Independent Test**: `ecotokens install` → lancer `git status` dans Claude Code → vérifier dans `~/.config/ecotokens/metrics.jsonl` qu'une interception est enregistrée avec `tokens_after < tokens_before`.

### Tests pour User Story 1 — écrire en premier, vérifier qu'ils ÉCHOUENT

- [X] T017 [P] [US1] Tests unitaires hook dans `tests/hook/handler_test.rs` : parsing JSON stdin valide, commande exclue → passthrough JSON, commande non-exclue → JSON avec `updatedInput` réécrit, modifier exclusion list dans config.json puis ré-invoquer → nouvelle liste prise en compte sans redémarrage — DOIT ÉCHOUER avant T021
- [X] T017 [P] [US1] Tests unitaires filtre git dans `tests/filter/git_test.rs` : `git status` propre → résumé court, `git status` avec fichiers modifiés → sections pertinentes conservées, `git diff` long → tronqué — DOIT ÉCHOUER avant T022
- [X] T017 [P] [US1] Tests unitaires filtre cargo dans `tests/filter/cargo_test.rs` : `cargo build` succès → stats seulement, `cargo build` avec erreurs → erreurs conservées, warnings → résumé comptage — DOIT ÉCHOUER avant T023
- [X] T020 [P] [US1] Tests unitaires filtre fs dans `tests/filter/fs_test.rs` : `ls -la` court → passthrough, `ls -la` long → tronqué à N entrées, `find` > 500 lignes → résumé — DOIT ÉCHOUER avant T024
- [X] T020 [US1] Intégration : test `ecotokens install` dans `tests/integration/install_test.rs` : vérifie écriture dans `~/.claude/settings.json` et création `~/.config/ecotokens/config.json` — DOIT ÉCHOUER avant T028

### Implémentation User Story 1

- [X] T020 [P] [US1] Implémenter `src/filter/git.rs` : filtres pour `git status`, `git diff`, `git log`, `git show` jusqu'à ce que T018 passe
- [X] T020 [P] [US1] Implémenter `src/filter/cargo.rs` : filtres pour `cargo build`, `cargo test`, `cargo check`, `cargo clippy` jusqu'à ce que T019 passe
- [X] T020 [P] [US1] Implémenter `src/filter/fs.rs` : filtres pour `ls`, `find`, `tree` jusqu'à ce que T020 passe
- [X] T020 [US1] Implémenter `src/hook/mod.rs` et `src/hook/handler.rs` : lecture stdin JSON, détection exclusion, réécriture commande → `ecotokens filter -- <cmd>`, output JSON PreToolUse jusqu'à ce que T017 passe
- [X] T020t [P] [US1] Tests unitaires metrics store dans `tests/metrics/store_test.rs` :
  écriture d'une `Interception` dans JSONL → fichier créé et ligne valide JSON,
  lecture fichier existant → vec<Interception> correct, fichier absent → Ok (crée vide),
  deux appels successifs → deux lignes distinctes, champ `savings_pct` = 0 si mode passthrough
  — DOIT ÉCHOUER avant T026
- [X] T020 [US1] Implémenter `src/metrics/mod.rs` et `src/metrics/store.rs` : struct `Interception`, écriture append-only dans `~/.config/ecotokens/metrics.jsonl`
- [X] T020 [US1] Implémenter sous-commande `ecotokens filter` dans `src/main.rs` : exécution commande originale, pipeline filtre famille → masquage → comptage tokens → enregistrement métriques → stdout
- [X] T020 [US1] Implémenter sous-commande `ecotokens install` dans `src/main.rs` : écriture hook dans `~/.claude/settings.json` (idempotent), création config par défaut jusqu'à ce que T021 passe
- [X] T020t [P] [US1] Tests unitaires uninstall dans `tests/integration/install_test.rs` :
  hook présent → suppression de l'entrée ecotokens uniquement (autres hooks intacts),
  hook absent → Ok sans erreur (idempotent), config ~/.config/ecotokens/ intacte après
  uninstall — DOIT ÉCHOUER avant T029
- [X] T020 [US1] Implémenter sous-commande `ecotokens uninstall` dans `src/main.rs` : suppression entrée hook de `~/.claude/settings.json`
- [X] T030t [P] [US1] Tests unitaires --debug dans `tests/hook/handler_test.rs` :
  flag --debug activé → lignes "[ecotokens debug]" sur stderr avec command/tokens/savings,
  flag absent → stderr vide pour les métriques, stdout identique avec ou sans --debug
  — DOIT ÉCHOUER avant T030
- [X] T030 [US1] Implémenter mode `--debug` dans `src/hook/handler.rs` et `ecotokens filter` : affichage command/tokens_before/tokens_after/savings/duration_ms sur stderr

**Checkpoint**: User Story 1 complète — `ecotokens install && git status` dans Claude Code réduit les tokens ≥ 60% pour git

---

## Phase 4: User Story 2 — Visualisation des économies réalisées (Priority: P2)

**Goal**: `ecotokens gain` affiche un rapport lisible et JSON des tokens économisés (total, par commande, par période).

**Independent Test**: Après ≥ 5 interceptions, `ecotokens gain --by-command` affiche un tableau cohérent avec les entrées de `metrics.jsonl` ; `ecotokens gain --json` retourne un JSON valide parseable.

### Tests pour User Story 2 — écrire en premier, vérifier qu'ils ÉCHOUENT

- [ ] T031 [P] [US2] Tests unitaires rapport dans `tests/metrics/report_test.rs` : agrégation period=all, period=today, period=week ; calcul savings_pct ; groupement by_family ; groupement by_project — DOIT ÉCHOUER avant T033
- [ ] T032 [P] [US2] Tests unitaires format sortie dans `tests/metrics/report_test.rs` : format humain aligné, format JSON valide, historique ordonné par date décroissante — DOIT ÉCHOUER avant T034

### Implémentation User Story 2

- [ ] T033 [US2] Implémenter `src/metrics/report.rs` : lecture `metrics.jsonl`, agrégation par période/famille/projet, struct `Report` jusqu'à ce que T031 passe
- [ ] T034 [US2] Implémenter sous-commande `ecotokens gain` dans `src/main.rs` : options `--period`, `--by-project`, `--by-command`, `--history`, `-n`, `--json`, `--model` jusqu'à ce que T032 passe

### Coût USD dans `ecotokens gain` (P1 — ajout à US2)

- [ ] T034a [P] [US2] Tests unitaires pricing dans `tests/metrics/report_test.rs` : calcul `cost_avoided_usd` pour modèle sonnet (tokens_saved × 3.00/1M), modèle inconnu → erreur claire, override `--model opus` → tarif opus — DOIT ÉCHOUER avant T034b
- [ ] T034b [US2] Implémenter table de pricing statique dans `src/metrics/report.rs` et calcul `cost_avoided_usd` dans `Report` jusqu'à ce que T034a passe

### TUI ratatui pour `ecotokens gain` (ajout à US2)

- [ ] T034c [P] [US2] Tests unitaires TTY-detect dans `tests/tui/tui_test.rs` : stdout non-TTY → retourne texte plat, flag `--json` → JSON sans TUI, flag `--no-tui` → texte plat même sur TTY — DOIT ÉCHOUER avant T034d
- [ ] T034d [US2] Créer `src/tui/mod.rs` : fonction `is_tui_enabled(json: bool, no_tui: bool) -> bool` + TTY-detect via `std::io::IsTerminal` jusqu'à ce que T034c passe
- [ ] T034et [P] [US2] Tests TUI gain dans `tests/tui/gain_test.rs` :
  rendu avec TestBackend sur données fixtures → buffer contient "Savings", "Cost avoided",
  sparkline présente sur 14 jours, fallback texte sans panic si data vide
  — DOIT ÉCHOUER avant T034e
- [ ] T034e [US2] Implémenter `src/tui/gain.rs` : dashboard ratatui avec tableau stats, barres par famille (Block + Gauge), sparkline économies 14 jours, ligne coût USD — jusqu'à ce que `ecotokens gain` en TTY affiche le dashboard

**Checkpoint**: User Stories 1 ET 2 fonctionnelles — `ecotokens gain` affiche le dashboard TUI sur TTY, texte plat en pipe/--json

---

## Phase 5: User Story 3 — Recherche sémantique dans le codebase (Priority: P3)

**Goal**: `ecotokens index` indexe le codebase courant ; `ecotokens search <query>` retourne les extraits pertinents au lieu de fichiers entiers.

**Independent Test**: Après `ecotokens index`, `ecotokens search "gestion des erreurs"` retourne des extraits avec score de pertinence et chemin fichier, en moins de 3 secondes sur un projet de 10k lignes.

**Note**: Ajouter la dépendance `tantivy = "0.21"` à `Cargo.toml` pour cette phase.

### Tests pour User Story 3 — écrire en premier, vérifier qu'ils ÉCHOUENT

- [ ] T035 [P] [US3] Tests unitaires index dans `tests/search/index_test.rs` : indexation d'un répertoire vide, indexation d'un projet test fixture, index existant → mise à jour incrémentale — DOIT ÉCHOUER avant T037
- [ ] T036 [P] [US3] Tests unitaires recherche dans `tests/search/search_test.rs` : query correspondant à un fichier connu retourne ce fichier en top-3, codebase non indexé → message d'erreur clair — DOIT ÉCHOUER avant T038

### Implémentation User Story 3

- [ ] T037 [US3] Implémenter `src/search/mod.rs` et `src/search/index.rs` : indexation BM25 via tantivy, découverte fichiers (respect `.gitignore`), stockage dans `~/.config/ecotokens/index/<hash>/` jusqu'à ce que T035 passe
- [ ] T038 [US3] Implémenter sous-commande `ecotokens search` dans `src/main.rs` : query → top-K extraits avec score et chemin, formats humain et JSON jusqu'à ce que T036 passe
- [ ] T039t [P] [US3] Tests CLI index dans `tests/integration/install_test.rs` :
  `ecotokens index --path <tmpdir>` → indexe uniquement ce répertoire (pas la racine),
  `ecotokens index --reset` → supprime l'index existant puis recrée à zéro,
  `ecotokens index` sans args → indexe le répertoire courant,
  progression affichée sur stderr (non capturée dans stdout), stats finales sur stdout
  — DOIT ÉCHOUER avant T039
- [ ] T039 [US3] Implémenter sous-commande `ecotokens index` dans `src/main.rs` : options `--path`, `--reset`, affichage progression et stats

**Checkpoint**: Toutes les User Stories fonctionnelles indépendamment

---

## Phase 5 (étendue): User Story 3 — Indexation symbolique via tree-sitter (P3)

**Goal**: Indexer les symboles individuels (fonctions, structs, impls) via tree-sitter AST
pour réduire les tokens de ~90% sur les requêtes ciblées. Complémente l'index BM25 existant.

**Independent Test**: Après `ecotokens index`, `ecotokens outline src/filter/git.rs` retourne
la liste des fonctions sans lire le fichier ; `ecotokens symbol src/filter/git.rs::filter_status#fn`
retourne l'extrait source en < 50ms.

**Note**: Ajouter `tree-sitter`, `tree-sitter-rust`, `tree-sitter-python`, `tree-sitter-javascript`
à `Cargo.toml` pour cette phase.

### Tests pour indexation symbolique — écrire en premier, vérifier qu'ils ÉCHOUENT

- [ ] T046 [P] [US3] Tests unitaires symbols dans `tests/search/symbols_test.rs` : parsing Rust → extraction fn/struct/impl avec ID stables, fichier vide → vec vide, fichier multi-language → erreur gracieuse — DOIT ÉCHOUER avant T049
- [ ] T047 [P] [US3] Tests unitaires outline dans `tests/search/outline_test.rs` : outline fichier → liste symboles triés par ligne, outline répertoire → symboles de tous les fichiers avec `--depth 1`, filtre `--kinds fn` → seulement les fonctions — DOIT ÉCHOUER avant T050
- [ ] T048 [P] [US3] Tests unitaires symbol lookup dans `tests/search/symbols_test.rs` : ID valide → source_snippet, ID invalide → erreur claire, ID après modification fichier → version indexée retournée — DOIT ÉCHOUER avant T051

### Implémentation indexation symbolique

- [ ] T048b [P] [US3] Tests unitaires text_docs dans `tests/search/text_docs_test.rs` : indexation README.md → symbols avec kind h1/h2/h3, `ecotokens outline README.md` → liste headings, indexation Cargo.toml → symbols avec kind `table`, query "installation" → section Installation retournée en top-1 — DOIT ÉCHOUER avant T049b
- [ ] T049 [US3] Implémenter `src/search/symbols.rs` : parsing AST tree-sitter par langage, extraction Symbol avec ID stable `{file}::{name}#{kind}`, stockage dans index tantivy jusqu'à ce que T046 passe
- [ ] T049b [US3] Implémenter `src/search/text_docs.rs` : extraction headings Markdown par regex, extraction clés racine TOML/JSON/YAML, génération Symbol avec kinds h1/h2/h3/table/key, stockage dans index tantivy jusqu'à ce que T048b passe
- [ ] T050 [US3] Implémenter `src/search/outline.rs` : requête index symboles par fichier/répertoire, tri par ligne, filtre par kind jusqu'à ce que T047 passe
- [ ] T051 [US3] Implémenter sous-commandes `ecotokens outline` et `ecotokens symbol` dans `src/main.rs` jusqu'à ce que T047 et T048 passent
- [ ] T051at [P] [US3] Tests TUI outline dans `tests/tui/outline_test.rs` :
  rendu avec TestBackend → liste de symboles affichée, navigation ↑↓ ne panic pas,
  liste vide → message "No symbols found"
  — DOIT ÉCHOUER avant T051a
- [ ] T051a [US3] Implémenter `src/tui/outline.rs` : liste scrollable ratatui (↑↓ + Enter pour snippet, q pour quitter), fallback texte plat si non-TTY
- [ ] T052t [P] [US3] Tests TUI progress et orchestration dual-index dans `tests/tui/progress_test.rs` :
  rendu barre de progression avec TestBackend → pourcentage affiché, 100% → barre pleine,
  `ecotokens index` sur fixture → BM25 ET symbolique tous deux déclenchés (vérifier entrées
  dans index tantivy des deux types), progression affichée sur stderr sans paniquer
  — DOIT ÉCHOUER avant T052
- [ ] T052 [US3] Étendre `ecotokens index` pour déclencher l'indexation symbolique en parallèle de l'indexation BM25 avec barre de progression `src/tui/progress.rs`

**Checkpoint**: `ecotokens outline src/` et `ecotokens symbol <id>` fonctionnels sur un projet Rust

---

## Phase 5.5: User Story 4 — Navigation structurelle : trace et MCP (P2)

**Goal**: Graphe d'appels (`ecotokens trace callers/callees`) et serveur MCP natif
(`ecotokens mcp`) permettant à Claude d'appeler directement les tools sans passer par bash.

**Independent Test (trace)**: `ecotokens trace callers filter_status` retourne les callers
avec fichier et ligne en < 200ms. `ecotokens trace callees filter_status` retourne les callees.

**Independent Test (MCP)**: Configurer `ecotokens mcp` comme serveur MCP dans Claude Code ;
appel du tool `ecotokens_outline` → réponse identique à la CLI.

**Note**: Ajouter `rmcp` à `Cargo.toml` pour cette phase.

### Tests pour US4 — écrire en premier, vérifier qu'ils ÉCHOUENT

- [ ] T053 [P] [US4] Tests unitaires callers dans `tests/trace/trace_test.rs` : symbole avec 2 callers → retourne 2 CallEdges, symbole sans callers → vec vide, symbole inconnu → erreur claire — DOIT ÉCHOUER avant T056
- [ ] T054 [P] [US4] Tests unitaires callees dans `tests/trace/trace_test.rs` : symbole appelant 3 autres → retourne 3 CallEdges, `--depth 2` → retourne les transitifs — DOIT ÉCHOUER avant T057
- [ ] T055 [P] [US4] Tests unitaires MCP server dans `tests/integration/mcp_test.rs` :
  démarrage serveur → répond au handshake MCP,
  appel `ecotokens_outline` sur fixture → JSON valide avec liste de symboles,
  appel `ecotokens_search` sur fixture → JSON valide avec extraits et scores,
  appel `ecotokens_symbol` avec ID valide → JSON avec source_snippet,
  appel `ecotokens_trace_callers` sur symbole connu → JSON avec CallEdges,
  appel `ecotokens_trace_callees` sur symbole connu → JSON avec CallEdges,
  appel tool inconnu → erreur MCP standard (code -32601)
  — DOIT ÉCHOUER avant T059

### Implémentation US4

- [ ] T056 [US4] Implémenter `src/trace/callers.rs` : construction CallEdge lors de l'indexation AST (tree-sitter), requête "qui appelle X" sur l'index jusqu'à ce que T053 passe
- [ ] T057 [US4] Implémenter `src/trace/callees.rs` : requête "qu'appelle X" sur l'index, support `--depth` récursif jusqu'à ce que T054 passe
- [ ] T058 [US4] Implémenter sous-commandes `ecotokens trace callers` et `ecotokens trace callees` dans `src/main.rs`
- [ ] T058at [P] [US4] Tests TUI trace dans `tests/tui/trace_test.rs` :
  rendu avec TestBackend → colonnes caller/fichier/ligne présentes, vec vide → message
  "No callers found", --depth 2 → résultats transitifs affichés
  — DOIT ÉCHOUER avant T058a
- [ ] T058a [US4] Implémenter `src/tui/trace.rs` : tableau ratatui callers/callees (colonnes caller, fichier, ligne), scrollable, fallback texte plat si non-TTY
- [ ] T059 [US4] Implémenter `src/mcp/server.rs` : boucle JSON-RPC stdio via `rmcp`, handshake MCP, dispatch vers handlers jusqu'à ce que T055 passe
- [ ] T060 [US4] Implémenter `src/mcp/tools.rs` : handlers pour `ecotokens_search`, `ecotokens_outline`, `ecotokens_symbol`, `ecotokens_trace_callers`, `ecotokens_trace_callees`
- [ ] T061 [US4] Implémenter sous-commande `ecotokens mcp` dans `src/main.rs` : démarrage serveur MCP stdio
- [ ] T062t [P] [US4] Tests extension install MCP dans `tests/integration/install_test.rs` :
  `ecotokens install --with-mcp` → entrée MCP présente dans `~/.claude/settings.json`,
  déjà présente → idempotent (pas de doublon), `--with-mcp` refusé par l'utilisateur →
  settings inchangé, autres hooks existants intacts après ajout MCP
  — DOIT ÉCHOUER avant T062
- [ ] T062 [US4] Étendre `ecotokens install` pour proposer (opt-in) l'ajout du serveur MCP dans `~/.claude/settings.json`
- [ ] T063 [P] [US4] Documentation `quickstart.md` : section "Mode MCP" avec configuration Claude Code

**Checkpoint**: US4 complète — `ecotokens trace callers/callees` fonctionnel ; `ecotokens mcp`
répond aux tools MCP depuis Claude Code

---

## Phase 6: Polish & Concerns transversaux

**Purpose**: Qualité, robustesse et packaging final

- [ ] T040 [P] Tests d'intégration end-to-end dans `tests/integration/end_to_end_test.rs` : scénario complet install → filter → gain → uninstall sans erreur
- [ ] T041 [P] Vérification conformité constitution dans `tests/integration/constitution_test.rs` : toutes les sous-commandes supportent `--json`, erreurs sur stderr, codes de retour corrects, exécution pipeline complet via `ecotokens filter` sans connexion réseau → aucun syscall réseau (valide FR-009, vérifiable via mock ou interception d'erreur DNS)
- [ ] T042 Build release statique musl : `cargo build --release --target x86_64-unknown-linux-musl`, vérifier taille binaire < 20 MB
- [ ] T043 [P] Mettre à jour `quickstart.md` avec les commandes réelles vérifiées
- [ ] T044 [P] `cargo clippy -- -D warnings` : zéro warning en mode strict
- [ ] T045 Valider SC-001 : mesurer économies réelles sur `git status`, `git diff`, `cargo test` — documenter baseline dans `research.md`
- [ ] T045b [P] Benchmark latence interception dans `tests/integration/perf_test.rs` :
  intercepter 100 commandes git/cargo sur fixtures → calculer P90 duration_ms,
  vérifier P90 ≤ 50ms — valide SC-003
- [ ] T045c [P] Benchmark rapport `ecotokens gain` : générer 10 000 entrées JSONL
  de test, mesurer temps de `ecotokens gain` → ≤ 3 secondes — valide SC-005
- [ ] T045d [P] Test fallback silencieux dans `tests/integration/fault_test.rs` :
  simuler panique dans le pipeline de filtrage, vérifier que la commande originale
  est transmise à Claude sans erreur visible — valide SC-006
- [ ] T045e Valider SC-002 manuellement : chronométrer `cargo install` + `ecotokens install`
  sur machine fraîche → documenter le résultat dans quickstart.md

---

## Phase 6.5: Daemon file watcher — `ecotokens watch` (P3)

**Goal**: Maintenir l'index symbolique à jour automatiquement sans relancer `ecotokens index`
manuellement. Utilise `notify` pour détecter les modifications de fichiers.

**Independent Test**: Modifier `src/filter/git.rs` pendant que `ecotokens watch` tourne →
`ecotokens outline src/filter/git.rs` reflète les changements en < 2 secondes.

**Note**: Ajouter `notify = "6"` à `Cargo.toml` pour cette phase.

### Tests pour daemon — écrire en premier, vérifier qu'ils ÉCHOUENT

- [ ] T064 [P] Tests unitaires watcher dans `tests/search/watcher_test.rs` : détection création fichier → déclenche indexation, détection modification → re-indexation partielle, fichier ignoré (`.gitignore`) → pas d'indexation — DOIT ÉCHOUER avant T066
- [ ] T065 [P] Tests intégration daemon dans `tests/integration/daemon_test.rs` : `ecotokens watch --path <tmpdir>` démarre, modif fichier → log "re-indexed", SIGTERM → arrêt propre — DOIT ÉCHOUER avant T067

### Implémentation daemon

- [ ] T066 Implémenter `src/daemon/watcher.rs` : surveillance via `notify`, debounce 500ms, appel indexation incrémentale jusqu'à ce que T064 passe
- [ ] T067 Implémenter sous-commande `ecotokens watch` dans `src/main.rs` : options `--path`, `--daemon` (fork + PID file) jusqu'à ce que T065 passe
- [ ] T067at [P] Tests TUI watch dans `tests/tui/watch_test.rs` :
  rendu avec TestBackend → header "ecotokens watch" présent, événements apparaissent
  dans le panel, mode --daemon ou non-TTY → pas de rendu TUI (texte seul)
  — DOIT ÉCHOUER avant T067a
- [ ] T067a Implémenter `src/tui/watch.rs` : panel live ratatui (alternate screen, liste scrollable des événements, compteurs en temps réel), fallback log texte si `--daemon` ou non-TTY

**Checkpoint**: `ecotokens watch --daemon` maintient l'index à jour en arrière-plan

---

## Phase 7 (optionnel): Providers d'embeddings — Ollama / LM Studio

**Goal**: Améliorer la précision de recherche sémantique en ajoutant des embeddings vectoriels
via Ollama ou LM Studio en complément de BM25. Feature flag `embeddings` — non bloquant pour MVP.

**Note**: Nécessite `reqwest` avec feature `embeddings`. Binaire sans ce flag reste < 20 MB.

### Tests pour embeddings — écrire en premier, vérifier qu'ils ÉCHOUENT

- [ ] T068 [P] Tests unitaires Ollama provider dans `tests/search/embed_test.rs` : requête embeddings Ollama → vec<f32> de taille fixe, URL invalide → erreur claire, modèle non disponible → fallback BM25 — DOIT ÉCHOUER avant T070
- [ ] T069 [P] Tests unitaires LM Studio provider dans `tests/search/embed_test.rs` : même couverture qu'Ollama, format API OpenAI-compatible — DOIT ÉCHOUER avant T071

### Implémentation embeddings

- [ ] T070 Implémenter provider Ollama dans `src/search/index.rs` (feature `embeddings`) : appel HTTP `POST /api/embeddings`, vecteur stocké dans index tantivy jusqu'à ce que T068 passe
- [ ] T071 Implémenter provider LM Studio dans `src/search/index.rs` (feature `embeddings`) : appel API OpenAI-compatible `/v1/embeddings` jusqu'à ce que T069 passe
- [ ] T072t [P] Tests CLI embed-provider dans `tests/config/settings_test.rs` :
  `ecotokens config --embed-provider ollama --embed-url http://localhost:11434` → stocké
  dans `~/.config/ecotokens/config.json`, `ecotokens config` sans flag → affiche provider
  et URL courants, provider inconnu → erreur claire sur stderr avec liste des valeurs valides,
  URL malformée → erreur de validation avant écriture
  — DOIT ÉCHOUER avant T072
- [ ] T072 [P] Étendre `ecotokens config --embed-provider <ollama|lmstudio> --embed-url <url>` et documenter dans `quickstart.md`

**Checkpoint**: `ecotokens search "authentification"` retourne des résultats sémantiquement
pertinents même sans correspondance lexicale exacte (avec Ollama actif)

---

## Dépendances & Ordre d'exécution

### Dépendances entre phases

- **Setup (Phase 1)**: Sans dépendances — démarre immédiatement
- **Fondational (Phase 2)**: Dépend de Phase 1 — **BLOQUE toutes les user stories**
- **US1 (Phase 3)**: Dépend de Phase 2 — pas de dépendance sur US2/US3
- **US2 (Phase 4)**: Dépend de Phase 2 + métriques de Phase 3 (store.rs réutilisé)
- **US3 (Phase 5)**: Dépend de Phase 2 uniquement — indépendante de US1/US2
- **US3 étendue (Phase 5 étendue, T046-T052)**: Dépend de Phase 5 (index BM25) — ajoute tree-sitter
- **US4 (Phase 5.5, T053-T063)**: Dépend de Phase 5 étendue (symboles nécessaires pour trace)
- **Polish (Phase 6)**: Dépend des phases US désirées
- **Daemon (Phase 6.5, T064-T067)**: Dépend de Phase 5 étendue — indépendante de US4
- **Embeddings (Phase 7, T068-T072)**: Dépend de Phase 5 (index tantivy) — totalement optionnel

### Dépendances intra-user story (constitution Test-First)

Pour chaque user story, l'ordre **OBLIGATOIRE** est :
1. Écrire les tests → vérifier qu'ils ÉCHOUENT (Red)
2. Implémenter jusqu'à ce qu'ils passent (Green)
3. Refactorer si nécessaire (Refactor)

### Opportunités parallèles

- **Phase 1** : T003, T004, T005, T007, T008 en parallèle après T001+T002
- **Phase 2** : T009/T011/T013/T015 en parallèle (tests) ; T010/T012/T014/T016 en parallèle (implémentations)
- **Phase 3** : T017/T018/T019/T020/T021 en parallèle (écriture tests) ; T022/T023/T024 en parallèle (filtres)
- **Phase 4** : T031/T032 en parallèle (tests) ; T034a peut démarrer dès T031 écrit
- **Phase 5** : T035/T036 en parallèle (tests) ; T037/T038/T039 séquentiels
- **Phase 5 étendue** : T046/T047/T048 en parallèle (tests) ; T049/T050 en parallèle (implémentations)
- **Phase 5.5** : T053/T054/T055 en parallèle (tests) ; T056/T057 en parallèle ; T059/T060 en parallèle
- **Phase 6.5** : T064/T065 en parallèle (tests)
- **Phase 7** : T068/T069 en parallèle (tests) ; T070/T071 en parallèle (implémentations)

---

## Exemple d'exécution parallèle : Phase 3 (US1)

```bash
# Écrire tous les tests US1 en parallèle :
Task: "T017 - Tests hook handler"       → tests/hook/handler_test.rs
Task: "T018 - Tests filtre git"         → tests/filter/git_test.rs
Task: "T019 - Tests filtre cargo"       → tests/filter/cargo_test.rs
Task: "T020 - Tests filtre fs"          → tests/filter/fs_test.rs

# Vérifier que TOUS échouent avant de continuer

# Implémenter les filtres en parallèle :
Task: "T022 - Filtre git"               → src/filter/git.rs
Task: "T023 - Filtre cargo"             → src/filter/cargo.rs
Task: "T024 - Filtre fs"               → src/filter/fs.rs
```

---

## Stratégie d'implémentation

### MVP (User Story 1 uniquement)

1. Compléter Phase 1 : Setup
2. Compléter Phase 2 : Fondational (**CRITIQUE — bloque tout**)
3. Compléter Phase 3 : User Story 1
4. **STOP & VALIDER** : `ecotokens install` → tester dans Claude Code → vérifier ≥ 60% économies
5. Livrable : binaire fonctionnel, hook actif, zéro config requise

### Livraison incrémentale

1. Setup + Fondational → base solide
2. + User Story 1 → MVP : hook + filtres + install ✅
3. + User Story 2 → `ecotokens gain` : visibilité économies ✅
4. + User Story 3 → recherche sémantique ✅
5. + Polish → release stable

---

## Notes

- `[P]` = fichiers différents, pas de dépendances incomplètes
- `[USn]` = traçabilité vers user story spec.md
- Tests MUST fail before implementation (constitution III)
- Commit après chaque tâche ou groupe logique
- `cargo test` doit passer à chaque checkpoint
- **Priorité absolue** : ne jamais marquer une tâche d'implémentation comme terminée si le test correspondant ne passe pas
