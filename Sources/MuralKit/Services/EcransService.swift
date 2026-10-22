import AppKit

/// Un ecran connecte — equivalent de `MonitorInfo` (`mural/models.py:8-17`) et
/// de `MonitorDetector.detect()` (`mural/monitors.py`).
///
/// Difference assumee : macOS n'expose pas de nom de connecteur (`DP-1`…).
/// L'identite persistee est donc « nom du systeme + position », qui joue le
/// meme role que `connector` dans les reglages `per_monitor` / `monitor_folders`.
public struct EcranInfo: Identifiable, Hashable, Sendable {
    /// Cle stable pour les reglages (joue le role de `connector`).
    public let id: String
    /// Position dans `NSScreen.screens` (valable pour la session courante).
    public let index: Int
    public let nom: String
    public let largeur: Int
    public let hauteur: Int
    public let x: Int
    public let y: Int
    public let principal: Bool

    /// Etiquette affichee par l'interface : « Monitor 1 », « Monitor 2 (primary) ».
    public var etiquette: String {
        "Monitor \(index + 1)" + (principal ? " (primary)" : "")
    }

    /// Sous-titre de l'onglet Display : nom, resolution, position
    /// (equivalent de `_monitor_markup`, `mural/app.py:914-920`).
    public var descriptionDetaillee: String {
        "\(nom)\(principal ? " (primary)" : "")\n\(largeur) × \(hauteur) — pos(\(x), \(y))"
    }
}

public enum EcransService {
    /// Enumere les ecrans en pixels logiques, comme `MonitorDetector.detect()`
    /// (`mural/monitors.py:12-24` — l'application GNOME documente que les pixels
    /// physiques produisent un composite trop grand ; ici c'est la meme
    /// convention que celle utilisee par `NSWorkspace`).
    public static func lister() -> [EcranInfo] {
        let ecrans = NSScreen.screens
        guard let principal = NSScreen.main else {
            return ecrans.enumerated().map { index, ecran in
                Self.information(pour: ecran, index: index, principal: index == 0)
            }
        }
        return ecrans.enumerated().map { index, ecran in
            Self.information(pour: ecran, index: index, principal: ecran === principal)
        }
    }

    private static func information(pour ecran: NSScreen, index: Int, principal: Bool) -> EcranInfo {
        let cadre = ecran.frame
        let nom = ecran.localizedName
        let x = Int(cadre.origin.x.rounded())
        let y = Int(cadre.origin.y.rounded())
        return EcranInfo(
            id: "\(nom)@\(x),\(y)",
            index: index,
            nom: nom,
            largeur: Int(cadre.size.width.rounded()),
            hauteur: Int(cadre.size.height.rounded()),
            x: x,
            y: y,
            principal: principal
        )
    }

    /// Ecran `NSScreen` correspondant a un index de la liste precedente.
    public static func nsEcran(index: Int) -> NSScreen? {
        let ecrans = NSScreen.screens
        guard index >= 0 && index < ecrans.count else { return nil }
        return ecrans[index]
    }

    /// Nom de la notification systeme a observer pour suivre les changements de
    /// disposition (equivalent de `items-changed`, `mural/app.py:171-173`).
    public static var notificationChangementEcrans: Notification.Name {
        NSApplication.didChangeScreenParametersNotification
    }
}
