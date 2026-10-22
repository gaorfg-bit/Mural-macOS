import AppKit
import MuralKit
import SwiftUI

/// Interieur de Mural — reprend la composition de `mural/app.py` :
/// barre compacte en haut, grand apercu, grille serree, barre d'etat en bas.
struct VueMural: View {
    @StateObject var modele: GalerieModele
    @State private var apercu: NSImage?
    @State private var panneauOuvert = false
    @State private var aSupprimer: ImageItem?
    @State private var rechercheVisible = true
    @State private var apercuPleinEcran = false
    @FocusState private var champFocus: Bool

    var body: some View {
        VStack(spacing: 0) {
            barreOutils
            apercuVue
            Divider()
            grille
            Divider()
            barreEtat
        }
        .frame(minWidth: 760, minHeight: 480)
        .background(
            // Espace : apercu plein ecran, comme la fenetre modale de l'original.
            Button("") { if modele.imageSelectionnee != nil { apercuPleinEcran = true } }
                .keyboardShortcut(.space, modifiers: [])
                .opacity(0)
        )
        .sheet(isPresented: $apercuPleinEcran) {
            ZStack {
                Color.black.ignoresSafeArea()
                if let apercu {
                    Image(nsImage: apercu).resizable().aspectRatio(contentMode: .fit)
                }
                Text("Press Escape or Space to close")
                    .font(.caption).foregroundStyle(.white.opacity(0.7))
                    .frame(maxHeight: .infinity, alignment: .bottom).padding(16)
            }
            .frame(minWidth: 900, minHeight: 600)
            .onExitCommand { apercuPleinEcran = false }
        }
        .task(id: modele.selection) { await chargerApercu() }
        .onAppear { modele.charger() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didChangeScreenParametersNotification)) { _ in
            modele.ecrans = EcransService.lister()
            modele.statut = "Monitors updated: \(modele.ecrans.count) detected"
        }
        .alert(
            "Delete from disk?",
            isPresented: Binding(get: { aSupprimer != nil }, set: { if !$0 { aSupprimer = nil } }),
            presenting: aSupprimer
        ) { item in
            Button("Cancel", role: .cancel) { aSupprimer = nil }
            Button("Delete", role: .destructive) {
                modele.supprimerDuDisque(item)
                aSupprimer = nil
            }
        } message: { item in
            Text("« \(item.nom) » will be deleted permanently. This cannot be undone.")
        }
    }

    // MARK: - Barre d'outils

    private var barreOutils: some View {
        HStack(spacing: 8) {
            Button {
                choisirDossier()
            } label: {
                Image(systemName: "folder")
            }
            .buttonStyle(.borderless)
            .help("Choose a folder")
            .keyboardShortcut("o", modifiers: .command)

            Button {
                rechercheVisible.toggle()
                if rechercheVisible { champFocus = true }
            } label: {
                Image(systemName: "magnifyingglass")
            }
            .buttonStyle(.borderless)
            .help("Search")
            .keyboardShortcut("f", modifiers: .command)

            if rechercheVisible {
                TextField("Search…", text: $modele.recherche)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 170)
                    .focused($champFocus)
            }

            Spacer(minLength: 12)

            // Commandes d'ecran, compactes (app.py:449-472).
            if modele.ecrans.count > 1 {
                Picker("", selection: Binding(
                    get: { modele.ecranCourant },
                    set: { modele.basculerEcran($0) }
                )) {
                    ForEach(Array(modele.ecrans.enumerated()), id: \.element.id) { index, ecran in
                        Text("Monitor \(index + 1)").tag(index)
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .frame(maxWidth: 200)

                Toggle("Same image on all", isOn: $modele.memeImagePartout)
                    .toggleStyle(.checkbox)
                    .onChange(of: modele.memeImagePartout) { _ in modele.enregistrer() }
            }

            Spacer(minLength: 12)

            Menu {
                Button("Refresh gallery") { modele.charger() }
                    .keyboardShortcut("r", modifiers: .command)
                Button("Clear cache") { modele.viderCache() }
                Divider()
                Button("Sort by name") { modele.changerTri(.nom) }
                Button("Sort by date") { modele.changerTri(.date) }
                Divider()
                Button("Ouvrir le journal") {
                    if let chemin = Journal.chemin {
                        NSWorkspace.shared.activateFileViewerSelecting([chemin])
                    }
                }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .frame(width: 30)
            .help("More options")

            Button(modele.libelleAppliquer) {
                if let item = modele.imageSelectionnee { modele.appliquer(item) }
            }
            .buttonStyle(.borderedProminent)
            .disabled(modele.imageSelectionnee == nil)
            .keyboardShortcut(.defaultAction)

            Button {
                panneauOuvert.toggle()
            } label: {
                Image(systemName: "sidebar.right")
            }
            .buttonStyle(.borderless)
            .help("Sidebar")
            .popover(isPresented: $panneauOuvert, arrowEdge: .bottom) {
                PanneauLateral(modele: modele)
            }
        }
        .padding(.horizontal, 12)
        .frame(height: 46)
    }

    // MARK: - Apercu panoramique (hauteur fixe, app.py:299-317)

    private var apercuVue: some View {
        ZStack {
            Color(nsColor: .underPageBackgroundColor)
            if let apercu {
                Image(nsImage: apercu)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
            } else {
                Text("Select an image")
                    .foregroundStyle(.secondary)
            }
        }
        .frame(height: Reglages.hauteurApercu)
        .frame(maxWidth: .infinity)
    }

    // MARK: - Grille (FlowBox : 4 px d'espacement, marges 2/4, app.py:339-357)

    private var grille: some View {
        GeometryReader { geo in
            let espacement: CGFloat = 4
            let largeurUtile = max(0, geo.size.width - 4)
            let colonnes = max(2, Int((largeurUtile + espacement) / (180 + espacement)))
            let largeurCellule = (largeurUtile - espacement * CGFloat(colonnes - 1)) / CGFloat(colonnes)
            let hauteurCellule = (largeurCellule * Reglages.rapportVignette).rounded()

            ScrollView {
                LazyVGrid(
                    columns: Array(repeating: GridItem(.flexible(), spacing: espacement), count: colonnes),
                    spacing: espacement
                ) {
                    ForEach(modele.imagesFiltrees) { item in
                        cellule(item, largeur: largeurCellule, hauteur: hauteurCellule)
                    }
                }
                .padding(.horizontal, 2)
                .padding(.vertical, 4)
            }
            .overlay(alignment: .bottom) {
                if case .chargement(let progression) = modele.etat {
                    ProgressView(value: progression)
                        .progressViewStyle(.linear)
                        .frame(height: 3)
                        .padding(.horizontal, 8)
                }
            }
        }
    }

    private func cellule(_ item: ImageItem, largeur: CGFloat, hauteur: CGFloat) -> some View {
        let selectionnee = modele.selection == item.id
        return ZStack {
            if let image = modele.vignettes[item.id] {
                Image(decorative: image, scale: 1)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
            } else {
                Rectangle().fill(Color(nsColor: .quaternaryLabelColor))
            }
        }
        .frame(width: largeur, height: hauteur)
        .clipped()
        .clipShape(RoundedRectangle(cornerRadius: 6))
        .overlay {
            if selectionnee {
                // Contour 3 px a l'interieur (app.py:120-124).
                RoundedRectangle(cornerRadius: 6)
                    .strokeBorder(Color.accentColor, lineWidth: 3)
            }
        }
        .overlay(alignment: .topTrailing) {
            if modele.estFondActif(item) {
                Image(systemName: "checkmark")
                    .font(.caption2.bold())
                    .foregroundStyle(.white)
                    .padding(2)
                    .background(Circle().fill(Color.accentColor))
                    .padding(4)
            }
        }
        .overlay(alignment: .bottomLeading) {
            if modele.estFavori(item) {
                Image(systemName: "star.fill")
                    .font(.caption2)
                    .foregroundStyle(.yellow)
                    .padding(2)
                    .background(Capsule().fill(Color.black.opacity(0.6)))
                    .padding(4)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture(count: 2) { modele.appliquer(item) }
        .onTapGesture { modele.selectionner(item) }
        .contextMenu {
            Button("Set as background") { modele.appliquer(item) }
            Button("Open in Files") { modele.revelerDansFinder(item) }
            Button("Copy path") { modele.copierChemin(item) }
            Divider()
            Button("Sort by name") { modele.changerTri(.nom) }
            Button("Sort by date") { modele.changerTri(.date) }
            Divider()
            Button(modele.estFavori(item) ? "Remove from slideshow" : "Add to slideshow") {
                modele.basculerFavori(item)
            }
            Divider()
            Button("Delete from disk", role: .destructive) { aSupprimer = item }
        }
        .help(item.nom)
    }

    // MARK: - Barre d'etat (app.py:366-407)

    private var barreEtat: some View {
        HStack(spacing: 10) {
            Text(modele.statut)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            Spacer(minLength: 8)
            if let item = modele.imageSelectionnee {
                Text(item.nom)
                    .font(.system(size: 11, weight: .bold))
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
                Divider().frame(height: 12)
                Text(dimensions(item))
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                Divider().frame(height: 12)
                Text(taille(item))
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 12)
        .frame(height: 28)
    }

    private func dimensions(_ item: ImageItem) -> String {
        guard let dimensions = BibliothequeService.dimensions(de: item.url) else { return "" }
        return "\(dimensions.largeur) × \(dimensions.hauteur) px"
    }

    private func taille(_ item: ImageItem) -> String {
        let octets = Double(item.tailleOctets)
        return octets > 1_048_576
            ? String(format: "%.1f MB", octets / 1_048_576)
            : String(format: "%.0f KB", octets / 1024)
    }

    // MARK: - Aides

    private func chargerApercu() async {
        guard let item = modele.imageSelectionnee else {
            apercu = nil
            return
        }
        let chemin = item.url
        let image = await Task.detached(priority: .userInitiated) { () -> NSImage? in
            guard let vignette = VignettesService.apercu(pour: chemin, largeurMax: 1600, hauteurMax: 780) else {
                return nil
            }
            return NSImage(cgImage: vignette.image, size: NSSize(width: vignette.largeurPixels, height: vignette.hauteurPixels))
        }.value
        // La selection a pu changer pendant le decodage (`_apply_preview`).
        guard modele.selection == item.id else { return }
        apercu = image
    }

    private func choisirDossier() {
        let panneau = NSOpenPanel()
        panneau.canChooseDirectories = true
        panneau.canChooseFiles = false
        panneau.allowsMultipleSelection = false
        panneau.message = "Choose the image folder"
        guard panneau.runModal() == .OK, let url = panneau.url else { return }
        modele.charger(dossier: url)
        modele.enregistrer()
    }
}
