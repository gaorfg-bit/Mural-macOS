# Validation locale de Mural

Depuis le projet :

```bash
make sparkle # une seule fois, acquisition du framework officiel epingle
./scripts/valider.sh
```

Exécuter dans une session graphique macOS déverrouillée, avec Cargo et les crates déjà en cache. AppKit et l’encodeur AVIF nécessitent les services natifs macOS : dans Codex, lancer hors sandbox. La validation n’utilise pas le réseau et ne demande pas sudo ; `make sparkle` télécharge la dépendance en amont.

La commande affiche et conserve les sorties de :

- 102 tests Cargo : 81 unitaires, 3 AVIF, 12 conversion, 1 isolation du journal et 5 protection des réglages.
- Bibliothèque et restauration dans un second processus ; menus et conversion AVIF ; panneau, aperçu et fermeture/réouverture.
- Composition Glass et galerie de 500 images, défilement, filtrage et miniatures chargées à la demande.
- Activation moderne et absence simulée du sélecteur macOS 14.
- Changement de dossier pendant une notification AppKit, actualisation imbriquée des favoris, première ouverture du panneau et configuration corrompue conservée après fermeture.
- Moteur Sparkle dans un bundle temporaire, puis archive source, empreinte et présence des tests extraits.

Elle s’arrête au premier échec. `--nocapture` rend visibles les scénarios AVIF non exécutés faute d’encodeur ; `omissions-avif.txt` les recense. Un test Cargo retournant sans exercer la conversion ne certifie pas cette conversion. Une omission due au mauvais emplacement du cache reste un échec.

Chaque exécution crée un dossier `/private/tmp/mural-validation.XXXXXX` unique avec préférences, images synthétiques, caches, journal applicatif, sorties et `code-sortie.txt`. Ces fichiers sont conservés. `MURAL_JOURNAL` est réellement pris en compte ; les traces des recettes ne sont plus ajoutées au journal habituel. Le script force son répertoire Cargo local pour exécuter les binaires qu’il vient de compiler. Le test Sparkle utilise son propre bundle et identifiant temporaires.

La recette bibliothèque démarre brièvement un minuteur à intervalle long puis l’arrête ; elle ne le laisse pas atteindre son échéance. Aucune action `appliquer:` ou `suivanteDiaporama:` n’est invoquée. Le script ne construit, signe ni installe le bundle principal et ne modifie aucun fond.

## Dernière exécution vérifiée

24/10/2026, macOS 27.0.1, Rust 1.98.1 : `/private/tmp/mural-validation.PV4n0a`, code 0. Les 102 tests et toutes les recettes ci-dessus réussissent, sans omission AVIF. La conversion native traite 3/3 images, relit les copies, réutilise le cache chaud et vérifie l’annulation. La galerie charge initialement 30 miniatures sur 500 ; de 26 à 36 pendant les défilements exercés, moins de 100 à chaque contrôle. Deux écrans détectés, aucun fond appliqué.

Les mêmes 102 tests Cargo passent sous Rust 1.88.0 dans une copie neuve nommée `Mural-macOS`, sans `target` initial. Sparkle est acquis séparément depuis l’archive officielle avec SHA-256 épinglé. L’archive source est également testée sous ce nom de dossier et contient les tests d’intégration. Les avertissements Rust préexistants restent visibles.

Les reproductions, limites et commandes du lot sont consignées dans [le rapport d’audit corrigé](validation-audit-2026-10-24.md).

## Ce que cette commande ne certifie pas

- Un lancement sur un vrai macOS 13 : la disponibilité du sélecteur est simulée.
- L’installation complète d’une mise à jour Sparkle, sa publication et la notarisation.
- Le rendu effectif des fonds sur les deux écrans, leur délai et le branchement/débranchement physique.
- Le survol sans clic, la fluidité pendant tout un glissement, VoiceOver ou tous les encodeurs macOS.
- Un profil Instruments complet de CPU, réveils et mémoire ; la recette de 500 images contrôle le chargement et le défilement, pas tous les usages.
