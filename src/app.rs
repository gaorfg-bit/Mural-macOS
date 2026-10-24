//! Interface AppKit de Mural — composition epuree validee le 24/10/2026.
//!
//!   apercu bord a bord redimensionnable (double-clic : image entiere)
//!   galerie opaque, sans barre d'etat permanente
//!   commandes et dossiers flottants reveles aux bords
//!   matiere Flat ou NSGlassEffectView native, selon le choix utilisateur
//!
//! Tout ce qui compte est journalise (voir `journal`) : chaque action, chaque
//! erreur Apple, et l'etat de la galerie au chargement.
//!
//! Piege AppKit a garder en tete : **`target` n'est PAS retenu** par un
//! `NSControl`. La cible est donc volontairement oubliee (`mem::forget`) et
//! vit aussi longtemps que le processus ; sans cela aucune action ne se
//! declenche.

use objc2_foundation::NSObjectNSDelayedPerforming;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use objc2::rc::Retained;
use objc2::runtime::{ProtocolObject, Sel};
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, define_class, msg_send, sel};
use objc2::{MainThreadOnly, Message};
use objc2_app_kit::NSApplicationDelegate;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSAutoresizingMaskOptions, NSBackingStoreType,
    NSBezelStyle, NSButton, NSColor, NSEvent, NSFont, NSImage, NSImageScaling, NSImageView, NSMenu,
    NSMenuItem, NSModalResponseOK, NSOpenPanel, NSPasteboard, NSPopUpButton, NSScrollView,
    NSSearchField, NSSegmentSwitchTracking, NSSegmentedControl, NSTextField, NSView, NSWindow,
    NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::NSObjectProtocol;
use objc2_foundation::{NSArray, NSObject, NSPoint, NSRect, NSSize, NSString, NSURL};
use objc2_quartz_core::CALayer;

use crate::bibliotheque::{self, Image, Tri};
use crate::journal;
use crate::interface;
use crate::reglages_panneau::Panneau;
use crate::suivi::{self, Issue};
use crate::{ecrans, fond_ecran};
#[path = "avif_panneau.rs"]
mod panneau_avif;

/// Hauteur de la bande d'apercu (`Config.PREVIEW_MAX_HEIGHT`).
pub const HAUTEUR_APERCU: f64 = 260.0;
/// Hauteur de la barre d'outils.
pub const HAUTEUR_BARRE: f64 = 46.0;
/// Hauteur de la barre d'etat.
pub const HAUTEUR_ETAT: f64 = 28.0;
/// Espacement de la grille (`column_spacing` de `Gtk.FlowBox`).
pub const ESPACEMENT: f64 = 4.0;
/// Largeur de vignette visee.
pub const LARGEUR_VIGNETTE: f64 = 180.0;

// ---------------------------------------------------------------------------
// Modele vivant
// ---------------------------------------------------------------------------

/// Etat partage entre Rust et les actions Objective-C.
pub struct Etat {
    cible: usize,
    /// Les outils hors ecran attendent toutes les images sans boucle AppKit.
    chargement_integral: bool,
    zone_vignettes: Vec<usize>,
    reception: Option<std::sync::mpsc::Receiver<(usize, Option<bibliotheque::Vignette>)>>,
    pub reglages: crate::preferences::Reglages,
    pub erreur_sauvegarde: Option<String>,
    pub diaporama_actif: bool,
    pub filtre_diaporama: bool,
    pub dossier: PathBuf,
    pub images: Vec<Image>,
    pub vignettes: Vec<Option<Retained<NSImage>>>,
    vignettes_en_attente: Vec<bool>,
    pub selection: Option<usize>,
    pub ecran_courant: usize,
    pub ecrans: Vec<ecrans::Ecran>,
    pub fonds_actifs: Vec<String>,
    pub tri: Tri,
    /// Texte de recherche (minuscules), chaine vide = pas de filtre.
    pub filtre: String,
    /// Pointeur du bouton de vignette -> index dans `images`.
    pub cellules: HashMap<usize, usize>,
    pub largeur: f64,
    pub hauteur: f64,
}

impl Etat {
    fn nouveau(dossier: PathBuf, cible: usize, chargement_integral: bool) -> Self {
        // Les anciens identifiants Swift (`nom@x,y`) ne sont remappes que vers
        // les ecrans reellement connectes, avec une correspondance exacte.
        let ecrans = ecrans::lister();
        let reglages = crate::preferences::charger_avec_ecrans(&ecrans);
        let mut etat = Etat {
            tri: if reglages.tri == "date" {
                Tri::Date
            } else {
                Tri::Nom
            },
            largeur: taille_valide(reglages.largeur_fenetre, 880.0, 2400.0, 1180.0),
            hauteur: taille_valide(reglages.hauteur_fenetre, 520.0, 1600.0, 760.0),
            reglages,
            erreur_sauvegarde: None,
            diaporama_actif: false,
            filtre_diaporama: false,
            cible,
            chargement_integral,
            zone_vignettes: Vec::new(),
            reception: None,
            dossier,
            images: Vec::new(),
            vignettes: Vec::new(),
            vignettes_en_attente: Vec::new(),
            selection: None,
            ecran_courant: 0,
            ecrans,
            fonds_actifs: Vec::new(),
            filtre: String::new(),
            cellules: HashMap::new(),
        };
        etat.ecran_courant = etat
            .ecrans
            .iter()
            .position(|e| Some(&e.cle) == etat.reglages.ecran_selectionne.as_ref())
            .unwrap_or(0);
        for e in &etat.ecrans {
            journal::info(&format!(
                "ecran {} : {} {}x{} a ({}, {}){}",
                e.index + 1,
                e.nom,
                e.largeur,
                e.hauteur,
                e.x as i64,
                e.y as i64,
                if e.principal { " principal" } else { "" }
            ));
        }
        etat.recharger();
        etat.relever_fonds_actifs();
        etat
    }

    pub fn cibles_application(&self) -> Vec<&ecrans::Ecran> {
        self.ecrans
            .iter()
            .filter(|e| self.reglages.meme_image_partout || e.index == self.ecran_courant)
            .collect()
    }

    pub fn cibles_diaporama(&self) -> Vec<&ecrans::Ecran> {
        self.ecrans
            .iter()
            .filter(|e| {
                self.reglages
                    .diaporama_ecrans
                    .as_ref()
                    .map(|cles| cles.contains(&e.cle))
                    .unwrap_or(true)
            })
            .collect()
    }

    /// Cle UUID de l'ecran selectionne dans l'interface.
    pub fn cle_ecran_courant(&self) -> Option<&str> {
        self.ecrans.get(self.ecran_courant).map(|e| e.cle.as_str())
    }

    /// Plan du prochain tick.
    ///
    /// - mode global : liste `favoris` et planificateur historique inchanges ;
    /// - mode independant : chaque ecran ne pioche que dans sa liste dediee ;
    /// - mode « image commune » : une seule liste de reference est choisie
    ///   (ecran selectionne s'il participe et a une liste non vide, sinon
    ///   premier participant eligible), puis la meme image partout.
    pub fn plan_diaporama(&self) -> Vec<(String, String)> {
        let ecrans: Vec<_> = self
            .cibles_diaporama()
            .iter()
            .map(|e| e.cle.clone())
            .collect();
        let graine = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let images_existantes = |liste: &[String]| -> Vec<String> {
            liste
                .iter()
                .filter(|p| std::path::Path::new(p).is_file())
                .cloned()
                .collect()
        };
        if !self.reglages.selection_independante {
            let images = images_existantes(&self.reglages.favoris);
            return crate::diaporama::planifier(
                &images,
                &ecrans,
                &self.reglages.image_par_ecran,
                self.reglages.meme_image_partout,
                self.reglages.diaporama_aleatoire,
                graine,
            );
        }
        let listes: BTreeMap<String, Vec<String>> = ecrans
            .iter()
            .map(|cle| {
                let images = self
                    .reglages
                    .favoris_par_ecran
                    .get(cle)
                    .map(|liste| images_existantes(liste))
                    .unwrap_or_default();
                (cle.clone(), images)
            })
            .collect();
        if self.reglages.meme_image_partout {
            let Some((cle_reference, reference)) = crate::diaporama::liste_reference(
                &listes,
                &ecrans,
                self.cle_ecran_courant(),
            ) else {
                return Vec::new();
            };
            // Le planificateur commun avance depuis sa première cible : placer
            // l'écran de référence en tête, pas seulement récupérer sa liste.
            let mut ordre = ecrans.clone();
            if let Some(position) = ordre.iter().position(|cle| cle.as_str() == cle_reference) {
                ordre.swap(0, position);
            }
            return crate::diaporama::planifier(
                reference,
                &ordre,
                &self.reglages.image_par_ecran,
                true,
                self.reglages.diaporama_aleatoire,
                graine,
            );
        }
        crate::diaporama::planifier_par_ecran(
            &listes,
            &ecrans,
            &self.reglages.image_par_ecran,
            self.reglages.diaporama_aleatoire,
            graine,
        )
    }

    /// Statut sans lecture d’image : partage par la selection et les notifications du worker.
    fn statut_bibliotheque(&self) -> String {
        if let Some(erreur) = &self.erreur_sauvegarde { return erreur.clone(); }
        let selection = self.selection.map(|index| format!("Selected: {}", self.images[index].nom))
            .unwrap_or_else(|| "Select an image".into());
        let base = format!("{} · {} slideshow image(s){}", selection,
            self.reglages.favoris_selection(self.cle_ecran_courant()).len(),
            if self.diaporama_actif { " · playing" } else { "" });
        let en_attente: Vec<_> = self.zone_vignettes.iter()
            .map(|&i| self.vignettes_en_attente[i]).collect();
        statut_vignettes(&base, &en_attente)
    }

    fn enregistrer(&mut self) {
        self.reglages.tri = if self.tri == Tri::Date { "date" } else { "nom" }.into();
        self.reglages.largeur_fenetre = self.largeur;
        self.reglages.hauteur_fenetre = self.hauteur;
        self.reglages.ecran_selectionne =
            self.ecrans.get(self.ecran_courant).map(|e| e.cle.clone());
        self.erreur_sauvegarde = crate::preferences::sauvegarder(&self.reglages)
            .err()
            .map(|e| format!("Settings could not be saved: {e}"));
        if let Some(erreur) = &self.erreur_sauvegarde {
            journal::erreur(erreur);
        }
    }

    /// Relit le dossier courant, regenere les vignettes, selectionne la premiere.
    pub fn recharger(&mut self) {
        match bibliotheque::lister(&self.dossier, self.tri) {
            Ok(images) => {
                journal::info(&format!(
                    "dossier « {} » : {} image(s)",
                    self.dossier.display(),
                    images.len()
                ));
                self.images = images;
            }
            Err(erreur) => {
                journal::erreur(&format!(
                    "lecture du dossier « {} » impossible : {erreur}",
                    self.dossier.display()
                ));
                self.images = Vec::new();
            }
        }
        self.charger_vignettes();
        self.selection = if self.images.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    fn charger_vignettes(&mut self) {
        // Detruire la reception annule le worker precedent, y compris un envoi bloque.
        self.reception = None;
        self.zone_vignettes.clear();
        self.vignettes = vec![None; self.images.len()];
        self.vignettes_en_attente = vec![false; self.images.len()];
        if self.chargement_integral {
            self.demander_vignettes((0..self.images.len()).collect());
        }
    }

    fn demander_vignettes(&mut self, indices: Vec<usize>) {
        if indices == self.zone_vignettes { return; }
        self.reception = None;
        self.zone_vignettes = indices;
        for index in 0..self.images.len() {
            if !self.zone_vignettes.contains(&index) {
                self.vignettes[index] = None;
            }
            self.vignettes_en_attente[index] = false;
        }
        let chemins: Vec<_> = self.zone_vignettes.iter().copied()
            .filter(|&i| self.vignettes[i].is_none())
            .map(|i| { self.vignettes_en_attente[i] = true; (i, self.images[i].chemin.clone()) })
            .collect();
        // Pas de file illimitee de pixels si le fil principal est occupe.
        let (envoi, reception) = std::sync::mpsc::sync_channel(2);
        self.reception = Some(reception);
        let cible = self.cible;
        std::thread::spawn(move || {
            for (index, chemin) in chemins {
                let vignette = objc2::rc::autoreleasepool(|_| {
                    bibliotheque::vignette(&chemin, LARGEUR_VIGNETTE as usize)
                });
                if envoi.send((index, vignette)).is_err() { break; }
                // La cible vit jusqu'a la fin du processus.
                unsafe {
                    let objet = &*(cible as *const NSObject);
                    let _: () = msg_send![objet, performSelectorOnMainThread: sel!(vignettesPretes:), withObject: std::ptr::null::<NSObject>(), waitUntilDone: false];
                }
            }
        });
    }

    /// Images qui sont actuellement le fond d'un ecran (lecture seule).
    fn relever_fonds_actifs(&mut self) {
        let mut actifs = Vec::new();
        for e in &self.ecrans {
            if let Some(ecran) = ecrans::retrouver_identifiant(e.identifiant)
                && let Some(chemin) = fond_ecran::image_courante(&ecran)
            {
                journal::info(&format!(
                    "fond actuel ecran {} : {}",
                    e.index + 1,
                    chemin.display()
                ));
                actifs.push(chemin.to_string_lossy().to_string());
            }
        }
        self.fonds_actifs = actifs;
    }

    pub fn est_fond_actif(&self, index: usize) -> bool {
        self.images
            .get(index)
            .map(|i| {
                self.fonds_actifs
                    .contains(&i.chemin.to_string_lossy().to_string())
            })
            .unwrap_or(false)
    }

    /// Indices des images qui passent le filtre de recherche.
    pub fn indices_affiches(&self) -> Vec<usize> {
        let selection = self
            .reglages
            .favoris_selection(self.cle_ecran_courant());
        (0..self.images.len())
            .filter(|i| {
                (self.filtre.is_empty()
                    || self.images[*i].nom.to_lowercase().contains(&self.filtre))
                    && (!self.filtre_diaporama
                        || selection.contains(&self.images[*i].chemin.to_string_lossy().into_owned()))
            })
            .collect()
    }
}

/// Mesures de la grille pour une largeur donnee : colonnes, largeur, hauteur
/// d'une cellule. Isole ici parce que la construction et la reconstruction
/// doivent donner exactement le meme resultat.
fn mesures_grille(largeur: f64) -> (usize, f64, f64) {
    let marge = 2.0;
    let largeur_utile = largeur - 2.0 * marge;
    let colonnes =
        (((largeur_utile + ESPACEMENT) / (LARGEUR_VIGNETTE + ESPACEMENT)).floor() as usize).max(2);
    let largeur_cellule = (largeur_utile - ESPACEMENT * (colonnes as f64 - 1.0)) / colonnes as f64;
    let hauteur_cellule = (largeur_cellule * bibliotheque::ASPECT_VIGNETTE).round();
    (colonnes, largeur_cellule, hauteur_cellule)
}

// ---------------------------------------------------------------------------
// Vue document « retournee » : compte du haut, comme une grille GTK
// ---------------------------------------------------------------------------

define_class!(
    #[unsafe(super(NSView))]
    #[name = "MuralVueRetournee"]
    pub struct VueRetournee;

    impl VueRetournee {
        #[unsafe(method(isFlipped))]
        fn est_retournee(&self) -> bool {
            true
        }
    }
);

impl VueRetournee {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>();
        unsafe { msg_send![this, init] }
    }
}

