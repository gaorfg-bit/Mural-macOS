# Mural — macOS

Portage macOS de **Mural**, le gestionnaire de fonds d'ecran GNOME.
Application native en **Swift** (SwiftUI + AppKit), **sans aucune dependance**,
distribuable hors Mac App Store. Licence **GPL-3.0** (oeuvre derivee, voir `NOTICE.md`).

## Ce qui fonctionne aujourd'hui

- **Ecrans** : detection (nom, resolution, disposition, ecran principal), suivi des
  changements de disposition.
- **Bibliotheque** : dossier courant, liste non recursive, tri par nom ou par date,
  plafond de 500 images, extension filtree.
- **Vignettes** : decodees par **ImageIO** directement a l'echelle demandee
  (orientation EXIF respectee), recadrage « cover » au rapport de la cellule,
  chargement par lots de 8 avec 4 taches simultanees, progression visible.
- **Fond d'ecran** : une image **par ecran** ou la meme partout
  (`NSWorkspace.setDesktopImageURL`, API publique). En monocible, les autres ecrans
  ne sont **pas** ecrases.
- **Interface** : barre compacte (dossier, recherche, ecrans, appliquer, menu,
  panneau), grand apercu, grille serree, barre d'etat (nom, dimensions, taille),
  contour de selection a la couleur d'accent, indicateurs « fond actif » et
  « favori du diaporama », menu contextuel complet (appliquer, reveler, copier le
  chemin, trier, favori, supprimer avec confirmation).
- **Panneau lateral** en surcouche, 4 onglets : Display, Slideshow, Folders, AVIF.
- **Journal** : `~/Library/Logs/Mural/mural.log` (chaque action, chaque erreur).

## Commandes

```bash
swift build                 # compilation
swift test                  # 9 tests unitaires (parite avec l'original)
make bundle                 # cree Mural.app (Info.plist + icone + signature)
make run                    # lance l'application
swift run sonde <dossier>   # mesures du backend, sans interface
make render DOSSIER=<chemin>  # rendu hors ecran de l'interface (PNG)
```

Minimum : **macOS 13.0**. Bundle `com.faux.mural`.

## Ce qui n'est pas encore fait

- **Diaporama** : l'interface et les donnees existent (playlists par ecran, mode
  100 % manuel), le **moteur de rotation** non — donc pas encore de rotation apres
  fermeture de la fenetre.
- **Cache disque des vignettes** : seules les memes images sont reutilisees en
  memoire ; le cache sur disque reste a brancher.
- **AVIF** : lecture native (ImageIO) ; la **conversion** est desactivee, car
  ImageIO n'expose aucun encodeur AVIF. Aucun bouton actif pour un comportement
  inexistant.
- **Ecran verrouille** : impossible, macOS n'offre aucune API publique. L'option de
  l'original existait mais n'etait jamais appliquee.
- Les **interactions** (clic, clavier, onglets du panneau) sont compilees mais pas
  encore exercees.

## Differences assumees avec l'application GNOME

- Pas de **composite** multi-ecrans : macOS applique une image independante par
  ecran, donc `build_canvas`, `composite_key` et la purge des composites n'ont pas
  d'equivalent et ne sont pas portes.
- Pas de **daemon D-Bus** : un seul processus.
- Le selecteur **Display mode** etait enregistre sans effet sous GNOME ; ici il
  pilote reellement une image par ecran ou la meme partout.
- Cache dans `~/Library/Caches/Mural/` plutot que dans les dossiers d'images.
