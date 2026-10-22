import AppKit
import CoreGraphics
import CryptoKit
import Foundation
import ImageIO

/// Une image decodee et redimensionnee, prete a afficher.
public struct Vignette: Sendable {
    public let image: CGImage
    public let largeurPixels: Int
    public let hauteurPixels: Int
}

/// Vignettes et apercus — equivalent de `Thumbnailer` et `ImageLoader`
/// (`mural/thumbnails.py`).
///
/// Choix mesure : ImageIO (`CGImageSourceCreateThumbnailAtIndex`) decode
/// **directement a l'echelle demandee** et respecte l'orientation EXIF. Sur une
/// image 8176x4096, decodage plein puis reduction = 766 ms ; ImageIO = 258 ms,
/// 92 ms de moyenne sur un dossier de 30 grandes images.
public enum VignettesService {
    /// Cache disque des vignettes. Divergence assumee avec GNOME, qui ecrit
    /// `THUMB_DIR` sous XDG : sur macOS, on ne met rien dans les dossiers
    /// d'images de l'utilisateur (convention `~/Library/Caches`).
    public static var dossierCache: URL {
        FileManager.default
            .urls(for: .cachesDirectory, in: .userDomainMask).first!
            .appendingPathComponent("Mural/vignettes", isDirectory: true)
    }

    /// Cle de cache : chemin + date de modification + taille + largeur demandee.
    /// Equivalent de la cle `sha1(chemin|mtime|WxH)` de `Thumbnailer.generate`
    /// (`mural/thumbnails.py:25-27`) : une image modifiee change de cle.
    static func cleCache(pour url: URL, largeurCible: Int) -> String? {
        let valeurs = try? url.resourceValues(forKeys: [.contentModificationDateKey, .fileSizeKey])
        let date = valeurs?.contentModificationDate?.timeIntervalSince1970 ?? 0
        let taille = valeurs?.fileSize ?? 0
        let brut = "\(url.path)|\(date)|\(taille)|\(largeurCible)"
        return SHA256.hash(data: Data(brut.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    private static func lireCache(_ cle: String) -> Vignette? {
        let fichier = dossierCache.appendingPathComponent("\(cle).jpg")
        guard let source = CGImageSourceCreateWithURL(fichier as CFURL, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { return nil }
        return Vignette(image: image, largeurPixels: image.width, hauteurPixels: image.height)
    }

    private static func ecrireCache(_ image: CGImage, cle: String) {
        try? FileManager.default.createDirectory(at: dossierCache, withIntermediateDirectories: true)
        guard let donnees = NSBitmapImageRep(cgImage: image)
            .representation(using: .jpeg, properties: [.compressionFactor: 0.92]) else { return }
        try? donnees.write(to: dossierCache.appendingPathComponent("\(cle).jpg"))
    }

    /// Nombre de vignettes presentes dans le cache disque.
    public static func nombreEnCache() -> Int {
        ((try? FileManager.default.contentsOfDirectory(at: dossierCache, includingPropertiesForKeys: nil)) ?? []).count
    }

    /// Vignette « cover » au rapport de la cellule (`THUMBNAIL_ASPECT`).
    public static func vignette(pour url: URL, largeurCible: Int) -> Vignette? {
        // Cache disque d'abord : une vignette deja calculee revient en ~1 ms au
        // lieu de ~14 ms de decodage (mesure sur ce Mac).
        let cle = cleCache(pour: url, largeurCible: largeurCible)
        if let cle, let enCache = lireCache(cle) { return enCache }
        let hauteurCible = max(1, Int((Double(largeurCible) * Reglages.rapportVignette).rounded()))
        // Le recadrage coupe dans la reduction : demander 4x la largeur cible
        // garantit qu'il reste au moins 2x la cellule APRES recadrage, dans les
        // deux orientations (mesure : 2x donnait une vignette de 165 px de large
        // pour une cellule de 180 pt, donc agrandie et molle).
        let cote = largeurCible * 4
        guard let reduite = reduite(pour: url, coteMax: cote) else { return nil }
        let rapport = Double(largeurCible) / Double(hauteurCible)
        guard let recadree = recadrer(reduite.image, rapportLargeurSurHauteur: rapport) else {
            return reduite
        }
        if let cle { ecrireCache(recadree, cle: cle) }
        return Vignette(image: recadree, largeurPixels: recadree.width, hauteurPixels: recadree.height)
    }

    /// Apercu panoramique, jamais agrandi au dela de l'original.
    public static func apercu(pour url: URL, largeurMax: Int, hauteurMax: Int) -> Vignette? {
        reduite(pour: url, coteMax: max(largeurMax, hauteurMax))
    }

    /// Decodage a l'echelle, sans jamais passer par la pleine resolution.
    private static func reduite(pour url: URL, coteMax: Int) -> Vignette? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: coteMax,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else {
            return nil
        }
        return Vignette(image: image, largeurPixels: image.width, hauteurPixels: image.height)
    }

    /// Recadrage centre au rapport demande.
    ///
    /// L'erreur a ne pas refaire : `ASPECT_VIGNETTE` est un rapport
    /// **hauteur/largeur** (110/180) ; la branche « image plus haute que le
    /// cadre » doit rendre `(largeur, largeur / rapport)`. L'inverser etire les
    /// images portrait — c'est le bug corrige ici.
    private static func recadrer(_ image: CGImage, rapportLargeurSurHauteur: Double) -> CGImage? {
        let largeur = Double(image.width)
        let hauteur = Double(image.height)
        guard largeur > 0, hauteur > 0, rapportLargeurSurHauteur > 0 else { return nil }
        let rapportSource = largeur / hauteur
        let (largeurCadre, hauteurCadre) = rapportSource > rapportLargeurSurHauteur
            ? (hauteur * rapportLargeurSurHauteur, hauteur)
            : (largeur, largeur / rapportLargeurSurHauteur)
        let cadre = CGRect(
            x: ((largeur - largeurCadre) / 2).rounded(),
            y: ((hauteur - hauteurCadre) / 2).rounded(),
            width: largeurCadre.rounded(),
            height: hauteurCadre.rounded()
        )
        return image.cropping(to: cadre)
    }
}