/// Instant logique (ms) depuis le premier usage, pour borner les confirmations.
fn maintenant_ms() -> u64 {
    static ORIGINE: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    ORIGINE
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis() as u64
}

fn taille_valide(valeur: f64, minimum: f64, maximum: f64, defaut: f64) -> f64 {
    if valeur.is_finite() {
        valeur.clamp(minimum, maximum)
    } else {
        defaut
    }
}

define_class!(
    #[unsafe(super(NSButton))]
    #[name = "MuralVignette"]
    struct BoutonVignette;
    impl BoutonVignette {
        #[unsafe(method(drawRect:))]
        fn dessiner(&self, rect: NSRect) {
            if let Some(image) = self.image() {
                objc2_app_kit::NSBezierPath::clipRect(self.bounds());
                unsafe { image.drawInRect_fromRect_operation_fraction_respectFlipped_hints(
                    interface::cadrage(image.size(), self.bounds(), false), NSRect::ZERO,
                    objc2_app_kit::NSCompositingOperation::SourceOver, 1.0, true, None); }
            } else { unsafe { let _: () = msg_send![super(self), drawRect: rect]; } }
        }
        #[unsafe(method(mouseDown:))]
        fn souris(&self, evenement: &NSEvent) {
            let _: () = unsafe { msg_send![super(self), mouseDown: evenement] };
            if evenement.clickCount() == 2 {
                if let Some(cible) = unsafe { self.target() } {
                    let _: () = unsafe { msg_send![&*cible, apercuPleinEcran: self] };
                }
            }
        }
    }
);

define_class!(
    #[unsafe(super(NSWindow))]
    #[name = "MuralApercu"]
    struct FenetreApercu;
    impl FenetreApercu {
        #[unsafe(method(keyDown:))]
        fn touche(&self, evenement: &NSEvent) {
            if evenement.keyCode() == 53 || evenement.keyCode() == 49 { self.close(); }
            else { let _: () = unsafe { msg_send![super(self), keyDown: evenement] }; }
        }
        #[unsafe(method(cancelOperation:))]
        fn annuler(&self, _expediteur: Option<&NSObject>) { self.close(); }
    }
);

// Decoration uniquement : les gestes restent traites par la vignette.
define_class!(
    #[unsafe(super(NSImageView))]
    #[name = "MuralBadge"]
    struct BadgeVignette;
    impl BadgeVignette {
        #[unsafe(method(hitTest:))]
        fn traverser(&self, _point: NSPoint) -> *mut NSView {
            std::ptr::null_mut()
        }
    }
);

// ---------------------------------------------------------------------------
// Cible des actions
// ---------------------------------------------------------------------------

