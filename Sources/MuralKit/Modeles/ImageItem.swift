import Foundation

/// Une image de la galerie.
///
/// Equivalent de l'entree de galerie construite par `_gallery_worker`
/// (`mural/app.py:1998-2074`). L'identite est le chemin absolu, comme la cle
/// `_child_to_path` de l'application GNOME.
public struct ImageItem: Identifiable, Hashable, Sendable {
    public let id: String
    public let url: URL
    public let nom: String
    public let date: Date
    public let tailleOctets: Int64

    public init(url: URL, date: Date, tailleOctets: Int64) {
        self.id = url.path
        self.url = url
        self.nom = url.lastPathComponent
        self.date = date
        self.tailleOctets = tailleOctets
    }
}
