import AppKit
import Foundation

/// Moteur de rotation — equivalent de `SlideshowManager` (`mural/slideshow.py`)
/// et du `_tick` du daemon (`mural-daemon-service.py`).
///
/// Deux regles de l'original sont conservees telles quelles :
/// 1. **config fraiche a chaque tick** : le daemon relit le fichier a chaque
///    rotation, sinon les changements faits dans la fenetre sont perdus ;
/// 2. **pas de repli implicite** : un ecran sans favoris dedies ne joue rien.
@MainActor
public final class DiaporamaService {
    private var minuteur: Timer?
    private var indexSequentiels: [String: Int] = [:]
    public private(set) var actif = false
    /// Derniere decision calculee (par connecteur) : verifiable sans risque.
    public private(set) var dernieresDecisions: [String: String] = [:]

    public init() {}

    /// Decision pure : quelles images pour quels ecrans a ce tick.
    ///
    /// Aucun effet de bord : c'est ce qui rend la rotation verifiable sans
    /// modifier le fond d'ecran de l'utilisateur (`tick(simuler: true)`).
    /// `nonisolated` : la decision ne touche a rien, elle doit etre appelable
    /// depuis les tests (et depuis n'importe quel fil).
    public nonisolated static func prochaineAssignation(
        reglages: Reglages,
        ecrans: [EcranInfo],
        indexSequentiels: inout [String: Int],
        tirage: ((Int) -> Int)? = nil
    ) -> [String: String] {
        let cibles = reglages.diaporamaEcrans.isEmpty ? ecrans.map(\.id) : reglages.diaporamaEcrans
        var choix: [String: String] = [:]
        for connecteur in cibles {
            let playlist = reglages.playlistDiaporama(ecran: connecteur)
            guard !playlist.isEmpty else { continue }
            if reglages.diaporamaAleatoire {
                let position = tirage.map { tirageChoisi in tirageChoisi(playlist.count) % playlist.count }
                    ?? Int.random(in: 0..<playlist.count)
                choix[connecteur] = playlist[position]
            } else {
                // Avancement explicite : +1 par tick, jamais deux fois la meme
                // image tant que la playlist n'est pas epuisee.
                let position = (indexSequentiels[connecteur] ?? 0) % playlist.count
                choix[connecteur] = playlist[position]
                indexSequentiels[connecteur] = position + 1
            }
        }
        return choix
    }

    public func demarrer(intervalleMinutes: Int) {
        arreter()
        let intervalle = TimeInterval(max(Reglages.intervalleMin, intervalleMinutes) * 60)
        let minuteur = Timer.scheduledTimer(withTimeInterval: intervalle, repeats: true) { _ in
            Task { @MainActor in self.tick(simuler: false) }
        }
        self.minuteur = minuteur
        actif = true
        Journal.info("diaporama demarre — toutes les \(Int(intervalle / 60)) min")
    }

    public func arreter() {
        minuteur?.invalidate()
        minuteur = nil
        if actif { Journal.info("diaporama arrete") }
        actif = false
    }

    /// Passage immediat a l'image suivante (« Next image now »).
    public func suivant() {
        tick(simuler: false)
    }

    /// Un tick. `simuler` calcule et journalise la decision **sans** appliquer :
    /// c'est le mode de verification.
    public func tick(simuler: Bool) {
        let reglages = ReglagesDepot.charger()          // config fraiche (regle 1)
        let ecrans = EcransService.lister()
        let choix = Self.prochaineAssignation(
            reglages: reglages,
            ecrans: ecrans,
            indexSequentiels: &indexSequentiels
        )
        dernieresDecisions = choix
        guard !choix.isEmpty else {
            Journal.info("diaporama : aucun ecran n'a de playlist (mode 100 % manuel)")
            return
        }
        for (connecteur, chemin) in choix.sorted(by: { $0.key < $1.key }) {
            guard let ecran = ecrans.first(where: { $0.id == connecteur }) else { continue }
            if simuler {
                Journal.info("diaporama SIMULE ecran \(ecran.index + 1) : \(chemin)")
                continue
            }
            guard let nsEcran = EcransService.nsEcran(index: ecran.index) else { continue }
            do {
                try FondEcranService.appliquer(image: URL(fileURLWithPath: chemin), ecran: nsEcran)
                Journal.info("diaporama ecran \(ecran.index + 1) : \(chemin)")
            } catch {
                Journal.erreur("diaporama ecran \(ecran.index + 1) : \(error.localizedDescription)")
            }
        }
        if !simuler {
            var aEnregistrer = reglages
            for (connecteur, chemin) in choix { aEnregistrer.imageParEcran[connecteur] = chemin }
            _ = ReglagesDepot.sauvegarder(aEnregistrer)
        }
    }
}
