import AppKit
import MuralKit

// Verification du backend, sans interface — equivalente de la « sonde » :
// ecrans, fonds actuels (lecture seule), analyse du dossier, vignette, apercu.
// Aucune ecriture, aucune modification d'image.

@MainActor
func executer() {
    Journal.demarrage()
    let arguments = CommandLine.arguments
    let dossier = arguments.count > 1
        ? URL(fileURLWithPath: arguments[1])
        : FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Pictures")

    // 1. Ecrans
    let ecrans = EcransService.lister()
    print("ECRANS : \(ecrans.count)")
    for ecran in ecrans {
        print("  \(ecran.etiquette) — \(ecran.nom) \(ecran.largeur)x\(ecran.hauteur) a (\(ecran.x), \(ecran.y))")
    }

    // 2. Fonds actuels (lecture seule)
    for ecran in ecrans {
        guard let nsEcran = EcransService.nsEcran(index: ecran.index) else { continue }
        let courant = FondEcranService.imageCourante(ecran: nsEcran)
        print("FOND \(ecran.index + 1) : \(courant?.path ?? "(aucun)")")
    }

    // 3. Analyse du dossier
    let debut = Date()
    let images = BibliothequeService.lister(dossier: dossier, tri: .nom)
    let duree = Date().timeIntervalSince(debut) * 1000
    print("DOSSIER : \(dossier.path)")
    print("IMAGES : \(images.count)")
    print(String(format: "TEMPS ANALYSE : %.0f ms", duree))

    // 4. Vignette et dimensions de la premiere image
    if let premier = images.first {
        var t0 = Date()
        let vignette = VignettesService.vignette(pour: premier.url, largeurCible: 180)
        print(String(format: "VIGNETTE 180 : %@ en %.0f ms",
                     vignette.map { "\($0.largeurPixels)x\($0.hauteurPixels)" } ?? "ECHEC",
                     Date().timeIntervalSince(t0) * 1000))

        t0 = Date()
        let dimensions = BibliothequeService.dimensions(de: premier.url)
        print(String(format: "DIMENSIONS : %@ en %.0f ms",
                     dimensions.map { "\($0.largeur)x\($0.hauteur)" } ?? "ECHEC",
                     Date().timeIntervalSince(t0) * 1000))

        t0 = Date()
        let apercu = VignettesService.apercu(pour: premier.url, largeurMax: 1600, hauteurMax: 780)
        print(String(format: "APERCU 1600x780 : %@ en %.0f ms",
                     apercu.map { "\($0.largeurPixels)x\($0.hauteurPixels)" } ?? "ECHEC",
                     Date().timeIntervalSince(t0) * 1000))
    }

    print("RESUME ecrans=\(ecrans.count) images=\(images.count)")
}

@main
enum Sonde {
    @MainActor static func main() { executer() }
}
