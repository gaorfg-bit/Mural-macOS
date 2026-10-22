import XCTest
@testable import MuralKit

/// Tests de parite avec le comportement de l'application GNOME.
/// Les cas viennent de `scripts/test_units.py` (depot de reference).

final class ReglagesTests: XCTestCase {
    /// `test_from_dict_repairs_bad_types` : une vieille config abimee ne doit
    /// jamais produire d'etat incoherent.
    func testReparationDesTypes() {
        let brut: [String: Any] = [
            "slideshow_images": "one.jpg",              // chaine au lieu d'une liste
            "per_monitor": ["dp-1"],                    // liste au lieu d'un dictionnaire
            "folder_bookmarks": 42,                     // type inconnu
            "slideshow_monitors": ["eDP-1"],            // valide
            "legacy_removed_field": ["old": true],      // cle inconnue
        ]
        let reglages = Reglages(dictionnaire: brut)
        XCTAssertEqual(reglages.diaporamaImages, ["one.jpg"])
        XCTAssertTrue(reglages.imageParEcran.isEmpty)
        XCTAssertTrue(reglages.favorisDossiers.isEmpty)
        XCTAssertEqual(reglages.diaporamaEcrans, ["eDP-1"])
    }

    /// `test_from_dict_accepts_valid_config`.
    func testConfigValide() {
        let brut: [String: Any] = [
            "folder": "/tmp",
            "slideshow_images": ["a.jpg", "b.jpg"],
            "per_monitor": ["eDP-1": "/tmp/a.jpg"],
        ]
        let reglages = Reglages(dictionnaire: brut)
        XCTAssertEqual(reglages.dossier, "/tmp")
        XCTAssertEqual(reglages.diaporamaImages, ["a.jpg", "b.jpg"])
        XCTAssertEqual(reglages.imageParEcran, ["eDP-1": "/tmp/a.jpg"])
    }

    /// `test_from_dict_non_dict`.
    func testDictionnaireVide() {
        let reglages = Reglages(dictionnaire: [:])
        XCTAssertEqual(reglages.dossier, "")
        XCTAssertEqual(reglages.diaporamaIntervalle, 10)
        XCTAssertTrue(reglages.diaporamaAleatoire)
    }

    /// Aller-retour : ce qui est encode doit se relire a l'identique.
    func testAllerRetour() throws {
        var reglages = Reglages()
        reglages.dossier = "/tmp/images"
        reglages.memeImagePartout = true
        reglages.imageParEcran = ["Ecran@0,0": "/tmp/a.jpg"]
        reglages.diaporamaImagesParEcran = ["Ecran@0,0": ["/tmp/a.jpg", "/tmp/b.jpg"]]
        reglages.favorisDossiers = ["/tmp/images"]

        let donnees = try JSONSerialization.data(withJSONObject: reglages.enDictionnaire())
        let brut = try XCTUnwrap(JSONSerialization.jsonObject(with: donnees) as? [String: Any])
        let relu = Reglages(dictionnaire: brut)

        XCTAssertEqual(relu.dossier, "/tmp/images")
        XCTAssertTrue(relu.memeImagePartout)
        XCTAssertEqual(relu.imageParEcran, ["Ecran@0,0": "/tmp/a.jpg"])
        XCTAssertEqual(relu.diaporamaImagesParEcran["Ecran@0,0"], ["/tmp/a.jpg", "/tmp/b.jpg"])
    }
}

final class DiaporamaTests: XCTestCase {
    /// Mode 100 % manuel : seules les images ajoutees jouent, et les chemins
    /// disparus sont ecartes (`_clean_existing_paths`).
    func testPlaylistManuelleEtFiltrage() throws {
        let dossier = FileManager.default.temporaryDirectory
            .appendingPathComponent("mural-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dossier, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dossier) }

        let existante = dossier.appendingPathComponent("a.png")
        try Data([0x89, 0x50]).write(to: existante)
        let disparue = dossier.appendingPathComponent("b.png")

        var reglages = Reglages()
        reglages.ajouterAuDiaporama(existante.path)
        reglages.ajouterAuDiaporama(disparue.path)

        XCTAssertEqual(reglages.playlistDiaporama(), [existante.path])
        XCTAssertTrue(reglages.estDansDiaporama(disparue.path))
        XCTAssertFalse(reglages.estDansDiaporama(dossier.appendingPathComponent("c.png").path))
    }

    /// Un ecran sans favoris dedies ne joue rien : pas de repli sur la liste
    /// globale (desaccord corrige cote daemon).
    func testPasDeRepliSurLaListeGlobale() {
        var reglages = Reglages()
        reglages.diaporamaImages = ["/tmp/global.png"]
        XCTAssertTrue(reglages.playlistDiaporama(ecran: "Ecran@0,0").isEmpty)
    }

    func testAjoutRetraitParEcran() {
        var reglages = Reglages()
        reglages.ajouterAuDiaporama("/tmp/a.png", ecran: "Ecran@0,0")
        reglages.ajouterAuDiaporama("/tmp/a.png", ecran: "Ecran@0,0")   // doublon ignore
        XCTAssertEqual(reglages.diaporamaImagesParEcran["Ecran@0,0"], ["/tmp/a.png"])
        XCTAssertTrue(reglages.estDansDiaporama("/tmp/a.png", ecran: "Ecran@0,0"))
        reglages.retirerDuDiaporama("/tmp/a.png", ecran: "Ecran@0,0")
        XCTAssertTrue(reglages.playlistDiaporama(ecran: "Ecran@0,0").isEmpty)
    }

    /// Mode sequentiel : on avance dans l'ordre puis on boucle.
    func testChoixSequentiel() {
        let playlist = ["1.png", "2.png", "3.png"]
        var index = 0
        XCTAssertEqual(ChoixDiaporama.suivant(playlist: playlist, aleatoire: false, index: &index), "1.png")
        XCTAssertEqual(ChoixDiaporama.suivant(playlist: playlist, aleatoire: false, index: &index), "2.png")
        XCTAssertEqual(ChoixDiaporama.suivant(playlist: playlist, aleatoire: false, index: &index), "3.png")
        XCTAssertEqual(ChoixDiaporama.suivant(playlist: playlist, aleatoire: false, index: &index), "1.png")
    }

    func testChoixAleatoireDeterministe() {
        let playlist = ["1.png", "2.png", "3.png"]
        var index = 0
        XCTAssertEqual(ChoixDiaporama.suivant(playlist: playlist, aleatoire: true, index: &index, tirage: 1), "2.png")
        XCTAssertEqual(ChoixDiaporama.suivant(playlist: [], aleatoire: true, index: &index), nil)
    }
}