pub struct CibleIvars {
    avif_panneau: RefCell<Option<panneau_avif::Panneau>>,
    avif_tache: RefCell<Option<panneau_avif::Tache>>,
    avif_sources: RefCell<Vec<PathBuf>>,
    etat: *mut Etat,
    surface: RefCell<Option<Retained<interface::Racine>>>,
    details: RefCell<Option<Retained<NSView>>>,
    apercu: *mut NSImageView,
    etat_texte: *mut NSTextField,
    nom_fichier: *mut NSTextField,
    dimensions: *mut NSTextField,
    bouton_appliquer: *mut NSButton,
    document: *mut NSView,
    defilement: *mut NSScrollView,
    recherche: *mut NSSearchField,
    menu: RefCell<Option<Retained<NSMenu>>>,
    segments: *mut NSSegmentedControl,
    fenetre: *mut NSWindow,
    favori: *mut NSButton,
    bouton_diaporama: *mut NSButton,
    bouton_suivante: *mut NSButton,
    apercu_plein: RefCell<Option<Retained<NSWindow>>>,
    minuterie: RefCell<Option<Retained<objc2_foundation::NSTimer>>>,
    suivi: RefCell<suivi::Suivi>,
    bilan: RefCell<suivi::Bilan>,
    relecture: RefCell<Option<Retained<objc2_foundation::NSTimer>>>,
    panneau: RefCell<Option<Panneau>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "MuralCible"]
    #[thread_kind = MainThreadOnly]
    #[ivars = CibleIvars]
    pub struct Cible;

    unsafe impl NSObjectProtocol for Cible {}
    unsafe impl NSApplicationDelegate for Cible {
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn rouvrir(&self, _application: &NSApplication, _visible: bool) -> bool {
            if !self.ivars().fenetre.is_null() { unsafe { &*self.ivars().fenetre }.makeKeyAndOrderFront(None); }
            true
        }
        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn rester_actif(&self, _application: &NSApplication) -> bool { false }
    }

    impl Cible {
        #[unsafe(method(cacheAvif:))]
        fn cache_avif(&self, _expediteur: &NSObject) {
            let mtm = MainThreadMarker::new().expect("AVIF panel on main thread");
            if self.ivars().avif_panneau.borrow().is_none() {
                *self.ivars().avif_panneau.borrow_mut() = Some(panneau_avif::Panneau::nouveau(mtm, self));
            }
            let panneau = self.ivars().avif_panneau.borrow();
            let panneau = panneau.as_ref().unwrap();
            if self.ivars().avif_tache.borrow().is_none() {
                let chemins = self.candidats_avif();
                panneau.preparer(&unsafe { &*self.ivars().etat }.dossier, chemins.len());
                *self.ivars().avif_sources.borrow_mut() = chemins;
            }
            panneau.fenetre.makeKeyAndOrderFront(None);
        }

        #[unsafe(method(convertirAvif:))]
        fn convertir_avif(&self, _expediteur: &NSObject) {
            if self.ivars().avif_tache.borrow().is_some() || !crate::formats::avif_encodable() { return; }
            let Some(cache) = crate::avif::dossier_cache() else { return; };
            let chemins = self.ivars().avif_sources.borrow().clone();
            if chemins.is_empty() { return; }
            // Le dossier et les chemins sont captures avant le depart du worker.
            if let Some(panneau) = self.ivars().avif_panneau.borrow().as_ref() {
                panneau.demarrer(chemins.len());
            }
            let tache = panneau_avif::lancer(chemins, cache, self as *const Self as usize);
            *self.ivars().avif_tache.borrow_mut() = Some(tache);
        }

        #[unsafe(method(annulerAvif:))]
        fn annuler_avif(&self, _expediteur: &NSObject) {
            if let Some(tache) = self.ivars().avif_tache.borrow().as_ref() {
                tache.annulation.store(true, std::sync::atomic::Ordering::Release);
                if let Some(panneau) = self.ivars().avif_panneau.borrow().as_ref() { panneau.annulation(); }
            }
        }

        #[unsafe(method(conversionAvifPrete:))]
        fn conversion_avif_prete(&self, _expediteur: Option<&NSObject>) {
            let dernier = self.ivars().avif_tache.borrow().as_ref()
                .and_then(|t| t.reception.try_iter().last().map(|e| (e, t.annulation.load(std::sync::atomic::Ordering::Acquire))));
            let Some((evenement, annulation)) = dernier else { return; };
            if let Some(panneau) = self.ivars().avif_panneau.borrow().as_ref() {
                panneau.actualiser(&evenement, annulation);
            }
            if evenement.fin {
                journal::info(&format!("AVIF conversion: processed {}/{}, new {}, reused {}, failed {}, cancelled {}", evenement.traitees, evenement.total, evenement.converties, evenement.reutilisees, evenement.echecs, evenement.annulee));
                self.ivars().avif_tache.borrow_mut().take();
            }
        }

        #[unsafe(method(ouvrirCacheAvif:))]
        fn ouvrir_cache_avif(&self, _expediteur: &NSObject) {
            if let Some(cache) = crate::avif::dossier_cache().filter(|p| p.is_dir()) {
                let url = NSURL::fileURLWithPath(&NSString::from_str(&cache.to_string_lossy()));
                NSWorkspace::sharedWorkspace().openURL(&url);
            }
        }

        /// Menu « Mural > Settings… » (⌘,) et derniere entree du menu « … ».
        /// Fenetre non modale conservee : fermer ne detruit rien, rouvrir ne
        /// cree aucun doublon.
        #[unsafe(method(ouvrirReglages:))]
        fn ouvrir_reglages(&self, _expediteur: &NSObject) {
            if self.ivars().etat.is_null() {
                return;
            }
            let mtm = MainThreadMarker::new().expect("settings on main thread");
            {
                let mut panneau = self.ivars().panneau.borrow_mut();
                if panneau.is_none() {
                    *panneau = Some(Panneau::nouveau(
                        mtm,
                        self,
                        unsafe { &*self.ivars().etat },
                    ));
                }
            }
            let panneau = self.ivars().panneau.borrow();
            let Some(panneau) = panneau.as_ref() else { return; };
            panneau.actualiser(unsafe { &*self.ivars().etat });
            panneau.fenetre.makeKeyAndOrderFront(None);
        }

        /// Instantane JSON des controles reels du panneau (usage recette).
        /// Retourne une chaine possedee : l'appelant la libere.
        #[unsafe(method(instantaneReglages:))]
        fn instantane_reglages(&self, _expediteur: &NSObject) -> *mut NSString {
            let texte = self
                .ivars()
                .panneau
                .borrow()
                .as_ref()
                .map(Panneau::instantane)
                .unwrap_or_else(|| "null".to_string());
            Retained::into_raw(NSString::from_str(&texte))
        }

        /// Choix d'ecran depuis le panneau : UUID, jamais un index visuel.
        #[unsafe(method(ecranPanneau:))]
        fn ecran_panneau(&self, expediteur: &NSPopUpButton) {
            let Some(cle) = expediteur
                .selectedItem()
                .and_then(|item| item.representedObject())
                .and_then(|objet| objet.downcast_ref::<NSString>().map(|s| s.to_string()))
            else {
                return;
            };
            let index = unsafe { &*self.ivars().etat }
                .ecrans
                .iter()
                .position(|e| e.cle == cle);
            if let Some(index) = index {
                self.choisir_ecran(index);
            }
        }

        /// Intervalle choisi dans le panneau : le sender est un NSPopUpButton,
        /// pas un NSMenuItem. On lit son tag puis on applique la valeur par le
        /// meme chemin que le menu.
        #[unsafe(method(intervallePanneau:))]
        fn intervalle_panneau(&self, expediteur: &NSPopUpButton) {
            let secondes = expediteur.selectedTag();
            if secondes > 0 {
                self.appliquer_intervalle(secondes as u64);
            }
        }

        /// Participant coche dans le panneau : l'UUID vient de la table du
        /// panneau (identite du bouton), pas de son tag.
        #[unsafe(method(participantPanneau:))]
        fn participant_panneau(&self, expediteur: &NSButton) {
            let cle = {
                let panneau = self.ivars().panneau.borrow();
                panneau
                    .as_ref()
                    .and_then(|p| p.cle_pour_bouton(expediteur))
            };
            if let Some(cle) = cle {
                self.basculer_participation(&cle);
            } else {
                journal::erreur("panneau : controle participant obsolete ignore");
            }
        }

        /// « Use current folder » pour un ecran : enregistre son dossier sans
        /// changer l'ecran courant ni appliquer de fond.
        #[unsafe(method(dossierEcranPanneau:))]
        fn dossier_ecran_panneau(&self, expediteur: &NSButton) {
            let cle = {
                let panneau = self.ivars().panneau.borrow();
                panneau
                    .as_ref()
                    .and_then(|p| p.cle_pour_bouton(expediteur))
            };
            let Some(cle) = cle else {
                journal::erreur("panneau : controle de dossier obsolete ignore");
                return;
            };
            let dossier = unsafe { &*self.ivars().etat }.dossier.clone();
            {
                let etat = unsafe { &mut *self.ivars().etat };
                etat.reglages
                    .dossier_par_ecran
                    .insert(cle.clone(), dossier.to_string_lossy().into_owned());
                etat.enregistrer();
            }
            journal::info(&format!(
                "dossier associe a l'ecran {cle} : {}",
                dossier.display()
            ));
            self.rafraichir();
        }

        /// Ouverture d'un favori depuis le panneau (sender NSPopUpButton).
        #[unsafe(method(favoriPanneau:))]
        fn favori_panneau(&self, _expediteur: &NSPopUpButton) {
            let chemin = {
                let panneau = self.ivars().panneau.borrow();
                panneau.as_ref().and_then(Panneau::favori_selectionne)
            };
            if let Some(chemin) = chemin {
                self.ouvrir_favori_chemin(PathBuf::from(chemin));
            }
        }

        /// Retrait du favori selectionne dans le panneau.
        #[unsafe(method(retirerFavoriPanneau:))]
        fn retirer_favori_panneau(&self, _expediteur: &NSButton) {
            let chemin = {
                let panneau = self.ivars().panneau.borrow();
                panneau.as_ref().and_then(Panneau::favori_selectionne)
            };
            if let Some(chemin) = chemin {
                self.retirer_favori_chemin(std::path::Path::new(&chemin));
            }
        }

        #[unsafe(method(imageDiaporama:))]
        fn image_diaporama(&self, expediteur: &NSObject) {
            let Some(chemin) = self.chemin_action(expediteur) else { return; };
            let cle = unsafe { &*self.ivars().etat }.cle_ecran_courant().map(str::to_owned);
            {
                let etat = unsafe { &mut *self.ivars().etat };
                let chemin = chemin.to_string_lossy().into_owned();
                etat.reglages.basculer_favori(&chemin, cle.as_deref());
                etat.enregistrer();
            }
            self.selection_diaporama_modifiee();
        }

        #[unsafe(method(ajouterDossierDiaporama:))]
        fn ajouter_dossier_diaporama(&self, _expediteur: &NSObject) {
            let cle = unsafe { &*self.ivars().etat }.cle_ecran_courant().map(str::to_owned);
            {
                let etat = unsafe { &mut *self.ivars().etat };
                for image in &etat.images {
                    let chemin = image.chemin.to_string_lossy().into_owned();
                    etat.reglages.ajouter_favori(&chemin, cle.as_deref());
                }
                etat.enregistrer();
            }
            self.selection_diaporama_modifiee();
        }

        #[unsafe(method(retirerDossierDiaporama:))]
        fn retirer_dossier_diaporama(&self, _expediteur: &NSObject) {
            let cle = unsafe { &*self.ivars().etat }.cle_ecran_courant().map(str::to_owned);
            {
                let etat = unsafe { &mut *self.ivars().etat };
                let dossier = etat.dossier.clone();
                etat.reglages.retirer_dossier_favori(&dossier, cle.as_deref());
                etat.enregistrer();
            }
            self.selection_diaporama_modifiee();
        }

        #[unsafe(method(filtrerDiaporama:))]
        fn filtrer_diaporama(&self, _expediteur: &NSObject) {
            let etat = unsafe { &mut *self.ivars().etat };
            etat.filtre_diaporama = !etat.filtre_diaporama;
            self.rafraichir_grille(); self.rafraichir_selection();
            self.rafraichir_panneau();
        }

        #[unsafe(method(basculerDiaporama:))]
        fn basculer_diaporama(&self, _expediteur: &NSObject) {
            if unsafe { &*self.ivars().etat }.diaporama_actif {
                self.arreter_diaporama(); self.rafraichir(); self.message("Slideshow paused");
            } else {
                if unsafe { &*self.ivars().etat }.plan_diaporama().is_empty() {
                    self.message("Choose at least one image and one display for the slideshow"); return;
                }
                self.programmer_diaporama(); self.rafraichir();
                self.message("Slideshow started — first change at the end of the interval");
            }
        }

        #[unsafe(method(intervalleDiaporama:))]
        fn intervalle_diaporama(&self, expediteur: &NSMenuItem) {
            self.appliquer_intervalle(expediteur.tag().clamp(60, 86400) as u64);
        }

        #[unsafe(method(aleatoireDiaporama:))]
        fn aleatoire_diaporama(&self, _expediteur: &NSObject) {
            let etat = unsafe { &mut *self.ivars().etat };
            etat.reglages.diaporama_aleatoire = !etat.reglages.diaporama_aleatoire;
            etat.enregistrer(); self.rafraichir();
        }

        #[unsafe(method(ecranDiaporama:))]
        fn ecran_diaporama(&self, expediteur: &NSMenuItem) {
            let Some(objet) = expediteur.representedObject() else { return; };
            let Some(cle) = objet.downcast_ref::<NSString>().map(|s| s.to_string()) else { return; };
            self.basculer_participation(&cle);
        }

        #[unsafe(method(suivanteDiaporama:))]
        fn suivante_diaporama(&self, _expediteur: &NSObject) { self.avancer_diaporama(); }

        /// Relecture differee des fonds demandes (voir `suivi`).
        #[unsafe(method(relireDemandes:))]
        fn relire_demandes_minuterie(&self, _minuterie: &objc2_foundation::NSTimer) {
            self.relire_demandes();
        }

        #[unsafe(method(afficherFenetre:))]
        fn afficher_fenetre(&self, _expediteur: &NSObject) {
            if !self.ivars().fenetre.is_null() { unsafe { &*self.ivars().fenetre }.makeKeyAndOrderFront(None); }
        }
        #[unsafe(method(enregistrerEtat:))]
        fn enregistrer_etat(&self, notification: &objc2_foundation::NSNotification) {
            if !self.ivars().etat.is_null() { unsafe { &mut *self.ivars().etat }.enregistrer(); }
            if &*notification.name() == unsafe { objc2_app_kit::NSApplicationWillTerminateNotification } { self.arreter_diaporama(); self.arreter_relecture(); }
            if &*notification.name() == unsafe { objc2_app_kit::NSWindowWillCloseNotification } {
                if let Some(fenetre) = self.ivars().apercu_plein.borrow_mut().take() { fenetre.close(); }

            }
        }

        #[unsafe(method(dossierFavori:))]
        fn dossier_favori(&self, _expediteur: &NSObject) {
            {
                let etat = unsafe { &mut *self.ivars().etat };
                etat.reglages.basculer_dossier_favori(&etat.dossier);
                etat.enregistrer();
            }
            self.rafraichir();
        }

        #[unsafe(method(dossiersFavoris:))]
        fn dossiers_favoris(&self, expediteur: &NSButton) {
            let menu = self.menu_dossiers();
            menu.popUpMenuPositioningItem_atLocation_inView(None, NSPoint { x: 0.0, y: expediteur.bounds().size.height }, Some(expediteur));
        }

        #[unsafe(method(ouvrirFavori:))]
        fn ouvrir_favori(&self, expediteur: &NSMenuItem) {
            if let Some(chemin) = chemin_represente(expediteur) {
                self.ouvrir_favori_chemin(chemin);
            }
        }

        #[unsafe(method(retirerFavori:))]
        fn retirer_favori(&self, expediteur: &NSMenuItem) {
            if let Some(chemin) = chemin_represente(expediteur) {
                self.retirer_favori_chemin(&chemin);
            }
        }

        #[unsafe(method(memeImagePartout:))]
        fn meme_image_partout(&self, _expediteur: &NSObject) {
            {
                let etat = unsafe { &mut *self.ivars().etat };
                etat.reglages.meme_image_partout = !etat.reglages.meme_image_partout;
                etat.enregistrer();
            }
            self.rafraichir();
        }

        /// Bascule la selection independante par ecran. Aucune liste n'est
        /// supprimee : le mode global et les listes dediees coexistent.
        #[unsafe(method(selectionIndependante:))]
        fn selection_independante(&self, _expediteur: &NSObject) {
            {
                let etat = unsafe { &mut *self.ivars().etat };
                etat.reglages.selection_independante = !etat.reglages.selection_independante;
                etat.enregistrer();
            }
            self.selection_diaporama_modifiee();
        }

        #[unsafe(method(revelerImage:))]
        fn reveler_image(&self, expediteur: &NSObject) {
            if let Some(chemin) = self.chemin_action(expediteur) {
                let url = NSURL::fileURLWithPath(&NSString::from_str(&chemin.to_string_lossy()));
                NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[url]));
            }
        }

        #[unsafe(method(copierChemin:))]
        fn copier_chemin(&self, expediteur: &NSObject) {
            if let Some(chemin) = self.chemin_action(expediteur) {
                let presse_papiers = NSPasteboard::generalPasteboard();
                presse_papiers.clearContents();
                let ok = presse_papiers.setString_forType(&NSString::from_str(&chemin.to_string_lossy()), unsafe { objc2_app_kit::NSPasteboardTypeString });
                self.message(if ok { "Path copied" } else { "Could not copy the path" });
            }
        }

        #[unsafe(method(apercuPleinEcran:))]
        fn apercu_plein_ecran(&self, expediteur: &NSObject) {
            let Some(chemin) = self.chemin_action(expediteur) else { return; };
            let Some(mtm) = MainThreadMarker::new() else { return; };
            let principale = unsafe { &*self.ivars().fenetre };
            let Some(ecran) = principale.screen() else { return; };
            let cadre = ecran.frame();
            let echelle = ecran.backingScaleFactor();
            let Some(vignette) = bibliotheque::apercu(&chemin, (cadre.size.width * echelle).min(5120.0) as u32, (cadre.size.height * echelle).min(5120.0) as u32) else {
                self.message("Could not preview this image"); return;
            };
            if let Some(ancienne) = self.ivars().apercu_plein.borrow_mut().take() { ancienne.close(); }
            let fenetre: Retained<FenetreApercu> = unsafe { msg_send![mtm.alloc::<FenetreApercu>(), initWithContentRect: cadre, styleMask: NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Resizable, backing: NSBackingStoreType::Buffered, defer: false] };
            unsafe { fenetre.setReleasedWhenClosed(false); }
            fenetre.setTitle(&NSString::from_str(&format!("{} — Esc to close", chemin.file_name().unwrap_or_default().to_string_lossy())));
            let image = NSImage::initWithCGImage_size(NSImage::alloc(), &vignette.image, NSSize { width: vignette.largeur as f64, height: vignette.hauteur as f64 });
            let vue = NSImageView::new(mtm);
            vue.setImage(Some(&image));
            vue.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
            fenetre.setContentView(Some(&vue));
            fenetre.makeKeyAndOrderFront(None);
            fenetre.toggleFullScreen(None);
            *self.ivars().apercu_plein.borrow_mut() = Some(Retained::into_super(fenetre));
        }
        #[unsafe(method(ecransModifies:))]
        fn ecrans_modifies(&self, _notification: &objc2_foundation::NSNotification) {
            let ivars = self.ivars();
            if ivars.etat.is_null() || ivars.segments.is_null() { return; }
            let etat = unsafe { &mut *ivars.etat };
            let id = etat.ecrans.get(etat.ecran_courant).map(|e| e.identifiant);
            etat.ecrans = ecrans::lister();
            etat.ecran_courant = etat.ecrans.iter().position(|e| Some(e.identifiant) == id).unwrap_or(0);
            let segments = unsafe { &*ivars.segments };
            segments.setSegmentCount(etat.ecrans.len() as isize);
            for (index, ecran) in etat.ecrans.iter().enumerate() {
                segments.setLabel_forSegment(&NSString::from_str(&format!("Display {}", index + 1)), index as isize);
                segments.setToolTip_forSegment(Some(&NSString::from_str(&ecran.nom)), index as isize);
            }
            segments.setSelectedSegment(if etat.ecrans.is_empty() { -1 } else { etat.ecran_courant as isize });
            let mut cadre = segments.frame();
            cadre.size.width = 168.0;
            cadre.origin.x = 164.0;
            segments.setFrame(cadre);
            etat.relever_fonds_actifs();
            if etat.plan_diaporama().is_empty() && etat.diaporama_actif { self.arreter_diaporama(); }
            journal::info(&format!("configuration actualisee : {} ecran(s), selection {}", etat.ecrans.len(), etat.ecran_courant + 1));
            if let Some(dossier) = etat.ecrans.get(etat.ecran_courant).and_then(|e| etat.reglages.dossier_ecran(&e.cle)) {
                if dossier != etat.dossier { self.charger_dossier(dossier); return; }
            }
            self.rafraichir();
        }

        #[unsafe(method(fenetreRedimensionnee:))]
        fn fenetre_redimensionnee(&self, _notification: &objc2_foundation::NSNotification) {
            let ivars = self.ivars();
            if ivars.etat.is_null() || ivars.defilement.is_null() { return; }
            let etat = unsafe { &mut *ivars.etat };
            let defilement = unsafe { &*ivars.defilement };
            etat.largeur = defilement.frame().size.width;
            if !ivars.fenetre.is_null() {
                if let Some(contenu) = unsafe { &*ivars.fenetre }.contentView() {
                    etat.hauteur = contenu.bounds().size.height;

                }
            }
            // Regrouper les notifications du redimensionnement avant de recréer les boutons.
            unsafe {
                NSObject::cancelPreviousPerformRequestsWithTarget_selector_object(self, sel!(redessinerGrille:), None);
                self.performSelector_withObject_afterDelay(sel!(redessinerGrille:), None, 0.08);
            }
        }

        #[unsafe(method(revelerCommandes:))]
        fn reveler_commandes(&self, _expediteur: Option<&NSObject>) {
            if let Some(surface) = self.ivars().surface.borrow().as_ref() { surface.reveler(); }
        }
        #[unsafe(method(afficherDetails:))]
        fn afficher_details(&self, _expediteur: Option<&NSObject>) {
            if let Some(vue) = self.ivars().details.borrow().as_ref() { vue.setHidden(!vue.isHidden()); }
        }

        #[unsafe(method(basculerMatiere:))]
        fn basculer_matiere(&self, expediteur: &NSMenuItem) {
            let etat = unsafe { &mut *self.ivars().etat };
            let verre = expediteur.tag() == 1;
            etat.reglages.champs_inconnus.insert("interface_verre".into(), serde_json::json!(verre));
            etat.enregistrer();
            if let Some(surface) = self.ivars().surface.borrow().as_ref() { surface.matiere(verre); }
            self.actualiser_menu();
        }

        #[unsafe(method(ciblerRecherche:))]
        fn cibler_recherche(&self, _expediteur: Option<&NSObject>) {
            let ivars = self.ivars();
            if ivars.recherche.is_null() || ivars.fenetre.is_null() { return; }
            if let Some(surface) = ivars.surface.borrow().as_ref() { surface.reveler(); }
            let champ = unsafe { &*ivars.recherche };
            unsafe { &*ivars.fenetre }.makeKeyAndOrderFront(None);
            unsafe { champ.selectText(None); }
        }

        #[unsafe(method(revelerJournal:))]
        fn reveler_journal(&self, _expediteur: Option<&NSObject>) {
            let Some(chemin) = journal::chemin().filter(|p| p.is_file()) else {
                self.message("Log file unavailable");
                return;
            };
            let url = NSURL::fileURLWithPath(&NSString::from_str(&chemin.to_string_lossy()));
            NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[url]));
        }

        #[unsafe(method(redessinerGrille:))]
        fn redessiner_grille(&self, _objet: Option<&NSObject>) {
            self.rafraichir_grille();
            self.rafraichir_selection();
        }

        #[unsafe(method(zoneVisibleModifiee:))]
        fn zone_visible_modifiee(&self, _objet: Option<&NSObject>) {
            self.charger_zone_visible();
        }

        #[unsafe(method(vignettesPretes:))]
        fn vignettes_pretes(&self, _objet: Option<&NSObject>) {
            {
                let etat = unsafe { &mut *self.ivars().etat };
                if let Some(reception) = &etat.reception {
                    while let Ok((index, vignette)) = reception.try_recv() {
                        etat.vignettes_en_attente[index] = false;
                        etat.vignettes[index] = vignette.map(|v| NSImage::initWithCGImage_size(NSImage::alloc(), &v.image, NSSize { width: v.largeur as f64, height: v.hauteur as f64 }));
                    }
                }
            }
            // Les boutons restent en place : pas de reconstruction de toute
            // la grille a chaque vignette et pas de perte de selection.
            let etat = unsafe { &*self.ivars().etat };
            for (&adresse, &index) in &etat.cellules {
                let bouton = unsafe { &*(adresse as *const NSButton) };
                bouton.setImage(etat.vignettes[index].as_deref());
                bouton.setImageScaling(NSImageScaling::ScaleAxesIndependently);
                bouton.setTitle(&NSString::from_str(if etat.vignettes[index].is_some() { "" } else if etat.vignettes_en_attente[index] { "…" } else { "(unreadable)" }));
            }
            unsafe { &*self.ivars().etat_texte }.setStringValue(&NSString::from_str(&etat.statut_bibliotheque()));
        }

        /// Clic sur une vignette : selection, apercu, informations.
        #[unsafe(method(celluleCliquee:))]
        fn cellule_cliquee(&self, expediteur: &NSButton) {
            let ivars = self.ivars();
            let etat = unsafe { &mut *ivars.etat };
            let cle = expediteur as *const NSButton as usize;
            let Some(&index) = etat.cellules.get(&cle) else {
                journal::erreur("clic sur une cellule inconnue (carte vide)");
                return;
            };
            etat.selection = Some(index);
            journal::info(&format!("selection : {}", etat.images[index].nom));
            self.rafraichir();
        }

        /// Bouton principal : applique l'image selectionnee a l'ecran courant.
        #[unsafe(method(appliquer:))]
        fn appliquer(&self, expediteur: &NSObject) {
            if self.chemin_action(expediteur).is_none() { return; }
            let ivars = self.ivars();
            let etat = unsafe { &mut *ivars.etat };
            let Some(index) = etat.selection else {
                journal::erreur("appliquer sans image selectionnee");
                return;
            };
            let chemin = etat.images[index].chemin.clone();
            let cible = etat.ecran_courant;
            journal::info(&format!(
                "application de « {} » sur l'ecran {}",
                chemin.display(),
                cible + 1
            ));
            let mut appliques = 0usize;
            let mut erreurs = Vec::new();
            let cibles: Vec<_> = etat.cibles_application().into_iter().cloned().collect();
            if cibles.is_empty() { journal::erreur("aucun ecran disponible"); return; }
            for e in cibles {
                let sequence = self.enregistrer_intention(&e.cle, &chemin);
                match ecrans::retrouver_identifiant(e.identifiant) {
                    Some(ecran) => match fond_ecran::appliquer(&chemin, &ecran) {
                        Ok(()) => {
                            appliques += 1;
                            self.assurer_relecture();
                            journal::info(&format!("ecran {} : demande acceptee par macOS (confirmation differee)", e.index + 1));
                        }
                        Err(erreur) => {
                            // L'intention est enregistree : l'echec ferme la
                            // demande, et l'ancienne ne peut plus confirmer.
                            self.ivars().suivi.borrow_mut().echouer(
                                &e.cle,
                                sequence,
                                &erreur.to_string(),
                            );
                            let texte = format!("Display {}: {erreur}", e.index + 1);
                            journal::erreur(&texte);
                            self.ivars().bilan.borrow_mut().echouer(&e.cle, texte.clone());
                            erreurs.push(texte);
                        }
                    },
                    None => {
                        self.ivars().suivi.borrow_mut().ecran_disparu(&e.cle, sequence);
                        journal::erreur(&format!("ecran {} introuvable", e.index + 1));
                        let texte = format!("Display {} not found", e.index + 1);
                        self.ivars().bilan.borrow_mut().echouer(&e.cle, texte.clone());
                        erreurs.push(texte);
                    }
                }
            }
            etat.enregistrer();
            etat.relever_fonds_actifs();
            self.rafraichir();
            if self.ivars().suivi.borrow().est_vide() {
                self.arreter_relecture();
                if let Some(texte) = self.prendre_bilan() {
                    self.message(&texte);
                }
            } else if erreurs.is_empty() {
                self.message(&format!(
                    "Request accepted on {appliques} display(s), checking: {}",
                    etat.images[index].nom
                ));
            } else {
                self.message(&format!(
                    "Request accepted on {appliques} display(s), checking; failed: {}",
                    erreurs.join(", ")
                ));
            }
        }

        /// Bouton dossier : choisit un dossier et recharge la grille.
        #[unsafe(method(ouvrirDossier:))]
        fn ouvrir_dossier(&self, _expediteur: &NSObject) {
            let Some(mtm) = MainThreadMarker::new() else {
                journal::erreur("action dossier hors du fil principal");
                return;
            };
            let panneau = NSOpenPanel::openPanel(mtm);
            unsafe {
                panneau.setCanChooseDirectories(true);
                panneau.setCanChooseFiles(false);
                panneau.setAllowsMultipleSelection(false);
                panneau.setMessage(Some(&NSString::from_str("Choose an image folder")));
            }
            if unsafe { panneau.runModal() } != NSModalResponseOK {
                journal::info("choix de dossier annule");
                return;
            }
            let urls = unsafe { panneau.URLs() };
            if urls.count() == 0 {
                journal::erreur("aucun dossier renvoye par le panneau");
                return;
            }
            let url = unsafe { urls.objectAtIndex(0) };
            let Some(chemin) = (unsafe { url.path() }) else {
                journal::erreur("dossier selectionne sans chemin lisible");
                return;
            };
            self.charger_dossier(PathBuf::from(chemin.to_string()));
        }

        /// Menu « Actualiser ».
        #[unsafe(method(actualiser:))]
        fn actualiser(&self, _expediteur: &NSObject) {
            let ivars = self.ivars();
            let etat = unsafe { &mut *ivars.etat };
            let dossier = etat.dossier.clone();
            journal::info("commande : actualiser");
            self.charger_dossier(dossier);
        }

        /// Menu « Trier par nom ».
        #[unsafe(method(trierNom:))]
        fn trier_nom(&self, _expediteur: &NSObject) {
            let ivars = self.ivars();
            let etat = unsafe { &mut *ivars.etat };
            etat.tri = Tri::Nom;
            let dossier = etat.dossier.clone();
            journal::info("commande : tri par nom");
            self.charger_dossier(dossier);
        }

        /// Menu « Trier par date ».
        #[unsafe(method(trierDate:))]
        fn trier_date(&self, _expediteur: &NSObject) {
            let ivars = self.ivars();
            let etat = unsafe { &mut *ivars.etat };
            etat.tri = Tri::Date;
            let dossier = etat.dossier.clone();
            journal::info("commande : tri par date");
            self.charger_dossier(dossier);
        }

        /// Champ de recherche (valide par Entree).
        #[unsafe(method(rechercher:))]
        fn rechercher(&self, expediteur: &NSSearchField) {
            let ivars = self.ivars();
            let etat = unsafe { &mut *ivars.etat };
            let texte = unsafe { expediteur.stringValue() }.to_string().to_lowercase();
            journal::info(&format!("recherche : « {texte} »"));
            etat.filtre = texte;
            self.rafraichir_grille();
            self.rafraichir_selection();
        }

        /// Menu « ... » : deroule les commandes secondaires.
        #[unsafe(method(menuOuvrir:))]
        fn menu_ouvrir(&self, expediteur: &NSButton) {
            let ivars = self.ivars();
            let Some(menu) = ivars.menu.borrow().clone() else {
                journal::erreur("menu demande alors qu'il n'existe pas");
                return;
            };
            self.actualiser_menu();
            let cadre = unsafe { expediteur.bounds() };
            unsafe {
                menu.popUpMenuPositioningItem_atLocation_inView(
                    None,
                    NSPoint { x: 0.0, y: cadre.size.height },
                    Some(expediteur),
                );
            }
        }

        /// Changement d'ecran courant.
        #[unsafe(method(ecranChange:))]
        fn ecran_change(&self, expediteur: &NSSegmentedControl) {
            let choisi = unsafe { expediteur.selectedSegment() };
            if choisi < 0 { return; }
            self.choisir_ecran(choisi as usize);
        }
    }
);

