import Foundation

// MARK: - Playlists de diaporama
//
// Portage fidele de `mural/models.py:76-120`. Regle structurante a conserver :
// le diaporama est en **mode 100 % manuel** — seules les images ajoutees a la
// main jouent ; un ecran sans favoris dedies ne joue **rien**, il ne retombe
// pas sur la liste globale (desaccord corrige cote daemon, voir
// `mural-daemon-service.py`, `_get_playlist`).

public extension Reglages {
    /// Liste finale pour un ecran (ou la liste globale si `ecran == nil`),
    /// chemins disparus filtres et doublons retires (`_clean_existing_paths`).
    func playlistDiaporama(ecran: String? = nil) -> [String] {
        let brute: [String]
        if let ecran {
            brute = diaporamaImagesParEcran[ecran] ?? []
        } else {
            brute = diaporamaImages
        }
        var vus = Set<String>()
        var propre: [String] = []
        for chemin in brute {
            guard !vus.contains(chemin) else { continue }
            guard FileManager.default.fileExists(atPath: chemin) else { continue }
            vus.insert(chemin)
            propre.append(chemin)
        }
        return propre
    }

    /// Vrai uniquement si l'image a ete ajoutee a la main (`is_in_slideshow`).
    func estDansDiaporama(_ chemin: String, ecran: String? = nil) -> Bool {
        if let ecran, let liste = diaporamaImagesParEcran[ecran] {
            return liste.contains(chemin)
        }
        return diaporamaImages.contains(chemin)
    }

    mutating func ajouterAuDiaporama(_ chemin: String, ecran: String? = nil) {
        if let ecran {
            var liste = diaporamaImagesParEcran[ecran] ?? []
            if !liste.contains(chemin) { liste.append(chemin) }
            diaporamaImagesParEcran[ecran] = liste
            return
        }
        if !diaporamaImages.contains(chemin) { diaporamaImages.append(chemin) }
    }

    mutating func retirerDuDiaporama(_ chemin: String, ecran: String? = nil) {
        if let ecran, var liste = diaporamaImagesParEcran[ecran] {
            liste.removeAll { $0 == chemin }
            diaporamaImagesParEcran[ecran] = liste
            return
        }
        diaporamaImages.removeAll { $0 == chemin }
    }
}

/// Choix de l'image suivante — logique pure, testable.
///
/// Equivalent de `SlideshowManager._tick` (`mural/slideshow.py:30-66`) et du
/// `_tick` du daemon : tirage au hasard en mode aleatoire, index sequentiel par
/// ecran sinon.
public enum ChoixDiaporama {
    /// Renvoie le chemin choisi, et avance l'index sequentiel de l'ecran.
    /// `tirage` permet un test deterministe (0..<playlist.count) ; nil = hasard.
    public static func suivant(
        playlist: [String],
        aleatoire: Bool,
        index: inout Int,
        tirage: Int? = nil
    ) -> String? {
        guard !playlist.isEmpty else { return nil }
        if aleatoire {
            let position = tirage.map { $0 % playlist.count } ?? Int.random(in: 0..<playlist.count)
            return playlist[position]
        }
        let choisi = playlist[index % playlist.count]
        index += 1
        return choisi
    }
}
