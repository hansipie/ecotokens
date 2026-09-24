# Audit de pertinence des tests

Audit initial du 24 septembre 2026. Le détail de **chaque fonction de test** se trouve dans [test-audit.csv](test-audit.csv) : chemin, ligne initiale et actuelle, nom, statut d'exécution, avis initial, état de correction, motif et première assertion observée avant correction. L'inventaire initial couvrait 770 définitions dans 100 fichiers Rust (96 sous `tests/`, quatre sous `src/`). Quatre tests de complétions shell ajoutés ensuite portent le CSV actuel à 774 définitions. `cargo test -- --list` affichait 791 entrées lors du premier relevé, car certains tests unitaires sont compilés dans plusieurs cibles.

## Résultat

| Avis | Nombre | Interprétation |
| --- | ---: | --- |
| Pertinent | 711 | Vérifie un comportement précis, une erreur attendue ou une propriété directement liée au nom du test. |
| À renforcer | 41 | Couvre un chemin utile, mais l'assertion laisse passer une régression plausible ou le nom est imprécis. |
| Trompeur | 11 | Peut réussir alors que le comportement annoncé est absent, ou exerce un autre chemin. |
| Manuel | 7 | Dépend d'un service, d'un modèle ou d'un environnement externe et est ignoré par défaut. |

Neuf tests au total portent `#[ignore]` ; deux d'entre eux ont reçu un avis « À renforcer » ou « Trompeur » à cause de leur assertion. Une exécution complète hors sandbox a donné **782 réussites, 0 échec, 9 ignorés**. Le premier essai dans le sandbox s'est arrêté sur les tests de réécriture qui ouvrent un port local (`PermissionDenied`).

Cet avis repose sur la lecture des noms, des assertions et des chemins de test, avec une inspection approfondie des cas suspects. Ce n'est pas une mesure de couverture ni un test de mutation. Un avis « Pertinent » signifie que le test a une raison d'être et une assertion liée à cette raison ; il ne garantit pas que toutes les branches de la fonction sont couvertes.

## Corrections prioritaires

Les 11 tests jugés « Trompeur » ci-dessous ont été corrigés. Les avis du tableau et du CSV restent ceux de l'audit initial afin de conserver la raison de chaque modification. Le test de réutilisation des embeddings emploie maintenant un faux fournisseur local qui compte les requêtes et s'exécute par défaut ; il ne requiert plus de modèle externe.

| Test | Problème vérifié dans le code | Amélioration utile |
| --- | --- | --- |
| `filter_with_unreadable_command_exits_cleanly` | `!stderr.contains("thread") || !stderr.contains("panicked")` peut réussir malgré une panique. | Vérifier le code de sortie et utiliser `&&` ou une assertion explicite sur l'absence de panique. |
| `filter_passes_through_original_content_on_error` | Lance `cat` sur un fichier lisible : le chemin d'erreur n'est jamais déclenché. | Provoquer une erreur contrôlée et comparer la sortie au contenu de départ. |
| `read_outline_empty_returns_passthrough` | Accepte explicitement tout résultat et n'a aucune assertion. | Construire un cas sans symbole et exiger `Passthrough`. |
| `test_watch_sends_reindexed_event_on_file_change`, `test_watch_debounces_rapid_changes`, `test_watcher_detects_file_creation`, `test_watcher_detects_modification` | Acceptent `status.starts_with("error")` comme succès. | Exiger `re-indexed` et vérifier le nombre d'événements pour le test de regroupement. |
| `incremental_reindex_reuses_embeddings` | Ne vérifie que la taille positive de deux fichiers HNSW. | Observer le nombre d'appels au fournisseur ou les identifiants réutilisés. |
| `gain_report_with_large_store_is_fast` | Mesure seulement `read_from`, alors que le nom annonce la génération du rapport. | Inclure `aggregate` et, si voulu, le rendu dans la mesure. |
| `gain_sparkline_present_adaptive` | Cherche seulement le titre `Savings`. | Vérifier des cellules du tracé ou une propriété du rendu liée aux données. |
| `test_threshold_filters_low_similarity` | Accepte des mots génériques dans stdout au lieu de vérifier les groupes. | Analyser le JSON et comparer les groupes attendus pour deux seuils. |

## Observations par famille

- Les tests de masquage, de découpage et de reconstruction, de migration du stockage, ainsi que les scénarios de repli de réécriture ont des assertions généralement précises et portent sur des pertes de données ou des secrets : leur nombre est justifié.
- Les tests de passage direct des filtres vérifient parfois une seule sous-chaîne ; une comparaison de la sortie entière protégerait mieux la conservation des données.
- Plusieurs tests de TUI ne contrôlent qu'un titre ou l'absence de panique. C'est utile comme test de stabilité, mais insuffisant pour leur nom quand celui-ci annonce un contenu précis.
- Les 33 cas de détection de famille couvrent des commandes et enveloppes différentes. Les regrouper en table réduirait le code, pas le nombre de cas à garder.
- Les 9 tests ignorés couvrent des modèles ou services externes et une mesure de latence. Ils ne contribuent pas à la protection de la suite exécutée par défaut.

## Vérification

- Inventaire par analyse syntaxique Rust : 770 fonctions portant `#[test]` ou `#[tokio::test]`.
- `cargo test -- --list` : 791 entrées exécutables/ignorées.
- `cargo test --quiet` hors sandbox : 782 réussites, 0 échec, 9 ignorés.

Après les corrections prioritaires, `cargo test -- --list` recense 795 entrées dans l'état actuel du dépôt. La suite complète hors sandbox donne **787 réussites, 0 échec, 8 ignorés**. Le test de réutilisation des embeddings fait désormais partie des tests exécutés par défaut. Quatre tests d'installation des complétions apparus entre les deux relevés expliquent la hausse de 791 à 795 entrées.