impl Cible {
    /// Change l'ecran courant (barre principale ou panneau) et suit le dossier
    /// associe a cet ecran. L'index est valide ici ; les actions venues du
    /// panneau ont deja resolu leur UUID.
    fn choisir_ecran(&self, index: usize) {
        let ivars = self.ivars();
        let etat = unsafe { &mut *ivars.etat };
        if index >= etat.ecrans.len() {
            return;
        }
        etat.ecran_courant = index;
        let cle = etat.ecrans[index].cle.clone();
        let dossier = etat.reglages.dossier_ecran(&cle);
        etat.enregistrer();
        journal::info(&format!("ecran courant : {}", index + 1));
        if let Some(dossier) = dossier {
            if dossier != etat.dossier {
                self.charger_dossier(dossier);
                return;
            }
        }
        self.rafraichir();
    }

    /// Applique un intervalle (menu ou panneau) sans dupliquer la logique.
    fn appliquer_intervalle(&self, secondes: u64) {
        {
            let etat = unsafe { &mut *self.ivars().etat };
            etat.reglages.diaporama_intervalle = secondes.clamp(60, 86400);
            etat.enregistrer();
        }
        if unsafe { &*self.ivars().etat }.diaporama_actif {
            self.programmer_diaporama();
        }
        self.rafraichir();
    }

    /// Coche/decoche un ecran participant au diaporama (cle UUID).
    fn basculer_participation(&self, cle: &str) {
        {
            let etat = unsafe { &mut *self.ivars().etat };
            let cles = etat
                .reglages
                .diaporama_ecrans
                .get_or_insert_with(|| etat.ecrans.iter().map(|e| e.cle.clone()).collect());
            if cles.iter().any(|c| c == cle) {
                cles.retain(|c| c != cle);
            } else {
                cles.push(cle.to_owned());
            }
            etat.enregistrer();
        }
        self.selection_diaporama_modifiee();
    }

    /// Rafraichit le panneau seul, sans reconstruire la galerie.
    fn rafraichir_panneau(&self) {
        if self.ivars().etat.is_null() {
            return;
        }
        if let Some(panneau) = self.ivars().panneau.borrow().as_ref() {
            panneau.actualiser(unsafe { &*self.ivars().etat });
        }
    }

    fn ouvrir_favori_chemin(&self, chemin: PathBuf) {
        if chemin.is_dir() {
            self.charger_dossier(chemin);
        } else {
            self.message(
                "This folder is no longer accessible. Remove it from favorites using the menu.",
            );
        }
    }

    fn retirer_favori_chemin(&self, chemin: &std::path::Path) {
        {
            let etat = unsafe { &mut *self.ivars().etat };
            etat.reglages
                .dossiers_favoris
                .retain(|p| p != &chemin.to_string_lossy());
            etat.enregistrer();
        }
        self.rafraichir();
    }

    fn arreter_diaporama(&self) {
        if let Some(minuterie) = self.ivars().minuterie.borrow_mut().take() {
            minuterie.invalidate();
        }
        unsafe { &mut *self.ivars().etat }.diaporama_actif = false;
    }

    /// Enregistre l'intention **avant** l'appel Apple et rend son numero de
    /// sequence. Une nouvelle intention pour cet ecran annule le resultat
    /// precedent dans le bilan du cycle courant : elle remplace l'ancienne,
    /// qu'elle soit acceptee ou non.
    fn enregistrer_intention(&self, cle: &str, chemin: &std::path::Path) -> u64 {
        self.ivars().bilan.borrow_mut().oublier(cle);
        self.ivars()
            .suivi
            .borrow_mut()
            .demander(cle, chemin, maintenant_ms())
    }

    /// Consomme le bilan du cycle courant (vide apres lecture).
    fn prendre_bilan(&self) -> Option<String> {
        let mut bilan = self.ivars().bilan.borrow_mut();
        let resume = bilan.resume();
        bilan.vider();
        resume
    }

