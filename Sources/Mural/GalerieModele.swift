import AppKit
import Combine
import MuralKit

/// Etat observable de la galerie — sert l'interface et centralise les regles.
///
/// Equivalent de l'etat de `MuralWindow` (`mural/app.py:144-168`) : dossier
/// courant, liste d'images, selection, texte de recherche, tri, progression.
@MainActor
public final class GalerieModele: ObservableObject {

    public enum Etat: Equatable {
        case vide
        case chargement(progression: Double)
        case pret(nombre: Int)
        case erreur(String)
    }

    @Published public private(set) var dossier: URL
    @Published public private(set) var images: [ImageItem] = []
    @Published public private(set) var vignettes: [String: CGImage] = [:]
    @Published public private(set) var etat: Etat = .vide
    @Published public var selection: String?
    @Published public var recherche: String = ""
    @Published public var tri: Tri = .nom
    @Published public var ecranCourant: Int = 0
    @Published public var memeImagePartout: Bool = false
    @Published public var ecrans: [EcranInfo] = []
    @Published public private(set) var fondsActifs: Set<String> = []
    @Published public var reglages = Reglages()

    /// Texte de la barre d'etat (`_status`, app.py:922-927).
    @Published public var statut: String = "Select an image"

    private var generation = 0
    private var tachesVignettes: Task<Void, Never>?

    public init(dossier: URL? = nil) {
        let chargees = ReglagesDepot.charger()
        self.reglages = chargees
        self.memeImagePartout = chargees.memeImagePartout
        self.tri = Tri(brut: chargees.dossier.isEmpty ? "nom" : chargees.mode == "" ? "nom" : "nom")
        self.ecrans = EcransService.lister()
        let depart = dossier
            ?? (chargees.dossier.isEmpty ? nil : URL(fileURLWithPath: chargees.dossier))
            ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Pictures")
        self.dossier = depart
        releverFondsActifs()
    }

    // MARK: - Images affichees

    /// Images visibles : filtre de recherche sur le nom (`_flowbox_filter`,
    /// app.py:1156-1161).
    public var imagesFiltrees: [ImageItem] {
        guard !recherche.isEmpty else { return images }
        return images.filter { $0.nom.localizedCaseInsensitiveContains(recherche) }
    }

    public var imageSelectionnee: ImageItem? {
        guard let selection else { return nil }
        return images.first { $0.id == selection }
    }

    public var ecranActif: EcranInfo? {
        ecrans.indices.contains(ecranCourant) ? ecrans[ecranCourant] : ecrans.first
    }

    /// Libelle du bouton principal (`_update_apply_btn_subtitle`, app.py:1066-1076).
    public var libelleAppliquer: String {
        if ecrans.count <= 1 || memeImagePartout { return "Apply (All)" }
        return "Apply (Monitor \(ecranCourant + 1))"
    }

    // MARK: - Chargement

    /// Relit le dossier puis charge les vignettes par lots, en annulant le
    /// chargement precedent (`_load_gallery` + `_gallery_worker`).
    public func charger(dossier: URL? = nil) {
        if let dossier, dossier != self.dossier {
            self.dossier = dossier
            recherche = ""
        }
        generation += 1
        let generationCourante = generation
        tachesVignettes?.cancel()

        let trouvees = BibliothequeService.lister(dossier: dossier ?? self.dossier, tri: tri)
        images = trouvees
        vignettes = [:]
        etat = trouvees.isEmpty ? .vide : .chargement(progression: 0)
        statut = trouvees.isEmpty ? "No images found" : "\(trouvees.count) images — \(self.dossier.lastPathComponent)"
        Journal.info("galerie : \(trouvees.count) image(s) dans \(self.dossier.path)")
        if selection == nil || !trouvees.contains(where: { $0.id == selection }) {
            selection = trouvees.first?.id
        }
        releverFondsActifs()

        guard !trouvees.isEmpty else { return }
        tachesVignettes = Task { [weak self] in
            await self?.chargerVignettes(de: trouvees, generation: generationCourante)
        }
    }

    /// Vignettes par lots de 8, 4 taches simultanees (`BATCH_SIZE = 8`,
    /// limite de 4 dans `_gallery_worker`).
    private func chargerVignettes(de liste: [ImageItem], generation courante: Int) async {
        let lots = liste.chunked(taille: 8)
        var faites = 0
        for lot in lots {
            if Task.isCancelled || courante != generation { return }
            let resultats = await avecSimultanéité(lot: lot, limite: 4)
            if Task.isCancelled || courante != generation { return }
            for (id, image) in resultats {
                vignettes[id] = image
                faites += 1
            }
            etat = .chargement(progression: Double(faites) / Double(max(1, liste.count)))
        }
        if courante == generation {
            etat = .pret(nombre: liste.count)
        }
    }

    private func avecSimultanéité(lot: [ImageItem], limite: Int) async -> [(String, CGImage)] {
        var resultats: [(String, CGImage)] = []
        await withTaskGroup(of: (String, CGImage?).self) { groupe in
            var enCours = 0
            var restants = lot[...]
            while enCours < limite, let item = restants.first {
                restants = restants.dropFirst()
                enCours += 1
                groupe.addTask { (item.id, VignettesService.vignette(pour: item.url, largeurCible: 180)?.image) }
            }
            while let resultat = await groupe.next() {
                if let image = resultat.1 { resultats.append((resultat.0, image)) }
                if let item = restants.first {
                    restants = restants.dropFirst()
                    groupe.addTask { (item.id, VignettesService.vignette(pour: item.url, largeurCible: 180)?.image) }
                }
            }
        }
        return resultats
    }

