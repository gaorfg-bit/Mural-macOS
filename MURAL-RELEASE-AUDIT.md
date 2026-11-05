# MURAL — Audit de release 0.1.3

Date : 5 novembre 2026 · Branche : `stabilisation-0.1.3` · Point de restauration : tag `pre-audit-0.1.3` → `14fab15`

> **Avertissement de méthode.** Cet audit est **statique** : lecture intégrale des modules applicatifs
> (`src/`, `src/app/`), des scripts, de `Info.plist`, `Cargo.toml` et de la documentation.
> **Rien n'a été compilé, exécuté ni mesuré** pendant cette session : la VM Linux n'a ni Rust ni Xcode,
> la cible Rust macOS n'était pas téléchargeable, et le Terminal du Mac n'accepte pas la saisie
> par contrôle d'ordinateur. Chaque constat porte la mention **[vérifié dans le code]**,
> **[documenté]** (affirmé par la doc du projet, non revérifié) ou **[hypothèse]**.

---

## 1. Résumé exécutif

Le cœur technique est sérieux : écritures atomiques, protection d'un réglage corrompu, suivi
des emprunts contre la réentrance AppKit, décodage hors fil principal, minuteries strictement
bornées, Sparkle épinglé par SHA-256, recettes isolées par variables d'environnement.

Les risques de lancement ne sont **pas** dans le code métier ; ils sont dans :

1. **La première impression** : aucune commande visible au repos, aucun état vide. Un utilisateur
   venu de Reddit dont `~/Pictures` ne contient pas d'images au premier niveau voit une fenêtre
   vide sans bouton. **Premier facteur d'abandon.**
2. **La distribution** : bêta non notarisée, signature locale « SnapLayout Dev », cask Homebrew
   dans un tap tiers alors que Homebrew 5 retire les casks non notarisés du tap officiel
   (échéance septembre 2026). Parcours d'installation propre **non vérifié**.
3. **La confiance** : la notice de confidentialité décrivait un flux de mises à jour inexact
   (corrigé, voir §7).

**Décision : NO-GO en l'état, GO WITH RISKS dès que les 4 vérifications bloquantes du §9 sont faites.**

## 2. Architecture observée [vérifié dans le code]

| Couche | Modules | Commentaire |
| --- | --- | --- |
| Entrée | `main.rs`, `app/construction.rs` | Hook de panique vers le journal, `MainThreadMarker` exigé |
| Présentation | `app.rs` (classe `Cible`, ~70 actions ObjC), `app/{grille,menus,apercu,diaporama}.rs`, `interface.rs`, `reglages_panneau.rs`, `avif_panneau.rs` | AppKit pur, mise en page en coordonnées absolues + autoresizing |
| État | `app/etat.rs` (`Etat` + `EtatRef`/`EtatMut`) | Pointeur brut vers un `Box` fuité, emprunts comptés : chevauchement → journal (release) / panic (debug) |
| Métier pur | `diaporama.rs`, `suivi.rs`, `suppression.rs`, `preferences.rs` | Testable sans AppKit, bien isolé |
| Système | `fond_ecran.rs` (NSWorkspace), `ecrans.rs` (NSScreen + UUID ColorSync), `tous_bureaux.rs` (plist privé + `killall WallpaperAgent`), `mises_a_jour.rs` (Sparkle via NSBundle) | |
| Ressources | `bibliotheque.rs`, `imageio.rs`, `cache.rs`, `avif.rs` | ImageIO à l'échelle, cache PNG + `.ident` |

Concurrence : un worker de vignettes par zone visible (`sync_channel(2)`, annulation par `AtomicBool`),
un thread par aperçu, un worker AVIF, une purge de cache au démarrage. Les threads ne touchent
jamais AppKit et ne notifient que via `performSelectorOnMainThread`. **Pas de data race identifiée.**

Choix discutables mais **à ne pas réécrire avant samedi** :
- `Cible` est un objet-dieu (≈ 1 400 lignes + modules) ; maintenabilité moyenne, risque nul à court terme.
- Pointeurs bruts vers les vues (`Cell<*mut NSView>`) : sûrs tant que la fenêtre n'est jamais libérée
  (`setReleasedWhenClosed(false)` + `mem::forget`) — c'est le cas.
- Mise en page en coordonnées fixes : fragile aux tailles de police/accessibilité, pas aux écrans.

## 3. Audit du code