    /// Demarre le relevé periodique (250 ms) s'il ne tourne pas deja.
    ///
    /// Aucune attente ni sommeil : le fil principal rend la main entre deux
    /// relectures, et la minuterie s'arrete des qu'il n'y a plus rien a suivre.
    fn assurer_relecture(&self) {
        if self.ivars().relecture.borrow().is_some() {
            return;
        }
        let periode = suivi::PERIODE_RELECTURE_MS as f64 / 1000.0;
        let minuterie = unsafe {
            objc2_foundation::NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                periode,
                self,
                sel!(relireDemandes:),
                None,
                true,
            )
        };
        unsafe {
            objc2_foundation::NSRunLoop::mainRunLoop()
                .addTimer_forMode(&minuterie, objc2_foundation::NSRunLoopCommonModes);
        }
        *self.ivars().relecture.borrow_mut() = Some(minuterie);
    }

    fn arreter_relecture(&self) {
        if let Some(minuterie) = self.ivars().relecture.borrow_mut().take() {
            minuterie.invalidate();
        }
    }

    /// Relit les fonds sans bloquer : confirme, expire ou abandonne chaque demande.
    fn relire_demandes(&self) {
        let maintenant = maintenant_ms();
        let demandes: Vec<(String, u64, PathBuf)> = self
            .ivars()
            .suivi
            .borrow()
            .demandes()
            .map(|d| (d.cle.clone(), d.sequence, d.chemin.clone()))
            .collect();
        let mut confirmees_tick = 0usize;
        for (cle, sequence, chemin) in demandes {
            let (libelle, ecran) = match unsafe { &*self.ivars().etat }
                .ecrans
                .iter()
                .find(|e| e.cle == cle)
            {
                Some(e) => (
                    format!("Display {}", e.index + 1),
                    ecrans::retrouver_identifiant(e.identifiant),
                ),
                None => (format!("Display {cle}"), None),
            };
            let issue = {
                let mut suivi = self.ivars().suivi.borrow_mut();
                match ecran {
                    Some(ecran) => {
                        let constat = fond_ecran::image_courante(&ecran);
                        suivi.relire(&cle, sequence, constat.as_deref(), maintenant)
                    }
                    None => suivi.ecran_disparu(&cle, sequence),
                }
            };
            match issue {
                Issue::Confirmee => {
                    confirmees_tick += 1;
                    unsafe { &mut *self.ivars().etat }
                        .reglages
                        .image_par_ecran
                        .insert(cle.clone(), chemin.to_string_lossy().into_owned());
                    self.ivars().bilan.borrow_mut().confirmer(&cle, chemin);
                    journal::info(&format!(
                        "{libelle} : chemin confirme par relecture (rendu non verifie)"
                    ));
                }
                Issue::DelaiDepasse => {
                    journal::erreur(&format!(
                        "{libelle} : changement non confirme apres {} ms",
                        suivi::DELAI_CONFIRMATION_MS
                    ));
                    self.ivars().bilan.borrow_mut().echouer(
                        &cle,
                        format!(
                            "{libelle} not confirmed after {} s",
                            suivi::DELAI_CONFIRMATION_MS / 1000
                        ),
                    );
                }
                Issue::EcranDisparu => {
                    journal::erreur(&format!("{libelle} : deconnecte avant confirmation"));
                    self.ivars()
                        .bilan
                        .borrow_mut()
                        .echouer(&cle, format!("{libelle} disconnected"));
                }
                Issue::EchecApi(message) => {
                    journal::erreur(&format!("{libelle} : {message}"));
                    self.ivars()
                        .bilan
                        .borrow_mut()
                        .echouer(&cle, format!("{libelle} : {message}"));
                }
                _ => {}
            }
        }
        if confirmees_tick > 0 {
            unsafe { &mut *self.ivars().etat }.enregistrer();
        }
        if self.ivars().suivi.borrow().est_vide() {
            self.arreter_relecture();
            unsafe { &mut *self.ivars().etat }.relever_fonds_actifs();
            self.rafraichir();
            if let Some(texte) = self.prendre_bilan() {
                self.message(&texte);
            }
        }
    }

    fn programmer_diaporama(&self) {
        self.arreter_diaporama();
        let intervalle = unsafe { &*self.ivars().etat }
            .reglages
            .diaporama_intervalle
            .clamp(60, 86400) as f64;
        let minuterie = unsafe {
            objc2_foundation::NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                intervalle,
                self,
                sel!(suivanteDiaporama:),
                None,
                true,
            )
        };
        minuterie.setTolerance((intervalle * 0.05).min(5.0));
        unsafe {
            objc2_foundation::NSRunLoop::mainRunLoop()
                .addTimer_forMode(&minuterie, objc2_foundation::NSRunLoopCommonModes);
        }
        *self.ivars().minuterie.borrow_mut() = Some(minuterie);
        unsafe { &mut *self.ivars().etat }.diaporama_actif = true;
    }

    fn selection_diaporama_modifiee(&self) {
        let vide = unsafe { &*self.ivars().etat }.plan_diaporama().is_empty();
        if vide && unsafe { &*self.ivars().etat }.diaporama_actif {
            self.arreter_diaporama();
        }
        self.rafraichir_grille();
        self.rafraichir();
    }

    fn avancer_diaporama(&self) {
        // Relever les fonds actuels avant de calculer la suite, y compris les
        // changements faits par l’utilisateur dans les reglages macOS.
        let etat = unsafe { &mut *self.ivars().etat };
        for ecran in &etat.ecrans {
            if let Some(ecran_vivant) = ecrans::retrouver_identifiant(ecran.identifiant) {
                if let Some(chemin) = fond_ecran::image_courante(&ecran_vivant) {
                    etat.reglages
                        .image_par_ecran
                        .insert(ecran.cle.clone(), chemin.to_string_lossy().into_owned());
                }
            }
        }
        let plan = etat.plan_diaporama();
        if plan.is_empty() {
            self.arreter_diaporama();
            self.rafraichir();
            self.message("Slideshow stopped: no accessible images or participating displays");
            return;
        }
        let mut appliques = 0;
        let mut erreurs = Vec::new();
        for (cle, chemin) in plan {
            let chemin = PathBuf::from(chemin);
            // L'intention est enregistree avant l'appel : meme si macOS refuse,
            // l'ancienne demande de cet ecran ne peut plus confirmer.
            let sequence = self.enregistrer_intention(&cle, &chemin);
            let cible = etat.ecrans.iter().find(|e| e.cle == cle);
            let libelle = cible
                .map(|e| format!("Display {}", e.index + 1))
                .unwrap_or_else(|| format!("Display {cle}"));
            let Some(ecran) = cible.and_then(|e| ecrans::retrouver_identifiant(e.identifiant)) else {
                self.ivars().suivi.borrow_mut().ecran_disparu(&cle, sequence);
                self.ivars()
                    .bilan
                    .borrow_mut()
                    .echouer(&cle, format!("{libelle} disconnected"));
                erreurs.push(format!("{libelle} disconnected"));
                continue;
            };
            match fond_ecran::appliquer(&chemin, &ecran) {
                Ok(()) => {
                    self.assurer_relecture();
                    appliques += 1;
                }
                Err(erreur) => {
                    self.ivars()
                        .suivi
                        .borrow_mut()
                        .echouer(&cle, sequence, &erreur.to_string());
                    self.ivars()
                        .bilan
                        .borrow_mut()
                        .echouer(&cle, format!("{libelle} : {erreur}"));
                    erreurs.push(format!("{libelle} : {erreur}"));
                }
            }
        }
        etat.enregistrer();
        etat.relever_fonds_actifs();
        self.rafraichir();
        if erreurs.is_empty() {
            self.message(&format!(
                "Slideshow: request accepted on {appliques} display(s), checking"
            ));
        } else {
            self.arreter_diaporama();
            self.rafraichir();
            let bilan = if self.ivars().suivi.borrow().est_vide() {
                self.arreter_relecture();
                self.prendre_bilan()
            } else {
                None
            };
            let texte = match bilan {
                Some(bilan) => format!(
                    "Slideshow paused: {} — {bilan}",
                    erreurs.join(", ")
                ),
                None => format!("Slideshow paused: {}", erreurs.join(", ")),
            };
            self.message(&texte);
        }
    }

    fn menu_diaporama(&self) -> Retained<NSMenu> {
        let mtm = MainThreadMarker::new().expect("menu sur le fil principal");
        let etat = unsafe { &*self.ivars().etat };
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        let demarrer = self.element_menu(
            if etat.diaporama_actif {
                "Pause"
            } else {
                "Start slideshow"
            },
            Some(sel!(basculerDiaporama:)),
            "",
            None,
        );
        demarrer.setEnabled(etat.diaporama_actif || !etat.plan_diaporama().is_empty());
        menu.addItem(&demarrer);
        let suivante =
            self.element_menu("Next image", Some(sel!(suivanteDiaporama:)), "", None);
        suivante.setEnabled(!etat.plan_diaporama().is_empty());
        menu.addItem(&suivante);
        let aleatoire =
            self.element_menu("Random order", Some(sel!(aleatoireDiaporama:)), "", None);
        aleatoire.setState(if etat.reglages.diaporama_aleatoire {
            1
        } else {
            0
        });
        menu.addItem(&aleatoire);
        let intervalle = self.element_menu("Interval", None, "", None);
        let choix = NSMenu::new(mtm);
        for secondes in [60, 300, 900, 1800, 3600] {
            let item = self.element_menu(
                &format!("{} min", secondes / 60),
                Some(sel!(intervalleDiaporama:)),
                "",
                None,
            );
            item.setTag(secondes);
            item.setState(if etat.reglages.diaporama_intervalle == secondes as u64 {
                1
            } else {
                0
            });
            choix.addItem(&item);
        }
        intervalle.setSubmenu(Some(&choix));
        menu.addItem(&intervalle);
        let ecrans = self.element_menu("Participating displays", None, "", None);
        let choix = NSMenu::new(mtm);
        for ecran in &etat.ecrans {
            let item = self.element_menu(
                &format!("Display {} — {}", ecran.index + 1, ecran.nom),
                Some(sel!(ecranDiaporama:)),
                "",
                None,
            );
            unsafe {
                item.setRepresentedObject(Some(&NSString::from_str(&ecran.cle)));
            }
            item.setState(
                if etat.cibles_diaporama().iter().any(|e| e.cle == ecran.cle) {
                    1
                } else {
                    0
                },
            );
            choix.addItem(&item);
        }
        ecrans.setSubmenu(Some(&choix));
        menu.addItem(&ecrans);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        for (titre, action) in [
            (
                "Add folder to selection",
                sel!(ajouterDossierDiaporama:),
            ),
            (
                "Remove folder from selection",
                sel!(retirerDossierDiaporama:),
            ),
            (
                "Show slideshow images only",
                sel!(filtrerDiaporama:),
            ),
        ] {
            let item = self.element_menu(titre, Some(action), "", None);
            if action == sel!(filtrerDiaporama:) {
                item.setState(if etat.filtre_diaporama { 1 } else { 0 });
            }
            menu.addItem(&item);
        }
        let information =
            self.element_menu("Continues with window closed · ⌘Q to quit", None, "", None);
        information.setEnabled(false);
        menu.addItem(&information);
        menu
    }

    fn message(&self, texte: &str) {
        journal::info(texte);
        if !self.ivars().etat_texte.is_null() {
            unsafe { &*self.ivars().etat_texte }.setStringValue(&NSString::from_str(texte));
        }
        if let Some(details) = self.ivars().details.borrow().as_ref() { details.setHidden(false); }
    }

    fn chemin_action(&self, expediteur: &NSObject) -> Option<PathBuf> {
        let contexte = expediteur
            .downcast_ref::<NSMenuItem>()
            .and_then(chemin_represente);
        let etat = unsafe { &mut *self.ivars().etat };
        let selection_avant = etat.selection;
        if let Some(chemin) = contexte {
            etat.selection = etat.images.iter().position(|i| i.chemin == chemin);
        }
        let chemin = etat
            .selection
            .and_then(|i| etat.images.get(i))
            .map(|i| i.chemin.clone());
        if selection_avant != etat.selection {
            self.rafraichir();
        } else {
            self.rafraichir_selection();
        }
        chemin
    }

    fn candidats_avif(&self) -> Vec<PathBuf> {
        unsafe { &*self.ivars().etat }.images.iter()
            .filter(|i| crate::avif::est_candidate(&i.chemin))
            .map(|i| i.chemin.clone()).collect()
    }

    fn element_menu(
        &self,
        titre: &str,
        action: Option<Sel>,
        raccourci: &str,
        chemin: Option<&std::path::Path>,
    ) -> Retained<NSMenuItem> {
        let mtm = MainThreadMarker::new().expect("menu sur le fil principal");
        let element = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &NSString::from_str(titre),
                action,
                &NSString::from_str(raccourci),
            )
        };
        unsafe {
            element.setTarget(Some(self));
            if let Some(chemin) = chemin {
                element.setRepresentedObject(Some(&NSString::from_str(&chemin.to_string_lossy())));
            }
        }
        element
    }

    fn menu_image(&self, chemin: &std::path::Path) -> Retained<NSMenu> {
        let mtm = MainThreadMarker::new().expect("menu image sur le fil principal");
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        for (titre, action) in [
            ("Apply this image", sel!(appliquer:)),
            ("Full-screen preview", sel!(apercuPleinEcran:)),
            ("Show in Finder", sel!(revelerImage:)),
            ("Copy path", sel!(copierChemin:)),
        ] {
            menu.addItem(&self.element_menu(titre, Some(action), "", Some(chemin)));
        }
        let etat = unsafe { &*self.ivars().etat };
        let selection = self.element_menu(
            "Include in slideshow",
            Some(sel!(imageDiaporama:)),
            "",
            Some(chemin),
        );
        selection.setState(
            if etat
                .reglages
                .favori_inclus(&chemin.to_string_lossy(), etat.cle_ecran_courant())
            {
                1
            } else {
                0
            },
        );
        menu.addItem(&selection);
        menu
    }

    fn menu_dossiers(&self) -> Retained<NSMenu> {
        let mtm = MainThreadMarker::new().expect("menu dossiers sur le fil principal");
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        let etat = unsafe { &*self.ivars().etat };
        if etat.reglages.dossiers_favoris.is_empty() {
            let vide =
                self.element_menu("No favorite folders — use the star", None, "", None);
            vide.setEnabled(false);
            menu.addItem(&vide);
        }
        for texte in &etat.reglages.dossiers_favoris {
            let chemin = std::path::Path::new(texte);
            let element = self.element_menu(texte, Some(sel!(ouvrirFavori:)), "", Some(chemin));
            element.setState(if chemin == etat.dossier { 1 } else { 0 });
            menu.addItem(&element);
        }
        if !etat.reglages.dossiers_favoris.is_empty() {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            let retirer = self.element_menu("Remove favorite folder", None, "", None);
            let sous_menu = NSMenu::new(mtm);
            for texte in &etat.reglages.dossiers_favoris {
                sous_menu.addItem(&self.element_menu(
                    texte,
                    Some(sel!(retirerFavori:)),
                    "",
                    Some(std::path::Path::new(texte)),
                ));
            }
            retirer.setSubmenu(Some(&sous_menu));
            menu.addItem(&retirer);
        }
        menu
    }

    fn actualiser_menu(&self) {
        let mtm = MainThreadMarker::new().expect("menu sur le fil principal");
        let etat = unsafe { &*self.ivars().etat };
        let Some(menu) = self.ivars().menu.borrow().clone() else {
            return;
        };
        menu.removeAllItems();
        for (titre, action, actif) in [
            (
                "Same image on all displays",
                sel!(memeImagePartout:),
                etat.reglages.meme_image_partout,
            ),
            (
                "Independent selection per display",
                sel!(selectionIndependante:),
                etat.reglages.selection_independante,
            ),
            ("Sort by name", sel!(trierNom:), etat.tri == Tri::Nom),
            ("Sort by date", sel!(trierDate:), etat.tri == Tri::Date),
        ] {
            let element = self.element_menu(titre, Some(action), "", None);
            element.setState(if actif { 1 } else { 0 });
            menu.addItem(&element);
        }
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let compteur = etat
            .reglages
            .favoris_selection(etat.cle_ecran_courant())
            .len();
        let diaporama = self.element_menu(
            &format!("Slideshow — {compteur} image(s)"),
            None,
            "",
            None,
        );
        diaporama.setSubmenu(Some(&self.menu_diaporama()));
        menu.addItem(&diaporama);
        let dossiers = self.element_menu("Favorite folders", None, "", None);
        dossiers.setSubmenu(Some(&self.menu_dossiers()));
        menu.addItem(&dossiers);
        for (titre, action) in [
            ("Full-screen preview", sel!(apercuPleinEcran:)),
            ("Show in Finder", sel!(revelerImage:)),
            ("Copy path", sel!(copierChemin:)),
            ("Refresh gallery", sel!(actualiser:)),
            ("AVIF cache…", sel!(cacheAvif:)),
        ] {
            let element = self.element_menu(titre, Some(action), "", None);
            if action != sel!(actualiser:) && action != sel!(cacheAvif:) {
                element.setEnabled(etat.selection.is_some());
            }
            menu.addItem(&element);
        }
        // Derniere entree : les indices existants (AVIF = 11) ne bougent pas.
        menu.addItem(&self.element_menu(
            "Settings…",
            Some(sel!(ouvrirReglages:)),
            "",
            None,
        ));
        let apparence = self.element_menu("Appearance", None, "", None);
        let choix = NSMenu::new(MainThreadMarker::new().expect("menu sur fil principal"));
        let verre = etat.reglages.champs_inconnus.get("interface_verre").and_then(|v| v.as_bool()).unwrap_or(true);
        for (titre, tag) in [("Flat", 0), ("Liquid Glass", 1)] {
            let element = self.element_menu(titre, Some(sel!(basculerMatiere:)), "", None);
            element.setTag(tag);
            element.setState(if (tag == 1) == verre { 1 } else { 0 });
            choix.addItem(&element);
        }
        apparence.setSubmenu(Some(&choix));
        menu.addItem(&apparence);
    }

    fn installer_menu_principal(&self) {
        let mtm = MainThreadMarker::new().expect("menu principal sur le fil principal");
        let principal = NSMenu::new(mtm);
        let app = self.element_menu("Mural", None, "", None);
        let sous_menu = NSMenu::new(mtm);
        // Reglages natifs : ⌘, comme toute application macOS.
        sous_menu.addItem(&self.element_menu(
            "Settings…",
            Some(sel!(ouvrirReglages:)),
            ",",
            None,
        ));
        sous_menu.addItem(&NSMenuItem::separatorItem(mtm));
        let quitter = self.element_menu("Quit Mural", Some(sel!(terminate:)), "q", None);
        unsafe {
            quitter.setTarget(None);
        }
        sous_menu.addItem(&quitter);
        app.setSubmenu(Some(&sous_menu));
        principal.addItem(&app);
        let fichier = self.element_menu("File", None, "", None);
        let commandes = NSMenu::new(mtm);
        for (titre, action, cle) in [
            ("Open folder…", sel!(ouvrirDossier:), "o"),
            (
                "Toggle favorite folder",
                sel!(dossierFavori:),
                "d",
            ),
            ("Full-screen preview", sel!(apercuPleinEcran:), "\r"),
            ("Refresh", sel!(actualiser:), "r"),
            ("Find…", sel!(ciblerRecherche:), "f"),
            ("Show controls", sel!(revelerCommandes:), "t"),
            ("Image details", sel!(afficherDetails:), "i"),
            ("Show window", sel!(afficherFenetre:), "1"),
            ("AVIF cache…", sel!(cacheAvif:), ""),
            ("Show log in Finder", sel!(revelerJournal:), ""),
        ] {
            commandes.addItem(&self.element_menu(titre, Some(action), cle, None));
        }
        fichier.setSubmenu(Some(&commandes));
        principal.addItem(&fichier);

        // Les commandes texte visent le premier repondeur (champ actif),
        // contrairement aux actions propres a Mural qui ciblent self.
        let edition = self.element_menu("Edit", None, "", None);
        let commandes_texte = NSMenu::new(mtm);
        for (titre, action, cle) in [
            ("Cut", sel!(cut:), "x"),
            ("Copy", sel!(copy:), "c"),
            ("Paste", sel!(paste:), "v"),
            ("Select All", sel!(selectAll:), "a"),
        ] {
            let element = self.element_menu(titre, Some(action), cle, None);
            unsafe { element.setTarget(None); }
            commandes_texte.addItem(&element);
        }
        edition.setSubmenu(Some(&commandes_texte));
        principal.addItem(&edition);
        NSApplication::sharedApplication(mtm).setMainMenu(Some(&principal));
    }

    fn nouvelle(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>();
        let this = this.set_ivars(CibleIvars {
            avif_panneau: RefCell::new(None),
            avif_tache: RefCell::new(None),
            avif_sources: RefCell::new(Vec::new()),
            etat: std::ptr::null_mut(),
            surface: RefCell::new(None),
            details: RefCell::new(None),
            apercu: std::ptr::null_mut(),
            etat_texte: std::ptr::null_mut(),
            nom_fichier: std::ptr::null_mut(),
            dimensions: std::ptr::null_mut(),
            bouton_appliquer: std::ptr::null_mut(),
            document: std::ptr::null_mut(),
            defilement: std::ptr::null_mut(),
            recherche: std::ptr::null_mut(),
            menu: RefCell::new(None),
            segments: std::ptr::null_mut(),
            fenetre: std::ptr::null_mut(),
            favori: std::ptr::null_mut(),
            bouton_diaporama: std::ptr::null_mut(),
            bouton_suivante: std::ptr::null_mut(),
            apercu_plein: RefCell::new(None),
            minuterie: RefCell::new(None),
            suivi: RefCell::new(suivi::Suivi::new()),
            bilan: RefCell::new(suivi::Bilan::new()),
            relecture: RefCell::new(None),
            panneau: RefCell::new(None),
        });
        unsafe { msg_send![super(this), init] }
    }

    /// Etat, avant que les vues existent.
    fn definir_etat(&self, etat: *mut Etat) {
        let ivars = self.ivars() as *const CibleIvars as *mut CibleIvars;
        unsafe {
            (*ivars).etat = etat;
        }
    }

    /// Vues, une fois qu'elles existent.
    #[allow(clippy::too_many_arguments)]
    fn definir_vues(
        &self,
        apercu: &NSImageView,
        etat_texte: &NSTextField,
        nom_fichier: &NSTextField,
        dimensions: &NSTextField,
        bouton_appliquer: &NSButton,
        document: &NSView,
        defilement: &NSScrollView,
        recherche: &NSSearchField,
        menu: &NSMenu,
    ) {
        let ivars = self.ivars() as *const CibleIvars as *mut CibleIvars;
        unsafe {
            (*ivars).apercu = apercu as *const NSImageView as *mut NSImageView;
            (*ivars).etat_texte = etat_texte as *const NSTextField as *mut NSTextField;
            (*ivars).nom_fichier = nom_fichier as *const NSTextField as *mut NSTextField;
            (*ivars).dimensions = dimensions as *const NSTextField as *mut NSTextField;
            (*ivars).bouton_appliquer = bouton_appliquer as *const NSButton as *mut NSButton;
            (*ivars).document = document as *const NSView as *mut NSView;
            (*ivars).defilement = defilement as *const NSScrollView as *mut NSScrollView;
            (*ivars).recherche = recherche as *const NSSearchField as *mut NSSearchField;
            *(*ivars).menu.borrow_mut() = Some(menu.retain());
        }
    }

    /// Change de dossier : relit, regenere les vignettes, reconstruit la grille.
    fn charger_dossier(&self, dossier: PathBuf) {
        if !dossier.is_dir() {
            journal::erreur(&format!("dossier invalide : {}", dossier.display()));
            return;
        }
        {
            let etat = unsafe { &mut *self.ivars().etat };
            let cle = etat.ecrans.get(etat.ecran_courant).map(|e| e.cle.clone());
            etat.reglages.memoriser_dossier(&dossier, cle.as_deref());
            etat.dossier = dossier;
            etat.filtre.clear();
            unsafe { &*self.ivars().recherche }.setStringValue(&NSString::from_str(""));
            etat.enregistrer();
            etat.recharger();
        }
        self.rafraichir_grille();
        self.rafraichir();
    }

    /// Seuls les pixels visibles et une rangee de marge restent charges.
    fn charger_zone_visible(&self) {
        let ivars = self.ivars();
        if ivars.etat.is_null() || ivars.document.is_null() { return; }
        let etat = unsafe { &mut *ivars.etat };
        if etat.chargement_integral { return; }
        let visible = unsafe { &*ivars.document }.visibleRect();
        let mut indices: Vec<_> = etat.cellules.iter().filter_map(|(&adresse, &index)| {
            let cadre = unsafe { &*(adresse as *const NSButton) }.frame();
            let marge = cadre.size.height + ESPACEMENT;
            (cadre.origin.y < visible.origin.y + visible.size.height + marge
                && cadre.origin.y + cadre.size.height > visible.origin.y - marge)
                .then_some(index)
        }).collect();
        indices.sort_unstable();
        etat.demander_vignettes(indices);
        // NSButton retient aussi l'image : liberer les deux proprietaires.
        for (&adresse, &index) in &etat.cellules {
            if etat.vignettes[index].is_none() {
                let bouton = unsafe { &*(adresse as *const NSButton) };
                bouton.setImage(None);
                bouton.setTitle(&NSString::from_str(if etat.vignettes_en_attente[index] { "…" } else { "" }));
            }
        }
        unsafe { &*ivars.etat_texte }.setStringValue(&NSString::from_str(&etat.statut_bibliotheque()));
    }

    /// (Re)construit les cellules de la grille pour l'etat courant.
    fn rafraichir_grille(&self) {
        let ivars = self.ivars();
        if ivars.etat.is_null() || ivars.document.is_null() {
            return;
        }
        let etat = unsafe { &mut *ivars.etat };
        let document = unsafe { &*ivars.document };
        let defilement = unsafe { &*ivars.defilement };
        defilement.contentView().setPostsBoundsChangedNotifications(false);

        // On retire les anciennes cellules avant d'en recreer.
        let anciennes = unsafe { document.subviews() };
        for index in 0..anciennes.count() {
            let vue = unsafe { anciennes.objectAtIndex(index) };
            unsafe { vue.removeFromSuperview() };
        }
        etat.cellules.clear();

        let largeur_grille = defilement.contentSize().width;
        let (colonnes, largeur_cellule, mut hauteur_cellule) = mesures_grille(largeur_grille);
        let marge = 2.0;
        let affiches = etat.indices_affiches();
        let lignes = affiches.len().div_ceil(colonnes.max(1));
        if lignes > 0 {
            hauteur_cellule = hauteur_cellule.max((defilement.contentSize().height - 8.0
                - ESPACEMENT * (lignes.saturating_sub(1)) as f64) / lignes as f64);
        }

        for (position, index) in affiches.iter().enumerate() {
            let colonne = position % colonnes;
            let ligne = position / colonnes;
            let bouton = unsafe {
                {
                    let Some(mtm) = MainThreadMarker::new() else {
                        return;
                    };
                    let bouton: Retained<BoutonVignette> =
                        msg_send![mtm.alloc::<BoutonVignette>(), init];
                    bouton.setTitle(&NSString::from_str(""));
                    bouton.setTarget(Some(self));
                    bouton.setAction(Some(sel!(celluleCliquee:)));
                    bouton.setImagePosition(objc2_app_kit::NSCellImagePosition::ImageOnly);
                    let menu = self.menu_image(&etat.images[*index].chemin);
                    bouton.setMenu(Some(&menu));
                    Retained::into_super(bouton)
                }
            };
            unsafe {
                bouton.setBordered(false);
                // Le titre devient vide apres le chargement de l'image :
                // conserver un nom accessible independant de ce placeholder.
                let nom = NSString::from_str(&etat.images[*index].nom);
                let _: () = msg_send![&*bouton, setAccessibilityLabel: &*nom];
                bouton.setToolTip(Some(&nom));
                bouton.setFrame(NSRect {
                    origin: NSPoint {
                        x: marge + colonne as f64 * (largeur_cellule + ESPACEMENT),
                        y: 4.0 + ligne as f64 * (hauteur_cellule + ESPACEMENT),
                    },
                    size: NSSize {
                        width: largeur_cellule,
                        height: hauteur_cellule,
                    },
                });
                if let Some(vignette) = etat.vignettes.get(*index).and_then(|v| v.as_deref()) {
                    bouton.setImage(Some(vignette));
                    bouton.setImageScaling(NSImageScaling::ScaleAxesIndependently);
                } else {
                    bouton.setTitle(&NSString::from_str(if etat.vignettes_en_attente[*index] {
                        "…"
                    } else {
                        "(unreadable)"
                    }));
                }
                bouton.setWantsLayer(true);
                if let Some(couche) = bouton.layer() {
                    couche.setCornerRadius(6.0);
                    couche.setMasksToBounds(true);
                }
                for (tag, icone, legende, x, y) in [
                    (8101, "checkmark.circle.fill", "Wallpaper path active on a display", largeur_cellule - 24.0,
                        if bouton.isFlipped() { 6.0 } else { hauteur_cellule - 24.0 }),
                    (8102, "star.fill", "Included in this display’s slideshow", 6.0,
                        if bouton.isFlipped() { hauteur_cellule - 24.0 } else { 6.0 }),
                ] {
                    let mtm = MainThreadMarker::new().expect("badge sur le fil principal");
                    let badge: Retained<BadgeVignette> = msg_send![mtm.alloc::<BadgeVignette>(), init];
                    badge.setTag(tag);
                    badge.setFrame(NSRect { origin: NSPoint { x, y }, size: NSSize { width: 18.0, height: 18.0 } });
                    badge.setImage(Some(&symbole(icone)));
                    let teinte = if tag == 8101 { NSColor::controlAccentColor() } else { NSColor::systemYellowColor() };
                    badge.setContentTintColor(Some(&teinte));
                    let _: () = msg_send![&*badge, setAccessibilityLabel: &*NSString::from_str(legende)];
                    badge.setWantsLayer(true);
                    if let Some(couche) = badge.layer() {
                        couche.setBackgroundColor(Some(&NSColor::windowBackgroundColor().CGColor()));
                        couche.setCornerRadius(9.0);
                    }
                    badge.setHidden(true);
                    bouton.addSubview(&badge);
                }
                document.addSubview(&bouton);
            }
            etat.cellules
                .insert(unsafe { &*bouton } as *const NSButton as usize, *index);
        }

        let lignes = affiches.len().div_ceil(colonnes.max(1));
        let hauteur_document = 8.0 + lignes as f64 * hauteur_cellule + lignes.saturating_sub(1) as f64 * ESPACEMENT;
        unsafe {
            document.setFrame(NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: largeur_grille,
                    height: hauteur_document.max(unsafe { defilement.frame() }.size.height),
                },
            });
        }
        journal::info(&format!(
            "grille : {} vignette(s) affichee(s) sur {} colonnes{}",
            affiches.len(),
            colonnes,
            if etat.filtre.is_empty() {
                String::new()
            } else {
                format!(" (filtre « {} »)", etat.filtre)
            }
        ));
        defilement.contentView().setPostsBoundsChangedNotifications(true);
        self.charger_zone_visible();
    }

    /// Reapplique l'etat : apercu, informations, libelle du bouton, selection.
    fn rafraichir(&self) {
        let ivars = self.ivars();
        if ivars.etat.is_null() {
            return;
        }
        let etat = unsafe { &*ivars.etat };

        let (apercu, nom, dims) = match etat.selection {
            Some(index) => {
                let image = &etat.images[index];
                let debut = std::time::Instant::now();
                let apercu = unsafe {
                    bibliotheque::apercu(&image.chemin, 1600, 780).map(|v| {
                        let taille = NSSize {
                            width: v.largeur as f64,
                            height: v.hauteur as f64,
                        };
                        NSImage::initWithCGImage_size(NSImage::alloc(), &v.image, taille)
                    })
                };
                if apercu.is_none() {
                    journal::erreur(&format!("apercu impossible : {}", image.nom));
                }
                let dims = bibliotheque::dimensions(&image.chemin)
                    .map(|(l, h)| format!("{l} x {h} px"))
                    .unwrap_or_default();
                let taille = bibliotheque::taille_lisible(std::fs::metadata(&image.chemin).ok().map(|m| m.len()));
                let dims = if dims.is_empty() { taille } else { format!("{dims} · {taille}") };
                journal::info(&format!(
                    "apercu de « {} » en {} ms",
                    image.nom,
                    debut.elapsed().as_millis()
                ));
                (
                    apercu,
                    image.nom.clone(),
                    dims,
                )
            }
            None => (
                None,
                String::new(),
                String::new(),
            ),
        };

        if !ivars.favori.is_null() {
            let favori = etat
                .reglages
                .dossiers_favoris
                .contains(&etat.dossier.to_string_lossy().into_owned());
            unsafe { &*ivars.favori }.setImage(Some(&symbole(if favori {
                "star.fill"
            } else {
                "star"
            })));
            unsafe { &*ivars.favori }.setToolTip(Some(&NSString::from_str(if favori {
                "Remove folder from favorites"
            } else {
                "Add folder to favorites"
            })));
        }
        if !ivars.fenetre.is_null() {
            unsafe { &*ivars.fenetre }.setTitle(&NSString::from_str(&format!(
                "Mural — {}",
                etat.dossier
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            )));
            unsafe { &*ivars.fenetre }
                .setRepresentedFilename(&NSString::from_str(&etat.dossier.to_string_lossy()));
        }
        if !ivars.bouton_diaporama.is_null() {
            let disponible = !etat.plan_diaporama().is_empty();
            let bouton = unsafe { &*ivars.bouton_diaporama };
            bouton.setImage(Some(&symbole(if etat.diaporama_actif {
                "pause.fill"
            } else {
                "play.fill"
            })));
            bouton.setToolTip(Some(&NSString::from_str(if etat.diaporama_actif {
                "Pause slideshow"
            } else {
                "Start slideshow"
            })));
            bouton.setEnabled(etat.diaporama_actif || disponible);
            unsafe { &*ivars.bouton_suivante }.setEnabled(disponible);
        }
        let libelle = if etat.reglages.meme_image_partout {
            "Apply to all".to_owned()
        } else if etat.ecrans.len() > 1 {
            format!("Apply (display {})", etat.ecran_courant + 1)
        } else {
            "Apply".to_string()
        };

        let legende = etat.statut_bibliotheque();
        unsafe {
            (*ivars.apercu).setImage(apercu.as_deref());
            (*ivars.nom_fichier).setStringValue(&NSString::from_str(&nom));
            (*ivars.dimensions).setStringValue(&NSString::from_str(&dims));
            (*ivars.dimensions).setToolTip(Some(&NSString::from_str(&dims)));
            (*ivars.etat_texte).setStringValue(&NSString::from_str(
                &legende,
            ));
            (*ivars.bouton_appliquer).setTitle(&NSString::from_str(&libelle));
            (*ivars.bouton_appliquer)
                .setEnabled(etat.selection.is_some() && !etat.ecrans.is_empty());
            // Le panneau de reglages, la barre principale et les menus lisent
            // tous le meme etat : la selection d'ecran reste synchronisee.
            if !ivars.segments.is_null() {
                (*ivars.segments).setSelectedSegment(if etat.ecrans.is_empty() {
                    -1
                } else {
                    etat.ecran_courant as isize
                });
            }
        }

        self.rafraichir_selection();
        if let Some(panneau) = self.ivars().panneau.borrow().as_ref() {
            panneau.actualiser(etat);
        }
    }

    fn rafraichir_selection(&self) {
        let etat = unsafe { &*self.ivars().etat };
        // Contour de selection : 3 points, couleur d'accent, a l'interieur.
        let selection = etat.selection;
        let accent = unsafe { NSColor::controlAccentColor() };
        let couleur = unsafe { accent.CGColor() };
        for (pointeur, index) in etat.cellules.iter() {
            let bouton: &NSButton = unsafe { &*(*pointeur as *const NSButton) };
            let selectionnee = selection == Some(*index);
            let chemin = etat.images[*index].chemin.to_string_lossy();
            for badge in bouton.subviews() {
                match badge.tag() {
                    8101 => badge.setHidden(!etat.fonds_actifs.iter().any(|p| p == chemin.as_ref())),
                    8102 => badge.setHidden(!etat.reglages.favori_inclus(&chemin, etat.cle_ecran_courant())),
                    _ => {}
                }
            }
            unsafe {
                bouton.setWantsLayer(true);
                if let Some(couche) = bouton.layer() {
                    couche.setCornerRadius(6.0);
                    couche.setMasksToBounds(true);
                    couche.setBorderWidth(if selectionnee { 3.0 } else { 0.0 });
                    couche.setBorderColor(Some(&couleur));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

/// Construit la fenetre complete. `afficher` la rend visible (le rendu hors
/// ecran la laisse cachee).
pub fn construire(
    mtm: MainThreadMarker,
    dossier: PathBuf,
    afficher: bool,
) -> (Retained<NSWindow>, *mut Etat) {
    let debut = std::time::Instant::now();
    journal::demarrage();
    let cible = Cible::nouvelle(mtm);
    let etat = Box::into_raw(Box::new(Etat::nouveau(
        dossier,
        &*cible as *const Cible as usize,
        !afficher,
    )));
    cible.definir_etat(etat);
    if !afficher {
        // Les outils hors ecran n'executent pas la boucle AppKit : attendre
        // explicitement leurs images permet un rendu reproductible.
        let etat = unsafe { &mut *etat };
        if let Some(reception) = etat.reception.take() {
            for (index, vignette) in reception {
                etat.vignettes_en_attente[index] = false;
                etat.vignettes[index] = vignette.map(|v| {
                    NSImage::initWithCGImage_size(
                        NSImage::alloc(),
                        &v.image,
                        NSSize {
                            width: v.largeur as f64,
                            height: v.hauteur as f64,
                        },
                    )
                });
            }
        }
    }

    let (largeur, hauteur) = unsafe { ((*etat).largeur, (*etat).hauteur) };
    let cadre = NSRect {
        origin: NSPoint { x: 0.0, y: 0.0 },
        size: NSSize {
            width: largeur,
            height: hauteur,
        },
    };
    let fenetre = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            cadre,
            NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable
                | NSWindowStyleMask::Resizable
                | NSWindowStyleMask::FullSizeContentView,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe {
        fenetre.setReleasedWhenClosed(false);
        fenetre.setTitle(&NSString::from_str("Mural"));
        fenetre.setTitleVisibility(objc2_app_kit::NSWindowTitleVisibility::Hidden);
        fenetre.setTitlebarAppearsTransparent(true);
        fenetre.setAcceptsMouseMovedEvents(true);
        fenetre.setContentMinSize(NSSize {
            width: 880.0,
            height: 520.0,
        });
        fenetre.center();
    }

    let contenu = interface::Racine::new(mtm);
    unsafe {
        contenu.setFrame(cadre);
        fenetre.setContentView(Some(&contenu));
    }

    // --- Barre d'outils -----------------------------------------------------
    let barre = NSView::new(mtm);
    unsafe {
        barre.setFrame(NSRect {
            origin: NSPoint {
                x: 0.0,
                y: hauteur - HAUTEUR_BARRE,
            },
            size: NSSize {
                width: largeur,
                height: HAUTEUR_BARRE,
            },
        });
        contenu.addSubview(&barre);
    }

    // Bouton dossier : action branchee a la creation (target non retenu).
    let bouton_dossier = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(""),
            Some(&cible),
            Some(sel!(ouvrirDossier:)),
            mtm,
        )
    };
    unsafe {
        bouton_dossier.setBordered(false);
        bouton_dossier.setImage(Some(&symbole("folder")));
        bouton_dossier.setToolTip(Some(&NSString::from_str("Open folder")));
        bouton_dossier.setFrame(NSRect {
            origin: NSPoint { x: 14.0, y: 9.0 },
            size: NSSize {
                width: 30.0,
                height: 28.0,
            },
        });
        barre.addSubview(&bouton_dossier);
    }

    let bouton_favori = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(""),
            Some(&cible),
            Some(sel!(dossierFavori:)),
            mtm,
        )
    };
    let bouton_bibliotheque = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(""),
            Some(&cible),
            Some(sel!(dossiersFavoris:)),
            mtm,
        )
    };
    for (bouton, x, icone, aide) in [
        (
            &bouton_favori,
            48.0,
            "star",
            "Toggle favorite folder",
        ),
        (
            &bouton_bibliotheque,
            82.0,
            "books.vertical",
            "Favorite folders",
        ),
    ] {
        bouton.setBordered(false);
        bouton.setImage(Some(&symbole(icone)));
        bouton.setToolTip(Some(&NSString::from_str(aide)));
        bouton.setFrame(NSRect {
            origin: NSPoint { x, y: 9.0 },
            size: NSSize {
                width: 30.0,
                height: 28.0,
            },
        });
        barre.addSubview(bouton);
    }

    // Champ de recherche : action validee par Entree.
    let recherche = NSSearchField::new(mtm);
    unsafe {
        recherche.setFrame(NSRect {
            origin: NSPoint { x: 120.0, y: 9.0 },
            size: NSSize {
                width: 190.0,
                height: 28.0,
            },
        });
    }
    unsafe {
        recherche.setTarget(Some(&cible));
        recherche.setAction(Some(sel!(rechercher:)));
        recherche.setPlaceholderString(Some(&NSString::from_str("Search")));
        barre.addSubview(&recherche);
    }

    // Selection d'ecran : compacte, centree.
    let etiquettes: Vec<Retained<NSString>> = unsafe { &*etat }
        .ecrans
        .iter()
        .map(|e| NSString::from_str(&format!("Display {}", e.index + 1)))
        .collect();
    let liste = NSArray::from_retained_slice(&etiquettes);
    let segments = unsafe {
        NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
            &liste,
            NSSegmentSwitchTracking::SelectOne,
            Some(&cible),
            Some(sel!(ecranChange:)),
            mtm,
        )
    };
    let largeur_segments = 168.0;
    unsafe {
        segments.setFrame(NSRect {
            origin: NSPoint {
                x: (largeur - largeur_segments) / 2.0,
                y: 9.0,
            },
            size: NSSize {
                width: largeur_segments,
                height: 28.0,
            },
        });
        segments.setSelectedSegment(unsafe { &*etat }.ecran_courant as isize);
        barre.addSubview(&segments);
    }

    // Bouton principal.
    let bouton_appliquer = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str("Apply"),
            Some(&cible),
            Some(sel!(appliquer:)),
            mtm,
        )
    };
    unsafe {
        bouton_appliquer.setBezelStyle(NSBezelStyle::Rounded);
        bouton_appliquer.setFrame(NSRect {
            origin: NSPoint {
                x: largeur - 148.0,
                y: 8.0,
            },
            size: NSSize {
                width: 134.0,
                height: 30.0,
            },
        });
        barre.addSubview(&bouton_appliquer);
    }

    let bouton_diaporama = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(""),
            Some(&cible),
            Some(sel!(basculerDiaporama:)),
            mtm,
        )
    };
    let bouton_suivante = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(""),
            Some(&cible),
            Some(sel!(suivanteDiaporama:)),
            mtm,
        )
    };
    for (bouton, distance, icone, aide) in [
        (
            &bouton_diaporama,
            256.0,
            "play.fill",
            "Start slideshow",
        ),
        (
            &bouton_suivante,
            220.0,
            "forward.end.fill",
            "Next slideshow image",
        ),
    ] {
        bouton.setBordered(false);
        bouton.setImage(Some(&symbole(icone)));
        bouton.setToolTip(Some(&NSString::from_str(aide)));
        bouton.setFrame(NSRect {
            origin: NSPoint {
                x: largeur - distance,
                y: 9.0,
            },
            size: NSSize {
                width: 30.0,
                height: 28.0,
            },
        });
        bouton.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
        barre.addSubview(bouton);
    }

    // Menu « ... » : entrees branchees sur la meme cible.
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    let bouton_menu = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(""),
            Some(&cible),
            Some(sel!(menuOuvrir:)),
            mtm,
        )
    };
    unsafe {
        bouton_menu.setBordered(false);
        bouton_menu.setImage(Some(&symbole("ellipsis.circle")));
        bouton_menu.setToolTip(Some(&NSString::from_str("More options")));
        bouton_menu.setFrame(NSRect {
            origin: NSPoint {
                x: largeur - 184.0,
                y: 9.0,
            },
            size: NSSize {
                width: 30.0,
                height: 28.0,
            },
        });
        barre.addSubview(&bouton_menu);
    }

    // --- Apercu panoramique -------------------------------------------------
    let cadre_apercu = NSRect {
        origin: NSPoint {
            x: 0.0,
            y: hauteur - HAUTEUR_BARRE - HAUTEUR_APERCU,
        },
        size: NSSize {
            width: largeur,
            height: HAUTEUR_APERCU,
        },
    };
    let fond_apercu = NSImageView::new(mtm);
    let apercu = interface::Apercu::new(mtm);
    unsafe {
        fond_apercu.setFrame(cadre_apercu);
        fond_apercu.setImageScaling(NSImageScaling::ScaleNone);
        fond_apercu.setWantsLayer(true);
        if let Some(couche) = fond_apercu.layer() {
            couche.setBackgroundColor(Some(&NSColor::underPageBackgroundColor().CGColor()));
        }
        contenu.addSubview(&fond_apercu);

        apercu.setFrame(cadre_apercu);
        apercu.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        contenu.addSubview(&apercu);
    }

    // --- Grille -------------------------------------------------------------
    let haut_grille = hauteur - HAUTEUR_BARRE - HAUTEUR_APERCU;
    let cadre_grille = NSRect {
        origin: NSPoint {
            x: 0.0,
            y: HAUTEUR_ETAT,
        },
        size: NSSize {
            width: largeur,
            height: haut_grille - HAUTEUR_ETAT,
        },
    };
    let defilement = unsafe { NSScrollView::new(mtm) };
    let document = VueRetournee::new(mtm);
    unsafe {
        defilement.setDocumentView(Some(&document));
        defilement.setFrame(cadre_grille);
        defilement.setHasVerticalScroller(true);
        defilement.setScrollerStyle(objc2_app_kit::NSScrollerStyle::Overlay);
        defilement.setDrawsBackground(true);
        defilement.setBackgroundColor(&NSColor::windowBackgroundColor());
        contenu.addSubview(&defilement);
    }

    // --- Barre d'etat -------------------------------------------------------
    let etat_texte =
        unsafe { NSTextField::labelWithString(&NSString::from_str("Select an image"), mtm) };
    let nom_fichier = unsafe { NSTextField::labelWithString(&NSString::from_str(""), mtm) };
    let dimensions = unsafe { NSTextField::labelWithString(&NSString::from_str(""), mtm) };
    unsafe {
        etat_texte.setFrame(NSRect {
            origin: NSPoint { x: 12.0, y: 8.0 },
            size: NSSize {
                width: largeur * 0.42,
                height: 18.0,
            },
        });
        nom_fichier.setFrame(NSRect {
            origin: NSPoint {
                x: largeur * 0.45,
                y: 8.0,
            },
            size: NSSize {
                width: largeur * 0.28,
                height: 18.0,
            },
        });
        nom_fichier.setFont(Some(&NSFont::boldSystemFontOfSize(11.0)));
        dimensions.setFrame(NSRect {
            origin: NSPoint {
                x: largeur * 0.74,
                y: 8.0,
            },
            size: NSSize {
                width: largeur * 0.25,
                height: 18.0,
            },
        });
        for vue in [&etat_texte, &nom_fichier, &dimensions] {
            vue.setBezeled(false);
            vue.setEditable(false);
            vue.setSelectable(false);
            vue.setDrawsBackground(false);
            vue.setTextColor(Some(&NSColor::secondaryLabelColor()));
            contenu.addSubview(vue);
        }
    }

    // AppKit maintient les ancrages ; la notification recalcule les colonnes.
    let largeur_flexible = NSAutoresizingMaskOptions::ViewWidthSizable;
    let haut_fixe = largeur_flexible | NSAutoresizingMaskOptions::ViewMinYMargin;
    barre.setAutoresizingMask(haut_fixe);
    fond_apercu.setAutoresizingMask(haut_fixe);
    apercu.setAutoresizingMask(haut_fixe);
    defilement.setAutoresizingMask(largeur_flexible | NSAutoresizingMaskOptions::ViewHeightSizable);
    segments.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMaxXMargin,
    );
    etat_texte.setAutoresizingMask(largeur_flexible);
    bouton_appliquer.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
    bouton_menu.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
    nom_fichier.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
    dimensions.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);

    // Composition validee : deux surfaces et des commandes flottantes.
    // Le split natif gere le drag sans minuterie ; la jonction reste invisible.
    let separation = interface::Separation::new(mtm);
    separation.setFrame(contenu.bounds());
    separation.setVertical(false);
    separation.setAutoresizingMask(largeur_flexible | NSAutoresizingMaskOptions::ViewHeightSizable);
    fond_apercu.removeFromSuperview();
    apercu.removeFromSuperview();
    defilement.removeFromSuperview();
    apercu.setFrame(NSRect::new(NSPoint::ZERO, NSSize::new(largeur, hauteur * 0.48)));
    defilement.setFrame(NSRect::new(NSPoint::ZERO, NSSize::new(largeur, hauteur * 0.52 - 1.0)));
    separation.addSubview(&apercu);
    separation.addSubview(&defilement);
    separation.adjustSubviews();
    separation.setPosition_ofDividerAtIndex(contenu.bounds().size.height * 0.48, 0);
    contenu.addSubview(&separation);
    // Reconstruire la grille aussi pendant le redimensionnement de l'aperçu.
    unsafe {
        objc2_foundation::NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &cible, sel!(fenetreRedimensionnee:),
            Some(objc2_app_kit::NSSplitViewDidResizeSubviewsNotification), Some(&separation));
    }
    let verre = unsafe { &*etat }.reglages.champs_inconnus.get("interface_verre")
        .and_then(|v| v.as_bool()).unwrap_or(true);
    let largeur_outils = (largeur_segments + 416.0).max(500.0);
    let hauteur_surface = contenu.bounds().size.height;
    let cadre_outils = NSRect::new(NSPoint::new((largeur-largeur_outils)/2.0, hauteur_surface-54.0), NSSize::new(largeur_outils, 42.0));
    let outils = interface::panneau(mtm, cadre_outils, verre);
    outils.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMaxXMargin | NSAutoresizingMaskOptions::ViewMinYMargin);
    barre.removeFromSuperview();
    barre.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
    interface::remplir(&outils, &barre);
    contenu.addSubview(&outils);
    recherche.setFrame(NSRect::new(NSPoint::new(8.0, 7.0), NSSize::new(150.0, 28.0)));
    segments.setFrame(NSRect::new(NSPoint::new(164.0, 7.0), NSSize::new(largeur_segments, 28.0)));
    segments.setAutoresizingMask(NSAutoresizingMaskOptions::empty());
    let x = 170.0 + largeur_segments;
    bouton_appliquer.setFrame(NSRect::new(NSPoint::new(x, 6.0), NSSize::new(134.0, 30.0)));
    bouton_diaporama.setFrame(NSRect::new(NSPoint::new(x+140.0, 7.0), NSSize::new(28.0, 28.0)));
    bouton_suivante.setHidden(true); // retenu par la barre ; accessible par le menu Diaporama
    bouton_menu.setFrame(NSRect::new(NSPoint::new(largeur_outils-36.0, 7.0), NSSize::new(28.0, 28.0)));
    let details_bouton = unsafe { NSButton::buttonWithTitle_target_action(
        &NSString::from_str(""), Some(&cible), Some(sel!(afficherDetails:)), mtm) };
    details_bouton.setBordered(false);
    details_bouton.setImage(Some(&symbole("info.circle")));
    details_bouton.setToolTip(Some(&NSString::from_str("Image details")));
    details_bouton.setFrame(NSRect::new(NSPoint::new(largeur_outils-68.0, 7.0), NSSize::new(28.0,28.0)));
    barre.addSubview(&details_bouton);

    let dossiers = interface::panneau(mtm,
        NSRect::new(NSPoint::new(8.0, hauteur_surface-176.0), NSSize::new(184.0, 112.0)), verre);
    dossiers.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    let dossiers_contenu = NSView::new(mtm);
    interface::remplir(&dossiers, &dossiers_contenu);
    for (bouton, y, titre) in [(&bouton_dossier, 72.0, "Open folder…"),
        (&bouton_bibliotheque, 40.0, "Favorite folders"), (&bouton_favori, 8.0, "Favorite this folder")] {
        bouton.removeFromSuperview();
        bouton.setTitle(&NSString::from_str(titre));
        bouton.setImagePosition(objc2_app_kit::NSCellImagePosition::ImageLeft);
        bouton.setFrame(NSRect::new(NSPoint::new(8.0, y), NSSize::new(168.0,28.0)));
        dossiers_contenu.addSubview(bouton);
    }
    contenu.addSubview(&dossiers);
    let details = interface::panneau(mtm,
        NSRect::new(NSPoint::new(largeur-382.0, 12.0), NSSize::new(370.0,88.0)), false);
    details.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
    let details_contenu = NSView::new(mtm);
    interface::remplir(&details, &details_contenu);
    for (vue, y) in [(&nom_fichier, 60.0), (&dimensions, 36.0), (&etat_texte, 12.0)] {
        vue.removeFromSuperview();
        vue.setFrame(NSRect::new(NSPoint::new(12.0,y),NSSize::new(346.0,18.0)));
        vue.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
        details_contenu.addSubview(vue);
    }
    details.setHidden(true);
    contenu.addSubview(&details);
    contenu.panneaux(&outils, &dossiers);
    *cible.ivars().surface.borrow_mut() = Some(contenu.clone());
    *cible.ivars().details.borrow_mut() = Some(details);

    // Les vues existent : on les branche.
    cible.definir_vues(
        &apercu,
        &etat_texte,
        &nom_fichier,
        &dimensions,
        &bouton_appliquer,
        &document,
        &defilement,
        &recherche,
        &menu,
    );

    defilement.contentView().setPostsBoundsChangedNotifications(true);
    unsafe {
        objc2_foundation::NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &cible, sel!(zoneVisibleModifiee:),
            Some(objc2_app_kit::NSViewBoundsDidChangeNotification), Some(&defilement.contentView()));
    }
    unsafe {
        objc2_foundation::NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &cible,
            sel!(fenetreRedimensionnee:),
            Some(objc2_app_kit::NSWindowDidResizeNotification),
            Some(&fenetre),
        );
    }
    unsafe {
        let ivars = cible.ivars() as *const CibleIvars as *mut CibleIvars;
        (*ivars).segments = &*segments as *const NSSegmentedControl as *mut NSSegmentedControl;
        (*ivars).fenetre = &*fenetre as *const NSWindow as *mut NSWindow;
        (*ivars).favori = &*bouton_favori as *const NSButton as *mut NSButton;
        (*ivars).bouton_diaporama = &*bouton_diaporama as *const NSButton as *mut NSButton;
        (*ivars).bouton_suivante = &*bouton_suivante as *const NSButton as *mut NSButton;
        for nom in [objc2_app_kit::NSWindowWillCloseNotification] {
            objc2_foundation::NSNotificationCenter::defaultCenter()
                .addObserver_selector_name_object(
                    &cible,
                    sel!(enregistrerEtat:),
                    Some(nom),
                    Some(&fenetre),
                );
        }
        objc2_foundation::NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &cible,
            sel!(enregistrerEtat:),
            Some(objc2_app_kit::NSApplicationWillTerminateNotification),
            None,
        );
        objc2_foundation::NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &cible,
            sel!(ecransModifies:),
            Some(objc2_app_kit::NSApplicationDidChangeScreenParametersNotification),
            None,
        );
    }
    cible.actualiser_menu();
    cible.installer_menu_principal();
    NSApplication::sharedApplication(mtm).setDelegate(Some(ProtocolObject::from_ref(&*cible)));
    cible.rafraichir_grille();
    cible.rafraichir();

    // AppKit ne retient pas le target d'un NSControl : sans cette ligne,
    // aucune action ne se declenche jamais.
    std::mem::forget(cible);

    if afficher {
        unsafe { fenetre.makeKeyAndOrderFront(None) };
        journal::info(&format!(
            "fenetre affichee en {} ms (vignettes en arriere-plan)",
            debut.elapsed().as_millis()
        ));
    }
    (fenetre, etat)
}

