# AGENTS.md — Mural (macOS)

Portage macOS de Mural (GNOME). Swift + SwiftUI + AppKit, **aucune dependance**.
Licence GPL-3.0 (oeuvre derivee) : voir `NOTICE.md`.

## Commandes

```bash
swift build / swift test          # 0 erreur, 9 tests
make bundle / make run            # paquet Mural.app signe
swift run sonde <dossier>         # mesures backend, sans interface
make render DOSSIER=<chemin>      # rendu hors ecran de l'interface
```

## Architecture

| module | equivalent d'origine |
| --- | --- |
| `EcransService` | `mural/monitors.py` |
| `FondEcranService` | `mural/backend.py` + `wallpaper_ops.py` |
| `BibliothequeService` | `mural/config.py` + `thumbnails.py` |
| `VignettesService` | `mural/thumbnails.py` (Thumbnailer, ImageLoader) |
| `Reglages` + `ReglagesDepot` | `mural/models.py` + `config.py` |
| `Diaporama.swift` (playlists) | `mural/models.py` + `slideshow.py` |
| `AvifService` | `mural/avif_cache.py` |
| `VueMural` + `PanneauLateral` | `mural/app.py` |

## Pieges deja payes

- **Aucun libelle ne doit etre invente** : reprendre les libelles de `app.py`.
- **`NSWorkspace.setDesktopImageURL` par ecran** : ne jamais reproduire le composite
  GNOME.
- **Recadrage** : `THUMBNAIL_ASPECT` est un rapport **hauteur/largeur** (110/180) ;
  l'inverser etire les images portrait. Le recadrage doit se faire **apres** avoir
  demande 4x la largeur cible a ImageIO, sinon la vignette est trop petite et molle.
- **Reparation des reglages** : les cles JSON sont en snake_case (`slideshow_images`,
  `per_monitor`, `slideshow_images_per_monitor`) — reparer avec les noms Swift ne
  repare rien.
- **Rendu hors ecran** : forcer l'apparence sur **l'application et la fenetre**, sinon
  les libelles sont blancs sur blanc hors bundle.

- **Demonstration « sequentielle »** : `Reglages()` prend `diaporamaAleatoire = true`
  par defaut (fidelite a `models.py:33`). Toute demonstration sequentielle doit
  poser le drapeau a `false` **explicitement**, sinon elle tire au hasard en
  s'appelant sequentielle — et on croit a un bug du moteur.
- **Verifier la donnee avant d'accuser le moteur** : afficher la playlist (ou la
  liste) avant de conclure. Trois faux diagnostics d'affilee venaient d'un
  harnais trompeur, pas du code.

## Interdits

- **Ne jamais modifier, deplacer ni supprimer une image de l'utilisateur.**
- Ni commit ni push sans demande explicite.
- Aucune API privee, aucun outil tiers, aucun acces reseau.
- Ne pas afficher une commande active dont le comportement n'existe pas.
