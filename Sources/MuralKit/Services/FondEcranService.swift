import AppKit
import Foundation

/// Application du fond d'ecran — equivalent de `mural/wallpaper_ops.py` et
/// `mural/backend.py`, cote macOS.
///
/// Difference structurelle assumee : sous GNOME, Mural fabrique une **image
/// composite** aux dimensions de la boite englobante des moniteurs puis
/// l'applique en `spanned` (`wallpaper_ops.py:80-136`). Sur macOS chaque ecran
/// recoit **sa propre** image : `build_canvas`, `composite_key` et la purge des
/// composites n'ont pas d'equivalent et ne sont pas portes.
public enum ErreurFond: LocalizedError {
    case fichierManquant(URL)
    case extensionRefusee(String)
    case ecranIntrouvable(Int)
    case echecApple(Error)

    public var errorDescription: String? {
        switch self {
        case .fichierManquant(let url): "Fichier introuvable : \(url.lastPathComponent)"
        case .extensionRefusee(let extension_): "Extension non supportee : \(extension_)"
        case .ecranIntrouvable(let index): "Ecran introuvable : index \(index)"
        case .echecApple(let erreur): "Erreur macOS : \(erreur.localizedDescription)"
        }
    }
}

public enum FondEcranService {
    /// Extensions acceptees — reprises de `Config._BASE_EXT` (`config.py:20-27`).
    /// Adaptation macOS : `.avif` est toujours lisible (ImageIO), et `.heic`
    /// / `.heif` sont ajoutes car macOS les decode nativement alors que
    /// l'application GNOME les ignore.
    public static let extensionsAcceptees: Set<String> = [
        "png", "jpg", "jpeg", "bmp", "webp", "svg", "avif", "heic", "heif",
    ]

    /// Nom court de l'extension, ou nil si elle n'est pas geree.
    public static func extensionAcceptee(_ url: URL) -> String? {
        let extension_ = url.pathExtension.lowercased()
        return extensionsAcceptees.contains(extension_) ? extension_ : nil
    }

    /// Applique une image au fond d'un ecran.
    ///
    /// `NSWorkspace.setDesktopImageURL(_:for:options:)` est l'API publique
    /// equivalente de `gsettings picture-uri` + `picture-options=spanned`
    /// (`wallpaper_ops.py:44-57`), avec une difference de fond : elle prend un
    /// `NSScreen`, donc l'independance par ecran est native.
    public static func appliquer(image: URL, ecran: NSScreen, options: [NSWorkspace.DesktopImageOptionKey: Any] = [:]) throws {
        guard FileManager.default.fileExists(atPath: image.path) else {
            throw ErreurFond.fichierManquant(image)
        }
        guard extensionAcceptee(image) != nil else {
            throw ErreurFond.extensionRefusee(image.pathExtension.lowercased())
        }
        do {
            try NSWorkspace.shared.setDesktopImageURL(image, for: ecran, options: options)
        } catch {
            throw ErreurFond.echecApple(error)
        }
    }

    /// Applique a tous les ecrans (equivalent de `assignments` couvrant tous les
    /// moniteurs) et renvoie les echecs par ecran.
    public static func appliquer(image: URL, aTousLesEcrans ecrans: [NSScreen]) -> [(Int, Error)] {
        var echecs: [(Int, Error)] = []
        for (index, ecran) in ecrans.enumerated() {
            do { try appliquer(image: image, ecran: ecran) }
            catch { echecs.append((index, error)) }
        }
        return echecs
    }

    /// Image de fond actuelle d'un ecran, en lecture seule.
    ///
    /// Comble un trou de l'original : `GnomeBackend.get_current()` renvoie `""`
    /// (`backend.py:51`) et `get_mode()` renvoie `"spanned"` en dur.
    public static func imageCourante(ecran: NSScreen) -> URL? {
        NSWorkspace.shared.desktopImageURL(for: ecran)
    }
}
