import AppKit
import SwiftUI
import MuralKit

// Entree de l'application. A ce stade (etape 2 du plan) la fenetre est un
// support minimal : le backend (ecrans, fonds, bibliotheque, vignettes) est
// verifie par l'outil `sonde` et par les tests. L'interface fidele (barre,
// apercu, grille, barre d'etat, panneau 4 onglets) arrive a l'etape 4.

@MainActor
final class DelegueApplication: NSObject, NSApplicationDelegate {
    /// Etat residant : l'application ne meurt pas avec sa fenetre, donc la
    /// rotation peut continuer (equivalent du daemon systemd de GNOME).
    private var elementBarreMenus: NSStatusItem?
    private let diaporama = DiaporamaService()

    private var fenetre: NSWindow?

    func applicationDidFinishLaunching(_ notification: Notification) {
        Journal.demarrage()

        // Verification visuelle : `Mural --rendu sortie.png [dossier]` dessine
        // la meme interface dans un bitmap, sans afficher de fenetre et sans
        // permission systeme (aucune capture d'ecran).
        let arguments = CommandLine.arguments
        if let position = arguments.firstIndex(of: "--rendu") {
            let sortie = arguments.count > position + 1 ? arguments[position + 1] : "rendu.png"
            let dossier = arguments.count > position + 2
                ? URL(fileURLWithPath: arguments[position + 2])
                : FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Pictures")
            rendreHorsEcran(sortie: sortie, dossier: dossier)
            NSApplication.shared.terminate(nil)
            return
        }

        let ecrans = EcransService.lister()
        Journal.info("ecrans detectes : \(ecrans.count)")
        for ecran in ecrans {
            Journal.info("  \(ecran.etiquette) — \(ecran.descriptionDetaillee.replacingOccurrences(of: "\n", with: " / "))")
        }

        let reglages = ReglagesDepot.charger()
        let modele = GalerieModele()
        let dossier = reglages.dossier.isEmpty
            ? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Pictures")
            : URL(fileURLWithPath: reglages.dossier)
        let images = BibliothequeService.lister(dossier: dossier, tri: .nom)
        Journal.info("dossier « \(dossier.path) » : \(images.count) image(s)")

        let cadre = NSRect(x: 0, y: 0, width: 640, height: 220)
        let fenetre = NSWindow(
            contentRect: cadre,
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        fenetre.title = "Mural — \(ecrans.count) screen\(ecrans.count > 1 ? "s" : "")"

        fenetre.contentView = NSHostingView(rootView: VueMural(modele: modele))
        fenetre.center()
        fenetre.makeKeyAndOrderFront(nil)
        self.fenetre = fenetre
        installerBarreMenus()
        if reglages.diaporamaActif { diaporama.demarrer(intervalleMinutes: reglages.diaporamaIntervalle) }
    }

    /// Icone de barre de menus : montre que le processus reste vivant.
    private func installerBarreMenus() {
        let element = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        element.button?.image = NSImage(systemSymbolName: "photo.on.rectangle", accessibilityDescription: "Mural")
        let menu = NSMenu()
        menu.addItem(withTitle: "Show Mural", action: #selector(montrerFenetre), keyEquivalent: "")
        menu.addItem(withTitle: "Next image now", action: #selector(imageSuivante), keyEquivalent: "")
        menu.addItem(withTitle: "Start/Stop slideshow", action: #selector(basculerDiaporama), keyEquivalent: "")
        menu.addItem(.separator())
        menu.addItem(withTitle: "Quit Mural", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        for item in menu.items { item.target = self }
        element.menu = menu
        elementBarreMenus = element
    }

    @objc private func montrerFenetre() { fenetre?.makeKeyAndOrderFront(nil) }

    @objc private func imageSuivante() { diaporama.suivant() }

    @objc private func basculerDiaporama() {
        let reglages = ReglagesDepot.charger()
        if diaporama.actif {
            diaporama.arreter()
        } else {
            diaporama.demarrer(intervalleMinutes: reglages.diaporamaIntervalle)
        }
    }

    /// Rend l'interface dans un PNG, hors ecran.
    @MainActor
    private func rendreHorsEcran(sortie: String, dossier: URL) {
        let largeur: CGFloat = 1180
        let hauteur: CGFloat = 760
        let modele = GalerieModele(dossier: dossier)
        let hosting = NSHostingView(rootView: VueMural(modele: modele))
        hosting.frame = NSRect(x: 0, y: 0, width: largeur, height: hauteur)
        // Un binaire hors bundle garde l'apparence du systeme pour les
        // controles, mais dessine les fonds en clair : on force les DEUX, sinon
        // les libelles deviennent blancs sur blanc.
        let apparence = NSAppearance(named: ProcessInfo.processInfo.environment["MURAL_APPARENCE"] == "dark" ? .darkAqua : .aqua)
        NSApplication.shared.appearance = apparence
        let fenetre = NSWindow(contentRect: hosting.frame, styleMask: [.titled], backing: .buffered, defer: false)
        fenetre.appearance = apparence
        fenetre.contentView = hosting
        fenetre.setFrame(hosting.frame, display: false)

        // Laisser charger les vignettes (lots de 8, 4 taches simultanees).
        let fin = Date().addingTimeInterval(12)
        while Date() < fin {
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        }
        hosting.layoutSubtreeIfNeeded()

        guard let bitmap = hosting.bitmapImageRepForCachingDisplay(in: hosting.bounds) else {
            Journal.erreur("rendu : pas de bitmap")
            return
        }
        hosting.cacheDisplay(in: hosting.bounds, to: bitmap)
        guard let png = bitmap.representation(using: .png, properties: [:]) else {
            Journal.erreur("rendu : encodage PNG impossible")
            return
        }
        let destination = URL(fileURLWithPath: sortie)
        do {
            try png.write(to: destination)
            print("rendu ecrit : \(sortie) (\(png.count) octets, \(Int(largeur))x\(Int(hauteur)))")
        } catch {
            Journal.erreur("rendu : ecriture impossible (\(error.localizedDescription))")
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false   // la rotation survit a la fermeture de la fenetre
    }
}

let application = NSApplication.shared
let delegue = DelegueApplication()
application.delegate = delegue
application.setActivationPolicy(.regular)
application.run()
