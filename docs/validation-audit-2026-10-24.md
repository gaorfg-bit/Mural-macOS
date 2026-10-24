# Correctifs de l’audit Rust/AppKit — 24 octobre 2026

Base du lot : `424ae89` (activation macOS 13, commit local). Ce lot corrige les constats ci-dessous et est testé sur macOS 27.0.1. Aucun push ni publication de release.

## Sûreté des ivars et des accès à l’état

Les pointeurs branchés après construction sont dans des `Cell` ; aucune écriture ne passe désormais par un cast de `&CibleIvars` en pointeur mutable. Leur propriété et leur durée de vie sont conservées : l’état et la cible vivent jusqu’à la fin du processus. Ce lot n’ajoute pas de destruction ni de gestion multi-fenêtres.

La galerie, les menus, les contrôles et le panneau consomment des instantanés possédés. Les lectures et mutations du modèle se terminent avant les appels imbriqués et les notifications AppKit synchrones. Les boutons utilisés après une notification sont retenus. Une génération interrompt les reconstructions de grille devenues obsolètes ; le panneau rejoue la dernière actualisation demandée pendant une notification imbriquée.

Une revue indépendante a identifié deux défauts de panneau pendant l’implémentation. Ils ont été reproduits puis corrigés : publication du panneau avant sa première actualisation, et sérialisation des actualisations imbriquées. La seconde revue statique ne relève aucun autre défaut concret dans les chemins examinés. Cette revue et les recettes ne constituent pas une preuve formelle de l’absence de comportement indéfini dans tout le programme.

## Régressions reproduites avant correction

- Menus et panneau : assertions `13 == 14`, devenues fausses depuis le retrait de Flat. Les commandes Settings et AVIF sont désormais retrouvées par leur action, et leur cible réelle est vérifiée.
- Grille : ouvrir un dossier vide pendant `NSMenuDidAddItemNotification` provoquait un `index out of bounds` dans la reconstruction. La recette vérifie désormais aussi qu’aucune ancienne cellule n’est republiée.
- Première ouverture de Settings : une notification changeait le dossier alors que le panneau affichait encore l’ancien.
- Favoris de Settings : sans la garde de réentrance, une reconstruction imbriquée dupliquait les deuxième et troisième favoris. Contre-épreuve exécutée avec la garde retirée temporairement, puis restaurée avant la validation finale.
- Réglages : 3 tests échouaient, car la sauvegarde remplaçait un JSON corrompu, des octets non UTF-8 ou un JSON sans objet par les valeurs par défaut. La lecture stricte distingue absence et erreur ; la sauvegarde refuse de remplacer un fichier inutilisable, y compris devenu corrompu après le démarrage. L’erreur est conservée dans l’état et présentée dans les informations de la fenêtre. L’écriture temporaire suivie du renommage reste utilisée pour un fichier absent ou valide.
- Journal : `MURAL_JOURNAL` était ignoré et le fichier temporaire du test restait absent. Le test utilise un processus enfant pour éviter de modifier l’environnement d’autres tests.
- Archive : le test échouait dans `Mural-macOS`, car il imposait le préfixe `Mural/`. Il suit désormais le nom réel, vérifie l’empreinte et extrait l’archive pour retrouver les cibles des tests avec Cargo. `tests/` est inclus.

## Vérification finale

`./scripts/valider.sh` : code 0, preuves `/private/tmp/mural-validation.PV4n0a`. 102 tests Cargo, menus/panneau, galerie de 500 images, deux branches d’activation, quatre scénarios de réentrance/protection, moteur Sparkle temporaire et archive source. Aucun scénario AVIF omis, aucun fond appliqué. Le journal applicatif et tous les réglages des recettes sont temporaires.

Rust 1.88.0 a été installé uniquement sous `/private/tmp/mural-audit-suite/msrv/rustup`, sans changer l’outil par défaut. Les 102 tests passent dans `/private/tmp/mural-audit-suite/source-neuve/Mural-macOS`, initialement sans `vendor` ni `target`. `make sparkle` télécharge et vérifie le framework officiel. Logs des tests et de construction conservés dans `/private/tmp/mural-audit-suite/`.

`make bundle` réussit dans les deux environnements : Rust 1.88.0 avec `SIGN=-` dans la copie neuve, et Rust 1.98.1 avec l’identité locale « SnapLayout Dev » dans le projet principal. Les deux signatures sont vérifiées. Le bundle principal est relancé ; galerie et ouverture/fermeture de Settings constatées via l’arbre d’accessibilité. Aucun réglage produit n’est changé pendant ce contrôle.

La validation macOS 13 réelle, l’installation complète d’une mise à jour, la notarisation et les effets physiques sur les fonds restent ouverts. La réutilisation de l’aperçu inchangé et le profil Instruments restent des tâches distinctes de la roadmap.