| ID | Gravité | Constat | Preuve | Statut |
| --- | --- | --- | --- | --- |
| C1 | P2 | Fichiers cachés et AppleDouble (`._x.jpg`) listés comme images → vignettes « (unreadable) » sur disques externes | `bibliotheque::lister_sauf` | **Corrigé** (non compilé) |
| C2 | P2 | `svg` accepté (`EXTENSIONS_ACCEPTEES`) mais absent du README ; décodage ImageIO et fond NSWorkspace SVG **non vérifiés** | `fond_ecran.rs:20` | Ouvert — [hypothèse] |
| C3 | P2 | Réglages illisibles au démarrage → l'app refuse ensuite toute sauvegarde (protection voulue), sans bouton de réinitialisation ; seul le message d'état l'indique | `preferences::sauvegarder_vers`, `Etat::nouveau` | Ouvert |
| C4 | P2 | `plan_diaporama()` fait un `stat` par favori, appelé 2–3 fois à chaque `rafraichir()` (chaque clic) | `etat.rs:172`, `grille.rs:228`, `rafraichir_panneau` | Ouvert — coût réel [hypothèse] |
| C5 | P2 | Aperçu plein écran décodé **sur le fil principal** jusqu'à 5120 px | `app.rs` `apercu_plein_ecran` | Ouvert |
| C6 | P2 | « Delete from disk… » = `remove_file` définitif, sans Corbeille (décision du 03/11) | `suppression.rs:23` | **Soumis à validation** |
| C7 | P2 | `tous_bureaux` : lecture/écriture du plist privé sans protection contre une écriture simultanée de `WallpaperAgent` ; expérimental, désactivé par défaut | `tous_bureaux::propager` | Accepté (risque documenté) |
| C8 | P3 | Sauvegarde des réglages sans `fsync` avant `rename` : coupure de courant → fichier possiblement vide → C3 | `preferences.rs:469` | Ouvert |
| C9 | P3 | `journal` : ouverture/fermeture du fichier + `NSDateFormatter` neuf à chaque ligne ; journal verbeux (chaque sélection, recherche) | `journal.rs` | Ouvert |
| C10 | P3 | `apercu_plein_ecran` déréférence `fenetre` sans test de nullité (toujours non nul après construction) | `app.rs:714` | Accepté |

Aucun `unwrap` dangereux hors tests ; les `expect` restants portent sur `MainThreadMarker` dans des actions AppKit (invariant vrai).

## 4. Performance [non mesuré]

