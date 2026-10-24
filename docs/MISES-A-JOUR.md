# Mises à jour Mural — Sparkle 2

## Comportement

Mural 0.1.1 (build 2) embarque Sparkle 2.10.0. Vérification quotidienne en HTTPS,
commande **Mural → Check for Updates…**, installation proposée à l'utilisateur.
Pas d'installation silencieuse par défaut, pas de profilage système.
Les choix ultérieurs de l'utilisateur sont conservés par Sparkle, sans écrasement
au lancement. Les images et les préférences JSON ne sont pas remplacées.

Flux public stable :
`https://github.com/gaorfg-bit/homebrew-mural/releases/latest/download/appcast.xml`

Le dépôt source peut rester privé : seuls le flux et les archives distribuées
doivent être accessibles sans authentification. Ne jamais embarquer un jeton GitHub.
Chaque release stable du tap doit contenir `appcast.xml`, même si elle sert aussi
Homebrew. Sinon `/latest/download/appcast.xml` renverra une erreur.

La version 0.1.0 déjà distribuée doit être remplacée manuellement une fois :
elle ne contient pas Sparkle et ne peut pas s'ajouter elle-même cette fonction.

## Construction

```sh
make sparkle                       # acquisition explicite, HTTPS + SHA-256 épinglé
make bundle                        # Rust hors ligne, copie + signature des composants
./scripts/test-mises-a-jour.sh      # bundle temporaire, aucun réseau ni remplacement
```

Le framework est chargé par NSBundle depuis `Contents/Frameworks` uniquement.
Aucune crate nouvelle. Les outils CLI fonctionnent sans Sparkle ; leur commande de
mise à jour est désactivée. Le contrôleur reste retenu sur le fil AppKit.

La clé privée Ed25519 reste dans le trousseau, compte **com.faux.mural**.
`Info.plist` contient uniquement la clé publique. Conserver une sauvegarde sécurisée
de la clé hors du dépôt ; ne pas régénérer la clé à chaque release.
La signature Ed25519 d'une archive ne remplace pas la signature Developer ID ni la
notarisation Apple. Ces dernières ne sont pas encore validées pour Mural.

## Préparation d'une release (locale, sans publication)

1. Augmenter `CFBundleVersion` (strictement croissant), la version commerciale et
   `Cargo.toml` ; mettre à jour `Cargo.lock` avec Cargo. Ajouter les changements.
2. Construire et signer l'app. Pour la distribution notarisée, utiliser
   `scripts/distribuer-beta.sh` avec les identifiants Developer ID/Notary déjà
   configurés. Ce script signe aussi les helpers Sparkle de l'intérieur vers l'extérieur.
3. Après notarisation/stapling éventuels, préparer le ZIP final :

```sh
./scripts/preparer-mise-a-jour.sh /chemin/Mural.app /chemin/nouveau-dossier
```

Le script vérifie la signature du bundle et la correspondance clé publique/trousseau,
crée `Mural-VERSION.zip` et `appcast.xml` avec `generate_appcast`.
Le flux contient l'URL du ZIP sous la release `vVERSION` du dépôt public Homebrew.
Les signatures des archives sont requises avant extraction (`SUVerifyUpdateBeforeExtraction`).
Aucun secret n'est écrit dans le dossier de sortie. Ne pas modifier le ZIP après signature.

4. **Sur demande explicite**, publier dans la même release stable `vVERSION` du tap
   le ZIP, le flux et le DMG/Homebrew éventuel. Joindre l'archive source GPL correspondant
   exactement au binaire (script `archiver-source.sh`). Vérifier les téléchargements
   anonymes et la concordance du cask. Pas de publication depuis les scripts ci-dessus.
5. Valider un passage de l'ancienne version Sparkle à la nouvelle dans une copie
   temporaire : détection, téléchargement, confirmation, remplacement, relance et
   conservation des réglages. La recette du moteur ne prouve pas ce parcours complet.

## État de validation locale

La recette native vérifie le menu, sa cible Sparkle, le démarrage réel de SPUUpdater,
la configuration HTTPS et l'isolation des préférences (identifiant de bundle unique).
Les vérifications automatiques sont désactivées uniquement dans le bundle de recette.
Le test de signature doit accepter le ZIP intact et refuser une copie altérée.

Le flux de production n'a pas été publié par cette intégration. Son indisponibilité
produit une erreur de recherche ; aucune ancienne version non signée n'est acceptée.
Une mise à jour complète depuis GitHub reste à valider après publication.

Sources : https://sparkle-project.org/documentation/ et
https://sparkle-project.org/documentation/programmatic-setup/.

### Preuves du 24/10/2026

- `cargo test --offline` : 81 tests unitaires + 3 AVIF + 12 conversion = 96 réussis.
- Recette native verte : `/private/tmp/mural-updates-test.4elUTG` (moteur démarré,
  menu et URL vérifiés ; zéro consultation réseau dans ce processus).
- `make bundle` : build optimisé et signature SnapLayout Dev réussis, framework inclus.
- `target/mise-a-jour-0.1.1/` : archive réelle de l'app et appcast produits avec
  `generate_appcast`, build 2, URL et taille contrôlées.
- `sign_update --verify` : ZIP intact accepté ; copie avec octets ajoutés refusée.
- App réelle relancée et commande `Check for Updates…` observée via accessibilité.
- Revue indépendante en lecture seule : aucun défaut bloquant relevé.
- Aucune publication, aucun commit/push ; notarisation et installation complète
  depuis un flux GitHub publié non vérifiées.
