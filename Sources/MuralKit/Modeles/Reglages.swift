import Foundation

/// Reglages de Mural — portage de `mural/models.py` (WallpaperSettings) et
/// `mural/config.py`.
///
/// Le decodage est volontairement tolerant, comme `WallpaperSettings.from_dict`
/// (models.py:41-63) : une vieille config peut contenir une chaine la ou une
/// liste est attendue, une liste la ou un dictionnaire est attendu. On repare
/// silencieusement au lieu de laisser un bug incoherent plus loin.
///
/// Aucun `Codable` : la lecture passe par `JSONSerialization` et un mappage
/// explicite, ce qui rend la reparation evidente et testable.
public struct Reglages: Equatable {
    public var dossier: String = ""
    public var mode: String = "spanned"
    public var ecranVerrouille: Bool = false
    public var memeImagePartout: Bool = false
    public var imageParEcran: [String: String] = [:]
    public var largeurFenetre: Double = 1280
    public var hauteurFenetre: Double = 800
    public var fenetreMaximisee: Bool = false

    public var diaporamaActif: Bool = false
    public var diaporamaIntervalle: Int = 10
    public var diaporamaAleatoire: Bool = true
    public var diaporamaImages: [String] = []
    public var diaporamaImagesParEcran: [String: [String]] = [:]
    public var diaporamaEcrans: [String] = []

    public var favorisDossiers: [String] = []
    public var dossierParEcran: [String: String] = [:]

    public var avifPourFond: Bool = false
    public var derniereVersionVue: String = ""

    public init() {}

    // MARK: - Lecture tolerante

    /// Champs qui doivent etre des listes de chaines.
    private static let champsListe = ["diaporamaImages", "diaporamaEcrans", "favorisDossiers"]
    /// Champs qui doivent etre des dictionnaires.
    private static let champsDictionnaire = ["imageParEcran", "diaporamaImagesParEcran", "dossierParEcran"]

    /// Construit des reglages a partir d'un dictionnaire brut (testable sans fichier).
    public init(dictionnaire brut: [String: Any]) {
        self.init()

        if let valeur = brut["folder"] as? String { dossier = valeur }
        if let valeur = brut["mode"] as? String { mode = valeur }
        if let valeur = brut["lock_screen"] as? Bool { ecranVerrouille = valeur }
        if let valeur = brut["same_image_on_all"] as? Bool { memeImagePartout = valeur }
        if let valeur = (brut["window_width"] as? NSNumber)?.doubleValue { largeurFenetre = valeur }
        if let valeur = (brut["window_height"] as? NSNumber)?.doubleValue { hauteurFenetre = valeur }
        if let valeur = brut["window_maximized"] as? Bool { fenetreMaximisee = valeur }

        if let valeur = brut["slideshow_enabled"] as? Bool { diaporamaActif = valeur }
        if let valeur = (brut["slideshow_interval"] as? NSNumber)?.intValue { diaporamaIntervalle = valeur }
        if let valeur = brut["slideshow_random"] as? Bool { diaporamaAleatoire = valeur }
        if let valeur = brut["avif_use_for_gnome"] as? Bool { avifPourFond = valeur }
        if let valeur = brut["last_seen_version"] as? String { derniereVersionVue = valeur }

        // Listes de chaines : une chaine seule devient une liste d'un element,
        // un autre type devient une liste vide (models.py:47-53).
        for champ in ["slideshow_images", "slideshow_monitors", "folder_bookmarks"] {
            let valeurs: [String]
            switch brut[champ] {
            case let liste as [String]: valeurs = liste
            case let chaine as String: valeurs = [chaine]
            case _: valeurs = []
            }
            Self.affecterListe(champ, valeurs, à: &self)
        }

        // Dictionnaires : tout autre type devient un dictionnaire vide.
        for champ in ["per_monitor", "monitor_folders"] {
            let valeurs = brut[champ] as? [String: String] ?? [:]
            Self.affecterDictionnaire(champ, valeurs, à: &self)
        }

        // Playlists par ecran : valeurs qui doivent etre des listes
        // (une chaine seule devient une liste d'un element).
        if let brutImages = brut["slideshow_images_per_monitor"] as? [String: Any] {
            var resultat: [String: [String]] = [:]
            for (cle, valeur) in brutImages {
                switch valeur {
                case let liste as [String]: resultat[cle] = liste
                case let chaine as String: resultat[cle] = [chaine]
                default: break
                }
            }
            diaporamaImagesParEcran = resultat
        }
    }

    private static func affecterListe(_ champ: String, _ valeurs: [String], à reglages: inout Reglages) {
        switch champ {
        case "slideshow_images": reglages.diaporamaImages = valeurs
        case "slideshow_monitors": reglages.diaporamaEcrans = valeurs
        case "folder_bookmarks": reglages.favorisDossiers = valeurs
        default: break
        }
    }

    private static func affecterDictionnaire(_ champ: String, _ valeurs: [String: String], à reglages: inout Reglages) {
        switch champ {
        case "per_monitor": reglages.imageParEcran = valeurs
        case "monitor_folders": reglages.dossierParEcran = valeurs
        
        default: break
        }
    }

    /// Encode en dictionnaire compatible JSON.
    public func enDictionnaire() -> [String: Any] {
        [
            "folder": dossier,
            "mode": mode,
            "lock_screen": ecranVerrouille,
            "same_image_on_all": memeImagePartout,
            "per_monitor": imageParEcran,
            "window_width": largeurFenetre,
            "window_height": hauteurFenetre,
            "window_maximized": fenetreMaximisee,
            "slideshow_enabled": diaporamaActif,
            "slideshow_interval": diaporamaIntervalle,
            "slideshow_random": diaporamaAleatoire,
            "slideshow_images": diaporamaImages,
            "slideshow_monitors": diaporamaEcrans,
            "slideshow_images_per_monitor": diaporamaImagesParEcran,
            "folder_bookmarks": favorisDossiers,
            "monitor_folders": dossierParEcran,
            "avif_use_for_gnome": avifPourFond,
            "last_seen_version": derniereVersionVue,
        ]
    }

    // MARK: - Plafonds (config.py:32-36)

    /// `Config.MAX_IMAGES`.
    public static let limiteImages = 500
    /// `Config.THUMBNAIL_ASPECT` : hauteur = largeur x 0,611.
    public static let rapportVignette = 220.0 / 360.0
    /// `Config.PREVIEW_MAX_HEIGHT`.
    public static let hauteurApercu = 260.0
    /// Bornes de l'intervalle de diaporama (app.py:541-544).
    public static let intervalleMin = 1
    public static let intervalleMax = 1440
}
