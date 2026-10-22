import MuralKit
import SwiftUI

enum OngletPanneau: String, CaseIterable, Identifiable {
    case display = "Display"
    case slideshow = "Slideshow"
    case folders = "Folders"
    case avif = "AVIF"
    var id: String { rawValue }
}

/// Panneau lateral — reprend les QUATRE onglets de `mural/app.py:415-714`
/// (Display, Slideshow, Folders, AVIF), en surcouche : aucun panneau permanent,
/// donc aucune colonne qui mange la place des images.
struct PanneauLateral: View {
    @ObservedObject var modele: GalerieModele
    @State private var onglet: OngletPanneau = .display
    @State private var intervalle: Int = 10

    var body: some View {
        VStack(spacing: 8) {
            Picker("", selection: $onglet) {
                ForEach(OngletPanneau.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    switch onglet {
                    case .display: ongletDisplay
                    case .slideshow: ongletDiaporama
                    case .folders: ongletDossiers
                    case .avif: ongletAvif
                    }
                }
                .padding(8)
            }
        }
        .frame(width: 340, height: 430)
        .onAppear {
            intervalle = modele.reglages.diaporamaIntervalle
        }
    }

    // MARK: - Display

    private var ongletDisplay: some View {
        Group {
            SectionTitre("Monitors")
            ForEach(Array(modele.ecrans.enumerated()), id: \.element.id) { index, ecran in
                HStack(alignment: .top, spacing: 6) {
                    Image(systemName: ecran.index == modele.ecranCourant ? "largecircle.fill.circle" : "circle")
                        .foregroundStyle(ecran.index == modele.ecranCourant ? Color.accentColor : .secondary)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(ecran.etiquette).font(.system(size: 12, weight: .medium))
                        Text("\(ecran.nom) — \(ecran.largeur)×\(ecran.hauteur) — pos(\(ecran.x), \(ecran.y))")
                            .font(.system(size: 10))
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                }
                .contentShape(Rectangle())
                .onTapGesture { modele.basculerEcran(index) }
            }

            Toggle("Same image on all", isOn: $modele.memeImagePartout)
                .onChange(of: modele.memeImagePartout) { _ in modele.enregistrer() }

            SectionTitre("Options")
            // Une seule source de verite avec la case de la barre d'outils :
            // sous GNOME, ce selecteur etait sauvegarde sans jamais etre applique.
            Picker("Display mode", selection: Binding(
                get: { modele.memeImagePartout ? "zoom" : "spanned" },
                set: { valeur in
                    modele.memeImagePartout = (valeur == "zoom")
                    modele.reglages.mode = valeur
                    modele.enregistrer()
                }
            )) {
                Text("Independent Multi-Monitor (Recommended)").tag("spanned")
                Text("Same image everywhere (Zoom)").tag("zoom")
            }
            Text("Under GNOME this list was saved but never applied. Here it is real: Independent gives each screen its own image, Same image copies it to every screen.")
                .font(.system(size: 10))
                .foregroundStyle(.secondary)

            HStack(spacing: 6) {
                Image(systemName: "lock.slash")
                Text("Lock screen: indisponible — macOS n'expose aucune API publique pour le fond de l'ecran verrouille. L'option GNOME existait mais n'etait jamais appliquee.")
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
            }
        }
    }

    // MARK: - Slideshow

