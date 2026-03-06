# Feature Specification: Outil compagnon d'économie de tokens pour Claude Code

**Feature Branch**: `001-token-companion`
**Created**: 2026-02-21
**Status**: Draft
**Input**: User description: "Outil compagnon pour claude code qui économise les tokens consommés, simple, transparent, set it and forget it."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Installation transparente et activation automatique (Priority: P1)

Un développeur installe l'outil une seule fois. À partir de ce moment, chaque commande
système invoquée dans le contexte de Claude Code est automatiquement interceptée et son
output est filtré/compressé avant d'être transmis à Claude. Le développeur n'a rien à
changer dans ses habitudes de travail.

**Why this priority**: C'est le cœur du produit — sans transparence d'usage, la promesse
"set it and forget it" ne tient pas. Toute autre fonctionnalité en dépend.

**Independent Test**: Peut être testé entièrement en installant l'outil, lançant
`git status` dans une session Claude Code, et vérifiant que l'output transmis à Claude
est réduit par rapport à l'output brut — sans aucune action supplémentaire de l'utilisateur.

**Acceptance Scenarios**:

1. **Given** l'outil est installé et la session Claude Code est active,
   **When** l'utilisateur exécute `git status`,
   **Then** l'output reçu par Claude est filtré (tokens réduits) sans intervention manuelle.

2. **Given** l'outil est installé,
   **When** l'utilisateur exécute une commande dont l'output ne peut pas être réduit,
   **Then** l'output original est transmis sans altération ni erreur.

3. **Given** l'outil n'est pas installé,
   **When** l'utilisateur exécute une commande,
   **Then** le comportement est identique au comportement natif de Claude Code (aucune régression).

---

### User Story 2 - Visualisation des économies réalisées (Priority: P2)

Le développeur peut, à tout moment, consulter le bilan des tokens économisés depuis
l'installation : nombre total de tokens économisés, économies par type de commande,
et tendance dans le temps.

**Why this priority**: La valeur de l'outil doit être visible et mesurable pour justifier
son adoption et maintenir la confiance.

**Independent Test**: Peut être testé en exécutant plusieurs commandes interceptées,
puis en consultant le rapport d'économies et en vérifiant que les chiffres sont cohérents
avec les outputs observés.

**Acceptance Scenarios**:

1. **Given** l'outil a intercepté au moins une commande,
   **When** l'utilisateur demande un rapport d'économies,
   **Then** le rapport affiche le nombre total de tokens économisés et le pourcentage d'économie global.

2. **Given** l'outil fonctionne depuis plusieurs jours,
   **When** l'utilisateur consulte l'historique,
   **Then** les économies sont présentées par commande et par période.

---

### User Story 3 - Recherche sémantique dans le codebase (Priority: P3)

Au lieu de transmettre des fichiers entiers à Claude, le développeur peut demander à
l'outil de fournir uniquement les extraits de code pertinents à la requête en cours,
via une recherche sémantique dans le codebase.

**Why this priority**: Fonctionnalité à haute valeur ajoutée pour les grands projets,
mais dépend d'une infrastructure d'indexation ; peut être ajoutée après le cœur.

**Independent Test**: Peut être testé en demandant "les fonctions qui gèrent l'authentification"
et en vérifiant que seuls les extraits pertinents sont retournés plutôt que les fichiers complets.

**Acceptance Scenarios**:

1. **Given** le codebase est indexé,
   **When** l'utilisateur pose une question ciblée à Claude,
   **Then** l'outil injecte uniquement les extraits pertinents et non les fichiers entiers.

2. **Given** le codebase n'est pas encore indexé,
   **When** la fonctionnalité de recherche sémantique est sollicitée,
   **Then** l'outil propose de lancer l'indexation et continue normalement sans bloquer.

---

### User Story 4 - Navigation structurelle : outline, symboles et graphe d'appels (Priority: P2 — implémentée après US3 par dépendance technique sur l'index symbolique)