    // MARK: - Actions

    public func selectionner(_ item: ImageItem) {
        selection = item.id
        statut = "Selected: \(item.nom)"
        Journal.info("selection : \(item.nom)")
    }

    public func changerTri(_ nouveau: Tri) {
        tri = nouveau
        charger()
    }

    /// Applique l'image selectionnee (a l'ecran courant, ou partout selon la case).
    /// Equivalent de `_on_apply` (`mural/app.py:1868-1953`) : en monocible on
    /// n'ecrase PAS les autres ecrans.
    public func appliquer(_ item: ImageItem) {
        let cibles: [Int] = memeImagePartout
            ? Array(ecrans.indices)
            : [ecranCourant]
        var reussis = 0
        var erreurs: [String] = []
        for index in cibles {
            guard let nsEcran = EcransService.nsEcran(index: index) else { continue }
            do {
                try FondEcranService.appliquer(image: item.url, ecran: nsEcran)
                reussis += 1
                for ecran in ecrans where ecran.index == index {
                    reglages.imageParEcran[ecran.id] = item.url.path
                }
            } catch {
                erreurs.append(error.localizedDescription)
                Journal.erreur("application sur l'ecran \(index + 1) : \(error.localizedDescription)")
            }
        }
        enregistrer()
        releverFondsActifs()
        if erreurs.isEmpty {
            statut = "Applied on \(reussis) monitor(s): \(item.nom)"
            Journal.info("applique \(item.url.path) sur \(reussis) ecran(s)")
        } else {
            statut = "Application failed: " + erreurs.joined(separator: ", ")
        }
    }

    public func basculerEcran(_ index: Int) {
        ecranCourant = index
        statut = "Monitor \(index + 1) selected"
    }

    /// Favori du diaporama pour l'ecran courant (`_on_thumb_slideshow_toggle`).
    public func basculerFavori(_ item: ImageItem) {
        let cle = ecranActif?.id
        if reglages.estDansDiaporama(item.url.path, ecran: cle) {
            reglages.retirerDuDiaporama(item.url.path, ecran: cle)
            statut = "Removed from slideshow: \(item.nom)"
        } else {
            reglages.ajouterAuDiaporama(item.url.path, ecran: cle)
            statut = "Added to slideshow: \(item.nom)"
        }
        enregistrer()
    }

    public func estFavori(_ item: ImageItem) -> Bool {
        reglages.estDansDiaporama(item.url.path, ecran: ecranActif?.id)
    }

    public func estFondActif(_ item: ImageItem) -> Bool {
        fondsActifs.contains(item.url.path)
    }

    public func revelerDansFinder(_ item: ImageItem) {
        NSWorkspace.shared.activateFileViewerSelecting([item.url])
        statut = "Revealed: \(item.nom)"
    }

    public func copierChemin(_ item: ImageItem) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(item.url.path, forType: .string)
        statut = "Path copied"
    }

    /// Supprime un fichier APRES confirmation (la confirmation est presentee par
    /// l'interface) — `_on_thumb_delete_disk`, app.py:1289-1325.
    public func supprimerDuDisque(_ item: ImageItem) {
        do {
            try FileManager.default.removeItem(at: item.url)
            Journal.info("supprime du disque : \(item.url.path)")
            reglages.retirerDuDiaporama(item.url.path)
            for (ecran, chemin) in reglages.imageParEcran where chemin == item.url.path {
                reglages.imageParEcran.removeValue(forKey: ecran)
            }
            enregistrer()
            statut = "File deleted: \(item.nom)"
            charger()
        } catch {
            statut = "Deletion error: \(error.localizedDescription)"
            Journal.erreur("suppression impossible : \(error.localizedDescription)")
        }
    }

    /// Retire les fichiers de vignettes du cache disque (`_on_clear_cache`).
    public func viderCache() {
        let dossier = VignettesService.dossierCache
        let contenu = (try? FileManager.default.contentsOfDirectory(at: dossier, includingPropertiesForKeys: nil)) ?? []
        for url in contenu { try? FileManager.default.removeItem(at: url) }
        vignettes = [:]
        statut = "Cache cleared (\(contenu.count) files)"
        charger()
    }

    public func enregistrer() {
        reglages.dossier = dossier.path
        reglages.memeImagePartout = memeImagePartout
        _ = ReglagesDepot.sauvegarder(reglages)
    }

    private func releverFondsActifs() {
        var actifs = Set<String>()
        for ecran in ecrans {
            guard let nsEcran = EcransService.nsEcran(index: ecran.index),
                  let courant = FondEcranService.imageCourante(ecran: nsEcran) else { continue }
            actifs.insert(courant.path)
            Journal.info("fond actuel ecran \(ecran.index + 1) : \(courant.path)")
        }
        fondsActifs = actifs
    }
}

public extension Tri {
    init(brut: String) {
        self = brut == "date" ? .date : .nom
    }
}

extension Array {
    func chunked(taille: Int) -> [[Element]] {
        guard taille > 0 else { return [self] }
        return stride(from: 0, to: count, by: taille).map { Array(self[$0..<Swift.min($0 + taille, count)]) }
    }
}