    private var ongletDiaporama: some View {
        Group {
            SectionTitre("Slideshow")
            Toggle("Automatic change", isOn: Binding(
                get: { modele.reglages.diaporamaActif },
                set: { modele.reglages.diaporamaActif = $0; modele.enregistrer() }
            ))
            HStack {
                Text("Interval (min)")
                Spacer()
                Stepper("\(intervalle)", value: $intervalle, in: Reglages.intervalleMin...Reglages.intervalleMax)
                    .onChange(of: intervalle) { valeur in
                        modele.reglages.diaporamaIntervalle = valeur
                        modele.enregistrer()
                    }
            }
            Toggle("Random order", isOn: Binding(
                get: { modele.reglages.diaporamaAleatoire },
                set: { modele.reglages.diaporamaAleatoire = $0; modele.enregistrer() }
            ))

            SectionTitre("Apply on")
            ForEach(modele.ecrans) { ecran in
                Toggle("Monitor \(ecran.index + 1)", isOn: Binding(
                    get: { modele.reglages.diaporamaEcrans.isEmpty || modele.reglages.diaporamaEcrans.contains(ecran.id) },
                    set: { actif in
                        var liste = modele.reglages.diaporamaEcrans
                        if actif { if !liste.contains(ecran.id) { liste.append(ecran.id) } }
                        else { liste.removeAll { $0 == ecran.id } }
                        modele.reglages.diaporamaEcrans = liste.count == modele.ecrans.count ? [] : liste
                        modele.enregistrer()
                    }
                ))
            }

            let playlist = modele.reglages.playlistDiaporama(ecran: modele.ecranActif?.id)
            Text(playlist.isEmpty ? "No image selected" : "\(playlist.count) image\(playlist.count > 1 ? "s" : "") in list")
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
            Text("Le diaporama est en mode 100 % manuel : seules les images ajoutees (menu contextuel de vignette) jouent.")
                .font(.system(size: 10))
                .foregroundStyle(.secondary)

            Button("Next image now") {
                modele.statut = "Rotation: to implement at step 7"
            }
            Button("Add current folder to slideshow") {
                let cle = modele.ecranActif?.id
                for item in modele.images { modele.reglages.ajouterAuDiaporama(item.url.path, ecran: cle) }
                modele.enregistrer()
                modele.statut = "\(modele.images.count) images added from folder"
            }
            Button("Remove current folder from slideshow") {
                let cle = modele.ecranActif?.id
                let prefixe = modele.dossier.path + "/"
                for chemin in modele.reglages.playlistDiaporama(ecran: cle) where chemin.hasPrefix(prefixe) {
                    modele.reglages.retirerDuDiaporama(chemin, ecran: cle)
                }
                modele.enregistrer()
                modele.statut = "Removed folder from slideshow"
            }
        }
    }

    // MARK: - Folders

    private var ongletDossiers: some View {
        Group {
            SectionTitre("Navigation")
            Text(modele.dossier.path)
                .font(.system(size: 11))
                .textSelection(.enabled)
            Text("\(modele.images.count) image\(modele.images.count > 1 ? "s" : "")")
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
            HStack {
                Button("Add to bookmarks") {
                    if !modele.reglages.favorisDossiers.contains(modele.dossier.path) {
                        modele.reglages.favorisDossiers.append(modele.dossier.path)
                        modele.enregistrer()
                    }
                }
                Button("Remove") {
                    modele.reglages.favorisDossiers.removeAll { $0 == modele.dossier.path }
                    modele.enregistrer()
                }
            }

            if !modele.reglages.favorisDossiers.isEmpty {
                SectionTitre("My bookmarks")
                ForEach(modele.reglages.favorisDossiers, id: \.self) { chemin in
                    Button {
                        modele.charger(dossier: URL(fileURLWithPath: chemin))
                    } label: {
                        HStack {
                            Image(systemName: "bookmark")
                            Text(URL(fileURLWithPath: chemin).lastPathComponent)
                            Spacer()
                        }
                    }
                    .buttonStyle(.borderless)
                }
            }

            SectionTitre("Monitor shortcuts")
            ForEach(modele.ecrans) { ecran in
                HStack {
                    Text("Monitor \(ecran.index + 1)")
                    Spacer()
                    Text(modele.reglages.dossierParEcran[ecran.id].map { URL(fileURLWithPath: $0).lastPathComponent } ?? "Unassigned")
                        .font(.system(size: 10))
                        .foregroundStyle(.secondary)
                    Button {
                        let panneau = NSOpenPanel()
                        panneau.canChooseDirectories = true
                        panneau.canChooseFiles = false
                        if panneau.runModal() == .OK, let url = panneau.url {
                            modele.reglages.dossierParEcran[ecran.id] = url.path
                            modele.enregistrer()
                        }
                    } label: { Image(systemName: "folder") }
                        .buttonStyle(.borderless)
                }
            }
        }
    }

    // MARK: - AVIF

    private var ongletAvif: some View {
        Group {
            SectionTitre("AVIF")
            Text("AVIF reading: available natively (ImageIO decode l'AVIF sur macOS 13+). Les fichiers .avif s'affichent donc dans la galerie, sans ImageMagick.")
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
            Text(AvifService.resumeCapacites())
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
            HStack {
                Button("Convert this folder") {}
                    .disabled(!AvifService.encodageDisponible)
                Button("Purge") { modele.viderCache() }
            }
            if !AvifService.encodageDisponible {
                Text("La conversion est desactivee : ImageIO n'expose pas d'encodeur AVIF sur ce systeme. Aucun bouton actif pour un comportement inexistant.")
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
            }
            Text("Thumbnail cache: \(VignettesService.nombreEnCache()) file(s) in \(VignettesService.dossierCache.path)")
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
        }
    }
}

private struct SectionTitre: View {
    let texte: String
    init(_ texte: String) { self.texte = texte }
    var body: some View {
        Text(texte)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(.secondary)
    }
}