/// Image systeme (SF Symbol).
fn symbole(nom: &str) -> Retained<NSImage> {
    unsafe {
        NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(nom), None)
            .unwrap_or_else(|| {
                let vide = NSImage::new();
                vide.setSize(NSSize {
                    width: 16.0,
                    height: 16.0,
                });
                vide
            })
    }
}

/// Point d'entree AppKit, sur le fil principal.
pub fn lancer(mtm: MainThreadMarker, dossier: PathBuf) {
    let application = NSApplication::sharedApplication(mtm);
    unsafe {
        application.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    }
    let (_fenetre, _etat) = objc2::rc::autoreleasepool(|_| construire(mtm, dossier, true));
    unsafe {
        application.activate();
        application.run();
    }
}

/// La couche de selection a besoin du trait en portee pour ses methodes.
#[allow(dead_code)]
fn _garder(_couche: &CALayer, _objet: &dyn Fn() -> Option<Retained<Sel>>) {}

fn chemin_represente(element: &NSMenuItem) -> Option<PathBuf> {
    let objet = unsafe { element.representedObject() }?;
    objet
        .downcast_ref::<NSString>()
        .map(|s| PathBuf::from(s.to_string()))
}

#[cfg(test)]
mod tests_progression {
    use super::statut_vignettes;

    #[test]
    fn progression_compte_les_resultats_meme_illisibles() {
        assert_eq!(statut_vignettes("Selected: photo · 2 slideshow image(s)", &[true, true, true]), "Loading thumbnails 0/3");
        assert_eq!(statut_vignettes("Selected: photo · 2 slideshow image(s)", &[false, true, false]), "Loading thumbnails 2/3");
    }

    #[test]
    fn termine_ou_vide_retablit_le_statut() {
        assert_eq!(statut_vignettes("Selected: photo · 2 slideshow image(s) · playing", &[false, false]), "Selected: photo · 2 slideshow image(s) · playing");
        assert_eq!(statut_vignettes("Select an image · 0 slideshow image(s)", &[]), "Select an image · 0 slideshow image(s)");
    }
}

fn statut_vignettes(base: &str, en_attente: &[bool]) -> String {
    let restantes = en_attente.iter().filter(|&&attente| attente).count();
    if restantes == 0 { base.to_owned() } else {
        format!("Loading thumbnails {}/{}", en_attente.len() - restantes, en_attente.len())
    }
}