**Aucune mesure n'a été prise dans cette session.** La doc du projet rapporte (24/10, [documenté]) :
30 vignettes chargées sur 500 au démarrage, 26–36 pendant le défilement, décodage ImageIO
8176×4096 en ~258 ms. Points de vigilance à profiler avec Instruments (Time Profiler + Allocations, 30 min d'usage) :
- reconstruction complète de la grille (≤ 500 boutons × 2 badges) à chaque recherche, filtre ou redimensionnement (débounce 80 ms) ;
- C4 et C5 ci-dessus ;
- consommation au repos : aucune minuterie hors diaporama/relecture [vérifié dans le code] — favorable.

## 5. Revue UX/UI

| ID | Gravité | Constat |
| --- | --- | --- |
| U1 | **P1** | Barre d'outils, panneau dossiers **et boutons de fenêtre** masqués au repos, révélés à 14 px du bord haut / 8 px du bord gauche (`interface.rs`). Découvrabilité quasi nulle pour un nouvel utilisateur. |
| U2 | **P1** | Aucun état vide : dossier sans image → aperçu et grille vides, statut dans un panneau masqué. |
| U3 | P2 | Pas de navigation clavier dans la grille (flèches, Entrée pour appliquer). |
| U4 | P2 | « AVIF cache… » visible dans deux menus : fonction obscure pour un nouvel utilisateur, les copies ne servent pas aux fonds. |
| U5 | P2 | Le diaporama ne reprend pas après relance et aucun lancement à l'ouverture de session : beaucoup d'utilisateurs s'attendront à l'inverse. |
| U6 | P3 | Libellés « Display 1/2 » sans nom d'écran (le nom n'est qu'en infobulle). |

La direction artistique (aperçu bord à bord, verre flottant) est réussie quand la galerie est remplie ;
U1/U2 ne demandent pas de la changer. **Proposition (à valider)** : révéler les commandes tant que
la galerie est vide ou à la première ouverture, et afficher au centre de la grille
« No images in this folder — Open Folder… (⌘O) ».

## 6. Sécurité, confidentialité, distribution

- Pas d'App Sandbox, pas d'entitlements, pas de permission système demandée [vérifié : `Info.plist`] — cohérent pour une distribution hors App Store.
- Aucun secret embarqué ; seule la clé publique Ed25519 est dans `Info.plist` [vérifié].
- Mises à jour : vérification automatique quotidienne via un Worker Cloudflare qui compte jour/version/macOS [documenté dans `docs/MISES-A-JOUR.md`]. **Le code du Worker n'a pas été relu** (hors du dossier partagé) : vérifie qu'il ne conserve effectivement ni IP ni identifiant, comme l'affirme désormais la notice.
- Notarisation : non faite (README). `scripts/distribuer-beta.sh` est prêt mais exige un certificat Developer ID.
- `LSMinimumSystemVersion` 26.0, mais la dernière validation documentée est sur macOS 27.0.1 : **macOS 26 jamais testé** [hypothèse de risque].

## 7. Corrections effectuées

| Commit | Contenu | Vérification |
| --- | --- | --- |
| `8234657` | `PRIVACY.md` : flux réel (Worker Cloudflare → GitHub), compteurs agrégés, portée 0.1.3 | Relecture du texte contre `Info.plist` et `MISES-A-JOUR.md` |
| `db5bf65` | `bibliotheque.rs` : ignore les fichiers commençant par `.` ; test `lister_ignore_les_fichiers_caches_et_appledouble` | **Non compilé** — à valider |

Aucune modification visuelle ni de comportement utilisateur n'a été faite sans accord.

## 8. Tests exécutés

**Aucun pendant cette session.** Dernière exécution documentée : `scripts/valider.sh`, 24/10/2026, 102 tests + recettes, code 0 [documenté, non revérifié].

## 9. Vérifications manuelles nécessaires

**Bloquantes avant publication**
1. `./scripts/valider.sh` sur la branche `stabilisation-0.1.3` → code 0 (valide aussi `db5bf65`).
2. Installation propre : nouveau compte macOS, `brew install --cask gaorfg-bit/mural/mural-macos`, premier lancement via Réglages → Confidentialité → « Ouvrir quand même ». Noter chaque friction.
3. Premier lancement avec un `~/Pictures` sans image au premier niveau : constater U1/U2 et trancher.
4. Mise à jour Sparkle réelle de la version publiée précédente vers celle de samedi (téléchargement, remplacement, relance, réglages conservés).

**Recommandées**
5. Dossier contenant un `.svg` (C2) : vignette, aperçu, application du fond.
6. Débrancher/rebrancher un écran pendant un diaporama ; sortie de veille pendant un diaporama.
7. Instruments 30 min : mémoire stable, CPU au repos ≈ 0.
8. Réglages corrompus volontairement (`echo x > ~/Library/Application\ Support/Mural/reglages.json`) : message compréhensible ?
9. VoiceOver sur la grille et la barre d'outils.

## 10. Risques résiduels

- Gatekeeper / Homebrew : un utilisateur sur deux peut abandonner à l'installation d'une app non notarisée.
- Découvrabilité (U1/U2) si non traitée.
- Suppression définitive sans Corbeille (C6) : un clic mal compris = perte de fichier, avec confirmation.
- Code non compilé dans `db5bf65` tant que l'étape 9.1 n'est pas faite.
- macOS 26 non testé.

## 11. Checklist de distribution

- [ ] `valider.sh` vert sur la branche
- [ ] `CFBundleVersion` incrémenté (actuellement 4) et cohérent avec `Cargo.toml`
- [ ] `make bundle` puis `codesign --verify --deep --strict` OK
- [ ] ZIP + `appcast.xml` via `preparer-mise-a-jour.sh`, signature `sign_update --verify` OK
- [ ] Release GitHub contenant `appcast.xml`, ZIP, archive source GPL
- [ ] Cask mis à jour, SHA-256 concordant, installation propre testée
- [ ] README : mentionner clairement « non notarisé » et la procédure « Ouvrir quand même » (déjà présent)
- [ ] Post Reddit : capture avec galerie remplie, expliquer comment révéler les commandes (si U1 conservé)

## 12. Décision

**NO-GO en l'état** — non parce que le code est mauvais, mais parce que rien n'a été validé
dynamiquement depuis le 24/10 et que le parcours d'installation et de premier lancement n'a pas été
éprouvé sur une machine propre.

**GO WITH RISKS** dès que les quatre vérifications bloquantes du §9 passent, en acceptant C6 et
la non-notarisation, et à condition de trancher U1/U2 (correctif recommandé, faible risque, ~1 h).