Le développeur peut demander à Claude de naviguer dans le code à un niveau structurel
(liste des symboles d'un fichier, qui appelle cette fonction) sans avoir à lire des
dizaines de fichiers entiers. L'outil répond en quelques millisecondes depuis l'index
local.

**Why this priority**: Réduit massivement les tokens consommés lors de l'exploration
architecturale d'un codebase. Un `ecotokens outline src/` économise la lecture de tous
les fichiers sources. La navigation par appels (`trace callers`) évite d'ouvrir manuellement
chaque fichier pour trouver les usages.

**Independent Test**: Après `ecotokens index`, `ecotokens outline src/filter/git.rs`
retourne la liste des fonctions publiques du fichier sans lire le fichier lui-même.
`ecotokens trace callers filter_status` retourne les callsites en < 200ms.

**Acceptance Scenarios**:

1. **Given** le codebase est indexé,
   **When** l'utilisateur exécute `ecotokens outline src/filter/git.rs`,
   **Then** l'outil retourne la liste des symboles (nom, kind, ligne) sans transmettre le source complet.

2. **Given** le codebase est indexé,
   **When** l'utilisateur exécute `ecotokens trace callers filter_status`,
   **Then** l'outil retourne tous les sites d'appel avec fichier et numéro de ligne, en < 200ms.

3. **Given** le codebase est indexé,
   **When** l'utilisateur exécute `ecotokens trace callees filter_status`,
   **Then** l'outil retourne les fonctions appelées par `filter_status` avec leurs IDs stables.

4. **Given** Claude Code est configuré avec le serveur MCP ecotokens,
   **When** Claude appelle le tool `ecotokens_outline` via MCP,
   **Then** la réponse est identique à celle de la CLI sans passer par bash.

---

### Edge Cases

- Que se passe-t-il si l'output filtré est plus long que l'original ? L'original est transmis.
- Que se passe-t-il si l'outil plante pendant l'interception ? Fallback transparent vers l'output brut, aucune perte de session.
- Que se passe-t-il si la commande produit des données binaires ? Les données sont transmises sans transformation.
- Que se passe-t-il si deux sessions Claude Code tournent en parallèle ? Chaque session est isolée ; les métriques sont agrégées globalement (l'outil est machine-wide).
- Comment l'outil gère-t-il les commandes produisant un très grand volume (build complet, test suite) ? Au-delà de 500 lignes ou ~50 Ko, l'output est résumé automatiquement avant transmission à Claude.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: L'outil DOIT intercepter automatiquement les outputs de commandes dans le contexte de Claude Code sans configuration utilisateur requise après l'installation initiale.
- **FR-012**: L'utilisateur DOIT pouvoir exclure des commandes spécifiques de l'interception via une liste d'exclusion définie dans un fichier de configuration simple (sans redémarrage requis).
- **FR-002**: L'outil DOIT réduire le nombre de tokens transmis à Claude pour les commandes à fort volume (git, build, tests, logs) sans altérer la pertinence de l'information.
- **FR-003**: L'outil DOIT fonctionner en mode passthrough (sans filtrage) lorsqu'aucune réduction n'est possible ou bénéfique.
- **FR-004**: L'outil DOIT enregistrer, pour chaque interception, le nombre de tokens avant et après filtrage.
- **FR-005**: L'outil DOIT exposer un rapport d'économies consultable à la demande (nombre total, par commande, par période).
- **FR-006**: L'outil DOIT s'installer en une seule commande et s'activer automatiquement dans Claude Code sans redémarrage de session requis.
- **FR-007**: L'outil DOIT permettre d'indexer un codebase localement pour fournir des extraits sémantiquement pertinents en lieu et place de fichiers entiers.
- **FR-008**: L'outil NE DOIT PAS introduire de latence perceptible sur les commandes interceptées pour les outputs inférieurs à 500 lignes / 50 Ko. Au-delà de ce seuil, l'output est résumé automatiquement avant transmission.
- **FR-009**: L'outil DOIT fonctionner sans connexion réseau (traitement 100% local).
- **FR-011**: L'outil DOIT détecter et masquer automatiquement les données sensibles connues (clés API, tokens Bearer, variables d'environnement de type secret) dans les outputs interceptés avant transmission à Claude.
- **FR-010**: L'outil DOIT supporter les formats de sortie JSON et lisible humain pour toutes ses commandes propres.
- **FR-013**: L'outil DOIT exposer un mode debug (flag `--debug`) affichant, pour chaque interception, l'output brut, l'output filtré, et le delta de tokens — sans impact sur le comportement nominal.
- **FR-014**: L'outil DOIT pouvoir démarrer en mode serveur MCP (stdio) via `ecotokens mcp`, exposant les tools `ecotokens_search`, `ecotokens_outline`, `ecotokens_trace_callers` et `ecotokens_trace_callees` directement à Claude Code sans passer par le hook bash.
- **FR-015**: L'outil DOIT pouvoir maintenir l'index symbolique à jour automatiquement en surveillant les modifications de fichiers du codebase (`ecotokens watch`), avec un délai de propagation inférieur à 2 secondes et un arrêt propre sur signal SIGTERM. Cette fonctionnalité est optionnelle (P3) et ne doit pas bloquer l'installation ni les autres commandes.

### Key Entities

- **Interception** : Représente une commande interceptée avec son output brut, son output filtré, le nombre de tokens avant/après, et l'horodatage.
- **Profil de filtre** : Ensemble de règles de transformation associées à un type de commande (ex. : `git status`, `cargo test`, `ls`).
- **Session** : Contexte d'une conversation Claude Code active, regroupant les interceptions associées.
- **Index codebase** : Représentation locale du code source permettant la recherche sémantique par pertinence.
- **Symbol** : Unité atomique d'un symbole de code (fonction, struct, impl…) avec ID stable, extrait via tree-sitter.
- **CallEdge** : Relation d'appel entre deux symboles, construite lors de l'indexation AST pour le graphe d'appels.
- **Rapport d'économies** (`ecotokens gain`) : Agrégation des métriques d'interception sur une période donnée, incluant l'équivalent USD économisé.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Les tokens transmis à Claude pour les commandes courantes (`git status`, `git diff`, `cargo test`, `ls -la`) sont réduits d'au moins 60% en moyenne par rapport à l'output brut.
- **SC-002**: Le temps d'installation et de configuration initiale est inférieur à 2 minutes.
- **SC-003**: 90% des commandes interceptées n'introduisent aucune latence perceptible pour l'utilisateur.
- **SC-004**: L'outil fonctionne sans aucune action de l'utilisateur après l'installation initiale (zéro commande supplémentaire à mémoriser pour le flux nominal).
- **SC-005**: Le rapport d'économies est accessible en moins de 3 secondes, quel que soit le volume d'historique.
- **SC-006**: Aucune session Claude Code n'est interrompue ou dégradée par une défaillance de l'outil (fallback silencieux systématique en cas d'erreur interne).

## Clarifications

### Session 2026-02-21

- Q: L'outil est-il global (machine-wide) ou scoped au projet courant ? → A: Global — actif dans tous les projets de la machine, métriques agrégées cross-projets.
- Q: L'outil doit-il détecter et masquer les données sensibles dans les outputs interceptés ? → A: Masquage automatique des patterns connus (clés API, tokens Bearer, secrets `.env`).
- Q: Quel est le seuil de taille d'output au-delà duquel le mode résumé s'active ? → A: 500 lignes / ~50 Ko.
- Q: L'utilisateur peut-il désactiver l'interception pour certaines commandes ? → A: Oui — liste d'exclusion configurable dans un fichier de config simple.
- Q: L'outil doit-il exposer un mode verbose/debug ? → A: Oui — flag `--debug` activable à la demande, affiche input/output/tokens pour chaque interception.

## Assumptions

- L'intégration avec Claude Code se fait via le mécanisme de hooks natif (hook `PreToolUse`), sans modification du binaire Claude Code.
- Le comptage des tokens est une approximation locale, suffisamment précise pour les métriques d'économies (pas d'appel réseau pour comptage).
- L'indexation sémantique du codebase est optionnelle et activée manuellement lors de la première utilisation de la fonctionnalité P3.
- Les filtres par type de commande sont pré-définis pour les commandes les plus courantes et extensibles par l'utilisateur via un fichier de configuration simple.
- Le stockage des métriques est entièrement local (aucune télémétrie externe).
- L'outil est machine-wide : une installation unique couvre tous les projets de la machine ; les métriques sont agrégées cross-projets mais consultables par répertoire racine Git.
