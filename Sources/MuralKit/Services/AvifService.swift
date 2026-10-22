import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// Capacites AVIF **reelles** du systeme — equivalent de `avif_supported()`
/// (`mural/avif_cache.py:56-71`), mais sans ImageMagick : macOS lit l'AVIF
/// nativement, et l'ecriture depend de ce qu'ImageIO expose vraiment.
/// On ne suppose rien : on interroge le systeme.
public enum AvifService {
    public static let typesEcriture: [String] =
        (CGImageDestinationCopyTypeIdentifiers() as? [String]) ?? []
    public static let typesLecture: [String] =
        (CGImageSourceCopyTypeIdentifiers() as? [String]) ?? []

    /// Ecriture AVIF : presque toujours indisponible sur macOS.
    public static var encodageDisponible: Bool { typesEcriture.contains("public.avif") }

    /// Lecture AVIF : disponible depuis macOS 13.
    public static var lectureDisponible: Bool { typesLecture.contains("public.avif") }

    public static func resumeCapacites() -> String {
        "ImageIO : lecture AVIF \(lectureDisponible ? "oui" : "non"), "
            + "ecriture AVIF \(encodageDisponible ? "oui" : "non") "
            + "(\(typesEcriture.count) types en ecriture)."
    }
}
