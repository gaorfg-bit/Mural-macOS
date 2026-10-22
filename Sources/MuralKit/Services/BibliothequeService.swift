import Foundation
import ImageIO

/// Ordre de tri — equivalent de `_sort_mode` (`mural/app.py:157`).
public enum Tri { case nom, date }

/// Dimensions en pixels.
public struct Dimensions: Equatable, Sendable {
    public let largeur: Int
    public let hauteur: Int
}

/// Liste les images d'un dossier — equivalent de `_gallery_worker`
/// (`mural/app.py:1998-2022`) et de `Config` (`config.py:32`).
public enum BibliothequeService {
    /// Contenu du PREMIER niveau seulement, comme `folder.iterdir()`.
    /// Extensions acceptees, tri, puis plafond `MAX_IMAGES = 500`.
    public static func lister(dossier: URL, tri: Tri) -> [ImageItem] {
        let cles: [URLResourceKey] = [.isRegularFileKey, .contentModificationDateKey, .fileSizeKey]
        guard let contenu = try? FileManager.default.contentsOfDirectory(
            at: dossier, includingPropertiesForKeys: cles, options: [.skipsHiddenFiles]
        ) else {
            Journal.erreur("lecture du dossier impossible : \(dossier.path)")
            return []
        }

        var images: [ImageItem] = []
        for url in contenu {
            guard let valeurs = try? url.resourceValues(forKeys: Set(cles)) else { continue }
            guard valeurs.isRegularFile == true else { continue }
            guard FondEcranService.extensionAcceptee(url) != nil else { continue }
            images.append(ImageItem(
                url: url,
                date: valeurs.contentModificationDate ?? .distantPast,
                tailleOctets: Int64(valeurs.fileSize ?? 0)
            ))
        }

        switch tri {
        case .nom:
            images.sort { $0.nom.localizedStandardCompare($1.nom) == .orderedAscending }
        case .date:
            images.sort { $0.date > $1.date }
        }

        if images.count > Reglages.limiteImages {
            images = Array(images.prefix(Reglages.limiteImages))
        }
        return images
    }

    /// Dimensions en pixels, lues dans l'en-tete du fichier (aucun decodage) —
    /// equivalent de `ImageLoader.get_dimensions` (`thumbnails.py:75-84`).
    public static func dimensions(de url: URL) -> Dimensions? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let proprietes = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let largeur = proprietes[kCGImagePropertyPixelWidth] as? Int,
              let hauteur = proprietes[kCGImagePropertyPixelHeight] as? Int
        else { return nil }
        return Dimensions(largeur: largeur, hauteur: hauteur)
    }
}
