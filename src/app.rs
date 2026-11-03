//! Interface AppKit de Mural — composition epuree validee le 24/10/2026.
//!
//!   apercu bord a bord redimensionnable (double-clic : image entiere)
//!   galerie opaque, sans barre d'etat permanente
//!   commandes et dossiers flottants reveles aux bords
//!   NSGlassEffectView native, avec materiau AppKit de repli
//!
//! Tout ce qui compte est journalise (voir `journal`) : chaque action, chaque
//! erreur Apple, et l'etat de la galerie au chargement.
//!
//! Piege AppKit a garder en tete : **`target` n'est PAS retenu** par un
//! `NSControl`. La cible est donc volontairement oubliee (`mem::forget`) et
//! vit aussi longtemps que le processus ; sans cela aucune action ne se
//! declenche.

use objc2_foundation::NSObjectNSDelayedPerforming;
use std::cell::{Cell, RefCell};
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
    NSAlert, NSAlertSecondButtonReturn, NSMenuItem, NSModalResponseOK, NSOpenPanel, NSPasteboard,
    NSPopUpButton, NSScrollView,
    NSSearchField, NSSegmentSwitchTracking, NSSegmentedControl, NSTextField, NSView, NSWindow,
    NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::NSObjectProtocol;
use objc2_foundation::{NSArray, NSBundle, NSObject, NSPoint, NSRect, NSSize, NSString, NSURL};
use objc2_quartz_core::CALayer;

use crate::bibliotheque::{self, Image, Tri};
use crate::journal;
use crate::interface;
use crate::reglages_panneau::Panneau;
use crate::suivi::{self, Issue};
use crate::{ecrans, fond_ecran, tous_bureaux};
#[path = "avif_panneau.rs"]
mod panneau_avif;

mod apercu;
mod construction;
mod diaporama;
mod etat;
mod grille;
mod menus;

pub use self::construction::{construire, lancer};
pub use self::etat::Etat;
pub(crate) use self::etat::InstantaneReglages;
use self::apercu::{ApercuMemoire, CleApercu, ResultatApercu};

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
    #[ivars = std::path::PathBuf]
    struct BoutonVignette;
    impl BoutonVignette {
        /// Menu contextuel construit au clic droit, pas pour les 500 cellules a chaque reconstruction.
        #[unsafe(method(menuForEvent:))]
        fn menu_pour_evenement(&self, _evenement: Option<&NSEvent>) -> *mut NSMenu {
            let Some(cible) = (unsafe { self.target() }) else {
                return std::ptr::null_mut();
            };
            let chemin = NSString::from_str(&self.ivars().to_string_lossy());
            let menu: Option<Retained<NSMenu>> =
                unsafe { msg_send![&*cible, menuPourImage: &*chemin] };
            menu.map_or(std::ptr::null_mut(), Retained::autorelease_return)
        }
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
    // Etat alloue separement et conserve jusqu'a la fin du processus.
    // Cell autorise uniquement le branchement du pointeur, pas un emprunt long.
    etat: Cell<*mut Etat>,
    /// Reponse imposee a la confirmation de suppression (recettes) : -1 aucune, 0 non, 1 oui.
    reponse_suppression: Cell<isize>,
    /// Emprunts de `etat` en cours : n lecteurs (> 0) ou un ecrivain (-1).
    emprunts: Cell<isize>,
    generation_grille: Cell<u64>,
    surface: RefCell<Option<Retained<interface::Racine>>>,
    details: RefCell<Option<Retained<NSView>>>,
    apercu: Cell<*mut NSImageView>,
    apercu_memoire: RefCell<Option<ApercuMemoire>>,
    /// Apercu en cours de decodage hors fil principal (une seule demande a la fois).
    apercu_demande: RefCell<Option<CleApercu>>,
    apercu_reception: RefCell<Option<std::sync::mpsc::Receiver<ResultatApercu>>>,
    /// Dernier apercu illisible : evite de relancer indefiniment le meme decodage.
    apercu_echec: RefCell<Option<CleApercu>>,
    etat_texte: Cell<*mut NSTextField>,
    nom_fichier: Cell<*mut NSTextField>,
    dimensions: Cell<*mut NSTextField>,
    bouton_appliquer: Cell<*mut NSButton>,
    document: Cell<*mut NSView>,
    defilement: Cell<*mut NSScrollView>,
    recherche: Cell<*mut NSSearchField>,
    menu: RefCell<Option<Retained<NSMenu>>>,
    segments: Cell<*mut NSSegmentedControl>,
    fenetre: Cell<*mut NSWindow>,
    favori: Cell<*mut NSButton>,
    bouton_diaporama: Cell<*mut NSButton>,
    bouton_suivante: Cell<*mut NSButton>,
    apercu_plein: RefCell<Option<Retained<NSWindow>>>,
    minuterie: RefCell<Option<Retained<objc2_foundation::NSTimer>>>,
    suivi: RefCell<suivi::Suivi>,
    bilan: RefCell<suivi::Bilan>,
    relecture: RefCell<Option<Retained<objc2_foundation::NSTimer>>>,
    /// Fonds a propager sur tous les bureaux : `attente` tant que macOS n'a pas
    /// confirme, `pretes` une fois confirmes (traitees quand le suivi est vide).
    propagation_attente: RefCell<Vec<(String, PathBuf)>>,
    propagation_pretes: RefCell<Vec<(String, PathBuf)>>,
    panneau: RefCell<Option<std::rc::Rc<Panneau>>>,
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
            if !self.ivars().fenetre.get().is_null() { unsafe { &*self.ivars().fenetre.get() }.makeKeyAndOrderFront(None); }
            true
        }
        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn rester_actif(&self, _application: &NSApplication) -> bool { false }
    }

    impl Cible {
        /// Menu contextuel d'une image (appele par `BoutonVignette` au clic droit).
        #[unsafe(method(menuPourImage:))]
        fn menu_pour_image(&self, chemin: &NSString) -> *mut NSMenu {
            Retained::autorelease_return(self.menu_image(std::path::Path::new(&chemin.to_string())))
        }

        /// Recettes seulement : impose la reponse du dialogue de suppression (-1 : dialogue reel).
        #[unsafe(method(forcerReponseSuppression:))]
        fn forcer_reponse_suppression(&self, valeur: isize) {
            self.ivars().reponse_suppression.set(valeur);
        }

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
                let dossier = self.etat().dossier.clone();
                panneau.preparer(&dossier, chemins.len());
                *self.ivars().avif_sources.borrow_mut() = chemins;
            }
            panneau.fenetre.makeKeyAndOrderFront(None);
        }

        #[unsafe(method(convertirAvif:))]
        fn convertir_avif(&self, _expediteur: &NSObject) {
            if self.ivars().avif_tache.borrow().is_some() || !crate::avif::avif_encodable() { return; }
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
            if self.ivars().etat.get().is_null() {
                return;
            }
            let mtm = MainThreadMarker::new().expect("settings on main thread");
            let panneau = self.ivars().panneau.borrow().clone();
            let panneau = panneau.unwrap_or_else(|| {
                let panneau = std::rc::Rc::new(Panneau::nouveau(mtm, self));
                *self.ivars().panneau.borrow_mut() = Some(panneau.clone());
                panneau
            });
            // Publier la fenetre avant les menus, dont les notifications sont synchrones.
            let etat = self.etat().instantane_reglages();
            panneau.actualiser(&etat);
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
                .map(|p| p.instantane())
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
            let index = self.etat()
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
            let dossier = self.etat().dossier.clone();
            {
                let mut etat = self.etat_mut();
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
                panneau.as_ref().and_then(|p| p.favori_selectionne())
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
                panneau.as_ref().and_then(|p| p.favori_selectionne())
            };
            if let Some(chemin) = chemin {
                self.retirer_favori_chemin(std::path::Path::new(&chemin));
            }
        }

        #[unsafe(method(imageDiaporama:))]
        fn image_diaporama(&self, expediteur: &NSObject) {
            let Some(chemin) = self.chemin_action(expediteur) else { return; };
            let cle = self.etat().cle_ecran_courant().map(str::to_owned);
            {
                let mut etat = self.etat_mut();
                let chemin = chemin.to_string_lossy().into_owned();
                etat.reglages.basculer_favori(&chemin, cle.as_deref());
                etat.enregistrer();
            }
            self.selection_diaporama_modifiee();
        }

        #[unsafe(method(ajouterDossierDiaporama:))]
        fn ajouter_dossier_diaporama(&self, _expediteur: &NSObject) {
            let cle = self.etat().cle_ecran_courant().map(str::to_owned);
            {
                let mut etat = self.etat_mut();
                let etat_brut: &mut Etat = &mut etat;
                for image in &etat_brut.images {
                    let chemin = image.chemin.to_string_lossy().into_owned();
                    etat_brut.reglages.ajouter_favori(&chemin, cle.as_deref());
                }
                etat.enregistrer();
            }
            self.selection_diaporama_modifiee();
        }

        #[unsafe(method(retirerDossierDiaporama:))]
        fn retirer_dossier_diaporama(&self, _expediteur: &NSObject) {
            let cle = self.etat().cle_ecran_courant().map(str::to_owned);
            {
                let mut etat = self.etat_mut();
                let dossier = etat.dossier.clone();
                etat.reglages.retirer_dossier_favori(&dossier, cle.as_deref());
                etat.enregistrer();
            }
            self.selection_diaporama_modifiee();
        }

        #[unsafe(method(filtrerDiaporama:))]
        fn filtrer_diaporama(&self, _expediteur: &NSObject) {
            let mut etat = self.etat_mut();
            etat.filtre_diaporama = !etat.filtre_diaporama;
            drop(etat);
            self.rafraichir_grille(); self.rafraichir_selection();
            self.rafraichir_panneau();
        }

        #[unsafe(method(basculerDiaporama:))]
        fn basculer_diaporama(&self, _expediteur: &NSObject) {
            if self.etat().diaporama_actif {
                self.arreter_diaporama(); self.rafraichir(); self.message("Slideshow paused");
            } else {
                if self.etat().plan_diaporama().is_empty() {
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
            let mut etat = self.etat_mut();
            etat.reglages.diaporama_aleatoire = !etat.reglages.diaporama_aleatoire;
            etat.enregistrer();
            drop(etat);
            self.rafraichir();
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
            if !self.ivars().fenetre.get().is_null() { unsafe { &*self.ivars().fenetre.get() }.makeKeyAndOrderFront(None); }
        }
        #[unsafe(method(enregistrerEtat:))]
        fn enregistrer_etat(&self, notification: &objc2_foundation::NSNotification) {
            if !self.ivars().etat.get().is_null() { self.etat_mut().enregistrer(); }
            if &*notification.name() == unsafe { objc2_app_kit::NSApplicationWillTerminateNotification } { self.arreter_diaporama(); self.arreter_relecture(); }
            if &*notification.name() == unsafe { objc2_app_kit::NSWindowWillCloseNotification } {
                if let Some(fenetre) = self.ivars().apercu_plein.borrow_mut().take() { fenetre.close(); }

            }
        }

        #[unsafe(method(dossierFavori:))]
        fn dossier_favori(&self, _expediteur: &NSObject) {
            {
                let mut etat = self.etat_mut();
                let dossier = etat.dossier.clone();
                etat.reglages.basculer_dossier_favori(&dossier);
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
                let mut etat = self.etat_mut();
                etat.reglages.meme_image_partout = !etat.reglages.meme_image_partout;
                etat.enregistrer();
            }
            self.rafraichir();
        }

        #[unsafe(method(tousLesBureaux:))]
        fn tous_les_bureaux(&self, _expediteur: &NSObject) {
            {
                let mut etat = self.etat_mut();
                etat.reglages.tous_les_bureaux = !etat.reglages.tous_les_bureaux;
                etat.enregistrer();
            }
            self.rafraichir();
        }

        /// Bascule la selection independante par ecran. Aucune liste n'est
        /// supprimee : le mode global et les listes dediees coexistent.
        #[unsafe(method(selectionIndependante:))]
        fn selection_independante(&self, _expediteur: &NSObject) {
            {
                let mut etat = self.etat_mut();
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

        #[unsafe(method(masquerImage:))]
        fn masquer_image(&self, expediteur: &NSObject) {
            let Some(chemin) = self.chemin_action(expediteur) else { return; };
            let nom = chemin.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            {
                let mut etat = self.etat_mut();
                etat.reglages.masquer_image(&chemin.to_string_lossy());
            }
            journal::info(&format!("image retiree de la galerie : {nom}"));
            self.recharger_galerie();
            self.message(&format!("Removed from gallery: {nom}"));
        }

        #[unsafe(method(supprimerImage:))]
        fn supprimer_image(&self, expediteur: &NSObject) {
            let Some(chemin) = self.chemin_action(expediteur) else { return; };
            let nom = chemin.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let avertissement = {
                let etat = self.etat();
                let texte = chemin.to_string_lossy();
                avertissement_suppression(
                    etat.fonds_actifs.iter().any(|p| p.as_str() == texte),
                    etat.reglages.dans_un_diaporama(&texte),
                )
            };
            if !self.confirmer_suppression(&nom, avertissement) { return; }
            let cache_vignettes = crate::cache::dossier_defaut();
            let cache_avif = crate::avif::dossier_cache();
            let resultat = {
                let mut etat = self.etat_mut();
                crate::suppression::supprimer_image(
                    &chemin,
                    &mut etat.reglages,
                    cache_vignettes.as_deref(),
                    cache_avif.as_deref(),
                )
            };
            match resultat {
                Ok(copies_cache) => {
                    journal::info(&format!(
                        "image supprimee du disque : {} ({copies_cache} copie(s) de cache retiree(s))",
                        chemin.display()
                    ));
                    self.recharger_galerie();
                    self.message(&format!("Deleted: {nom}"));
                }
                Err(erreur) => {
                    journal::erreur(&format!("suppression impossible : {} : {erreur}", chemin.display()));
                    self.message(&format!("Could not delete {nom}: {erreur}"));
                }
            }
        }

        #[unsafe(method(restaurerImages:))]
        fn restaurer_images(&self, _expediteur: &NSObject) {
            {
                let mut etat = self.etat_mut();
                etat.reglages.restaurer_images_masquees();
            }
            journal::info("commande : afficher les images masquees");
            self.recharger_galerie();
        }

        #[unsafe(method(apercuPleinEcran:))]
        fn apercu_plein_ecran(&self, expediteur: &NSObject) {
            let Some(chemin) = self.chemin_action(expediteur) else { return; };
            let Some(mtm) = MainThreadMarker::new() else { return; };
            let principale = unsafe { &*self.ivars().fenetre.get() };
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
            if ivars.etat.get().is_null() || ivars.segments.get().is_null() { return; }
            let ecrans = ecrans::lister();
            let (ecrans, courant, arreter, dossier) = {
                let mut etat = self.etat_mut();
                let id = etat.ecrans.get(etat.ecran_courant).map(|e| e.identifiant);
                etat.ecrans = ecrans;
                etat.ecran_courant = etat.ecrans.iter().position(|e| Some(e.identifiant) == id).unwrap_or(0);
                let dossier = etat.ecrans.get(etat.ecran_courant)
                    .and_then(|e| etat.reglages.dossier_ecran(&e.cle))
                    .filter(|d| *d != etat.dossier);
                (etat.ecrans.clone(), etat.ecran_courant,
                    etat.diaporama_actif && etat.plan_diaporama().is_empty(), dossier)
            };
            let segments = unsafe { &*ivars.segments.get() };
            segments.setSegmentCount(ecrans.len() as isize);
            for (index, ecran) in ecrans.iter().enumerate() {
                segments.setLabel_forSegment(&NSString::from_str(&format!("Display {}", index + 1)), index as isize);
                segments.setToolTip_forSegment(Some(&NSString::from_str(&ecran.nom)), index as isize);
            }
            segments.setSelectedSegment(if ecrans.is_empty() { -1 } else { courant as isize });
            let mut cadre = segments.frame();
            cadre.size.width = 168.0;
            cadre.origin.x = 164.0;
            segments.setFrame(cadre);
            self.relever_fonds_actifs();
            if arreter { self.arreter_diaporama(); }
            journal::info(&format!("configuration actualisee : {} ecran(s), selection {}", ecrans.len(), courant + 1));
            if let Some(dossier) = dossier { self.charger_dossier(dossier); return; }
            self.rafraichir();
        }

        #[unsafe(method(fenetreRedimensionnee:))]
        fn fenetre_redimensionnee(&self, _notification: &objc2_foundation::NSNotification) {
            let ivars = self.ivars();
            if ivars.etat.get().is_null() || ivars.defilement.get().is_null() { return; }
            let largeur = unsafe { &*ivars.defilement.get() }.frame().size.width;
            let hauteur = if !ivars.fenetre.get().is_null() {
                unsafe { &*ivars.fenetre.get() }.contentView().map(|c| c.bounds().size.height)
            } else { None };
            {
                let mut etat = self.etat_mut();
                etat.largeur = largeur;
                if let Some(hauteur) = hauteur { etat.hauteur = hauteur; }
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

        #[unsafe(method(ciblerRecherche:))]
        fn cibler_recherche(&self, _expediteur: Option<&NSObject>) {
            let ivars = self.ivars();
            if ivars.recherche.get().is_null() || ivars.fenetre.get().is_null() { return; }
            if let Some(surface) = ivars.surface.borrow().as_ref() { surface.reveler(); }
            let champ = unsafe { &*ivars.recherche.get() };
            unsafe { &*ivars.fenetre.get() }.makeKeyAndOrderFront(None);
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

        /// Un apercu decode hors du fil principal est pret.
        #[unsafe(method(apercuPret:))]
        fn apercu_pret(&self, _objet: Option<&NSObject>) {
            let dernier = self.ivars().apercu_reception.borrow().as_ref()
                .and_then(|reception| reception.try_iter().last());
            let Some(resultat) = dernier else { return; };
            if self.ivars().apercu_demande.borrow().as_ref() != Some(&resultat.cle) {
                return; // demande remplacee entre-temps
            }
            *self.ivars().apercu_demande.borrow_mut() = None;
            // Un fichier remplace pendant le decodage ne doit pas alimenter la memoire.
            let inchange = CleApercu::lire(&resultat.cle.chemin, resultat.cle.cible.0, resultat.cle.cible.1).as_ref() == Some(&resultat.cle);
            match resultat.vignette {
                Some(v) if inchange => {
                    let image = NSImage::initWithCGImage_size(NSImage::alloc(), &v.image,
                        NSSize { width: v.largeur as f64, height: v.hauteur as f64 });
                    journal::info(&format!("apercu de « {} » en {} ms (arriere-plan)",
                        resultat.cle.chemin.display(), resultat.duree_ms));
                    *self.ivars().apercu_memoire.borrow_mut() = Some(ApercuMemoire {
                        cle: resultat.cle, image, dimensions: resultat.dimensions,
                    });
                }
                Some(_) => {} // fichier modifie : le prochain rafraichissement relance
                None => {
                    journal::erreur(&format!("apercu impossible : {}", resultat.cle.chemin.display()));
                    *self.ivars().apercu_echec.borrow_mut() = Some(resultat.cle);
                }
            }
            self.rafraichir();
        }

        #[unsafe(method(vignettesPretes:))]
        fn vignettes_pretes(&self, _objet: Option<&NSObject>) {
            let pretes = {
                let etat = self.etat();
                etat.reception.as_ref().map(|reception| reception.try_iter()
                    .filter_map(|(index, vignette)| etat.images.get(index)
                        .map(|image| (index, image.chemin.clone(), vignette)))
                    .collect::<Vec<_>>()).unwrap_or_default()
            };
            for (index, chemin, vignette) in pretes {
                let image = vignette.map(|v| NSImage::initWithCGImage_size(NSImage::alloc(),
                    &v.image, NSSize { width: v.largeur as f64, height: v.hauteur as f64 }));
                let mut etat = self.etat_mut();
                if etat.images.get(index).is_some_and(|i| i.chemin == chemin) {
                    etat.vignettes_en_attente[index] = false;
                    etat.vignettes[index] = image;
                }
            }
            let (cellules, statut) = {
                let etat = self.etat();
                let cellules = etat.cellules.iter().map(|(&adresse, &index)| (
                    unsafe { &*(adresse as *const NSButton) }.retain(),
                    etat.vignettes[index].clone(), etat.vignettes_en_attente[index],
                )).collect::<Vec<_>>();
                (cellules, etat.statut_bibliotheque())
            };
            // Les boutons sont retenus par cet instantane pendant les callbacks.
            for (bouton, image, en_attente) in cellules {
                bouton.setImage(image.as_deref());
                bouton.setImageScaling(NSImageScaling::ScaleAxesIndependently);
                bouton.setTitle(&NSString::from_str(if image.is_some() { "" } else if en_attente { "…" } else { "(unreadable)" }));
            }
            unsafe { &*self.ivars().etat_texte.get() }.setStringValue(&NSString::from_str(&statut));
        }

        /// Clic sur une vignette : selection, apercu, informations.
        #[unsafe(method(celluleCliquee:))]
        fn cellule_cliquee(&self, expediteur: &NSButton) {
            let ivars = self.ivars();
            let mut etat = self.etat_mut();
            let cle = expediteur as *const NSButton as usize;
            let Some(&index) = etat.cellules.get(&cle) else {
                journal::erreur("clic sur une cellule inconnue (carte vide)");
                return;
            };
            etat.selection = Some(index);
            journal::info(&format!("selection : {}", etat.images[index].nom));
            drop(etat);
            self.rafraichir();
        }

        /// Bouton principal : applique l'image selectionnee a l'ecran courant.
        #[unsafe(method(appliquer:))]
        fn appliquer(&self, expediteur: &NSObject) {
            if self.chemin_action(expediteur).is_none() { return; }
            let ivars = self.ivars();
            let (chemin, cible, cibles, nom) = {
                let etat = self.etat();
                let Some(image) = etat.selection.and_then(|i| etat.images.get(i)) else {
                    journal::erreur("appliquer sans image selectionnee"); return;
                };
                (image.chemin.clone(), etat.ecran_courant,
                    etat.cibles_application().into_iter().cloned().collect::<Vec<_>>(), image.nom.clone())
            };
            journal::info(&format!(
                "application de « {} » sur l'ecran {}",
                chemin.display(),
                cible + 1
            ));
            let mut appliques = 0usize;
            let mut erreurs = Vec::new();
            let mut acceptes: Vec<(String, PathBuf)> = Vec::new();
            if cibles.is_empty() { journal::erreur("aucun ecran disponible"); return; }
            for e in cibles {
                let sequence = self.enregistrer_intention(&e.cle, &chemin);
                match ecrans::retrouver_identifiant(e.identifiant) {
                    Some(ecran) => match fond_ecran::appliquer(&chemin, &ecran) {
                        Ok(()) => {
                            appliques += 1;
                            acceptes.push((e.cle.clone(), chemin.clone()));
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
            self.etat_mut().enregistrer();
            // Une nouvelle application remplace toute propagation en cours.
            self.ivars().propagation_attente.borrow_mut().clear();
            self.ivars().propagation_pretes.borrow_mut().clear();
            if self.etat().reglages.tous_les_bureaux {
                *self.ivars().propagation_attente.borrow_mut() = acceptes;
            }
            self.relever_fonds_actifs();
            self.rafraichir();
            if self.ivars().suivi.borrow().est_vide() {
                self.arreter_relecture();
                if let Some(texte) = self.prendre_bilan() {
                    self.message(&texte);
                }
            } else if erreurs.is_empty() {
                self.message(&format!(
                    "Request accepted on {appliques} display(s), checking: {}",
                    nom
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
            let mut etat = self.etat_mut();
            let dossier = etat.dossier.clone();
            drop(etat);
            journal::info("commande : actualiser");
            self.charger_dossier(dossier);
        }

        /// Menu « Trier par nom ».
        #[unsafe(method(trierNom:))]
        fn trier_nom(&self, _expediteur: &NSObject) {
            let ivars = self.ivars();
            let mut etat = self.etat_mut();
            etat.tri = Tri::Nom;
            let dossier = etat.dossier.clone();
            drop(etat);
            journal::info("commande : tri par nom");
            self.charger_dossier(dossier);
        }

        /// Menu « Trier par date ».
        #[unsafe(method(trierDate:))]
        fn trier_date(&self, _expediteur: &NSObject) {
            let ivars = self.ivars();
            let mut etat = self.etat_mut();
            etat.tri = Tri::Date;
            let dossier = etat.dossier.clone();
            drop(etat);
            journal::info("commande : tri par date");
            self.charger_dossier(dossier);
        }

        /// Champ de recherche (valide par Entree).
        #[unsafe(method(rechercher:))]
        fn rechercher(&self, expediteur: &NSSearchField) {
            let ivars = self.ivars();
            let texte = expediteur.stringValue().to_string().to_lowercase();
            let mut etat = self.etat_mut();
            journal::info(&format!("recherche : « {texte} »"));
            etat.filtre = texte;
            drop(etat);
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
        let mut etat = self.etat_mut();
        if index >= etat.ecrans.len() {
            return;
        }
        etat.ecran_courant = index;
        let cle = etat.ecrans[index].cle.clone();
        let dossier = etat.reglages.dossier_ecran(&cle);
        etat.enregistrer();
        let dossier_courant = etat.dossier.clone();
        drop(etat);
        journal::info(&format!("ecran courant : {}", index + 1));
        if let Some(dossier) = dossier {
            if dossier != dossier_courant {
                self.charger_dossier(dossier);
                return;
            }
        }
        self.rafraichir();
    }


    /// Rafraichit le panneau seul, sans reconstruire la galerie.
    fn rafraichir_panneau(&self) {
        if self.ivars().etat.get().is_null() {
            return;
        }
        // Panneau jamais ouvert : inutile de cloner les reglages ni de calculer le plan.
        let panneau = self.ivars().panneau.borrow().clone();
        if let Some(panneau) = panneau {
            let etat = self.etat().instantane_reglages();
            panneau.actualiser(&etat);
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
            let mut etat = self.etat_mut();
            etat.reglages
                .dossiers_favoris
                .retain(|p| p != &chemin.to_string_lossy());
            etat.enregistrer();
        }
        self.rafraichir();
    }

    fn relever_fonds_actifs(&self) {
        let ecrans = self.etat().ecrans.clone();
        let actifs = ecrans.iter().filter_map(|e| {
            let ecran = ecrans::retrouver_identifiant(e.identifiant)?;
            fond_ecran::image_courante(&ecran).map(|p| p.to_string_lossy().into_owned())
        }).collect();
        self.etat_mut().fonds_actifs = actifs;
    }


    fn message(&self, texte: &str) {
        journal::info(texte);
        if !self.ivars().etat_texte.get().is_null() {
            unsafe { &*self.ivars().etat_texte.get() }.setStringValue(&NSString::from_str(texte));
        }
        if let Some(details) = self.ivars().details.borrow().as_ref() { details.setHidden(false); }
    }

    fn chemin_action(&self, expediteur: &NSObject) -> Option<PathBuf> {
        let contexte = expediteur
            .downcast_ref::<NSMenuItem>()
            .and_then(chemin_represente);
        let mut etat = self.etat_mut();
        let selection_avant = etat.selection;
        if let Some(chemin) = contexte {
            etat.selection = etat.images.iter().position(|i| i.chemin == chemin);
        }
        let chemin = etat
            .selection
            .and_then(|i| etat.images.get(i))
            .map(|i| i.chemin.clone());
        let selection_changee = selection_avant != etat.selection;
        drop(etat);
        if selection_changee {
            self.rafraichir();
        } else {
            self.rafraichir_selection();
        }
        chemin
    }

    fn candidats_avif(&self) -> Vec<PathBuf> {
        self.etat().images.iter()
            .filter(|i| crate::avif::est_candidate(&i.chemin))
            .map(|i| i.chemin.clone()).collect()
    }


    /// Recharge la galerie sans toucher au filtre de recherche.
    fn recharger_galerie(&self) {
        {
            let mut etat = self.etat_mut();
            etat.enregistrer();
            etat.recharger();
        }
        self.rafraichir_grille();
        self.rafraichir();
    }

    /// Demande confirmation avant suppression. Appele sans emprunt `RefCell`
    /// ni reference a l'etat : `runModal` peut rappeler l'application.
    fn confirmer_suppression(&self, nom: &str, avertissement: Option<String>) -> bool {
        match self.ivars().reponse_suppression.get() {
            0 => return false,
            1 => return true,
            _ => {}
        }
        let mtm = MainThreadMarker::new().expect("alerte sur le fil principal");
        let alerte = NSAlert::new(mtm);
        alerte.setMessageText(&NSString::from_str(&format!("Delete « {nom} » from disk?")));
        let mut detail = String::from("The file will be permanently deleted. This cannot be undone.");
        if let Some(avertissement) = avertissement {
            detail = format!("{avertissement}\n\n{detail}");
        }
        alerte.setInformativeText(&NSString::from_str(&detail));
        alerte.addButtonWithTitle(&NSString::from_str("Cancel"));
        alerte.addButtonWithTitle(&NSString::from_str("Delete"));
        let reponse = unsafe { alerte.runModal() };
        reponse == NSAlertSecondButtonReturn
    }


    fn nouvelle(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>();
        let this = this.set_ivars(CibleIvars {
            avif_panneau: RefCell::new(None),
            avif_tache: RefCell::new(None),
            avif_sources: RefCell::new(Vec::new()),
            etat: Cell::new(std::ptr::null_mut()),
            emprunts: Cell::new(0),
            reponse_suppression: Cell::new(-1),
            generation_grille: Cell::new(0),
            surface: RefCell::new(None),
            details: RefCell::new(None),
            apercu: Cell::new(std::ptr::null_mut()),
            apercu_memoire: RefCell::new(None),
            apercu_demande: RefCell::new(None),
            apercu_reception: RefCell::new(None),
            apercu_echec: RefCell::new(None),
            etat_texte: Cell::new(std::ptr::null_mut()),
            nom_fichier: Cell::new(std::ptr::null_mut()),
            dimensions: Cell::new(std::ptr::null_mut()),
            bouton_appliquer: Cell::new(std::ptr::null_mut()),
            document: Cell::new(std::ptr::null_mut()),
            defilement: Cell::new(std::ptr::null_mut()),
            recherche: Cell::new(std::ptr::null_mut()),
            menu: RefCell::new(None),
            segments: Cell::new(std::ptr::null_mut()),
            fenetre: Cell::new(std::ptr::null_mut()),
            favori: Cell::new(std::ptr::null_mut()),
            bouton_diaporama: Cell::new(std::ptr::null_mut()),
            bouton_suivante: Cell::new(std::ptr::null_mut()),
            apercu_plein: RefCell::new(None),
            minuterie: RefCell::new(None),
            suivi: RefCell::new(suivi::Suivi::new()),
            bilan: RefCell::new(suivi::Bilan::new()),
            relecture: RefCell::new(None),
            propagation_attente: RefCell::new(Vec::new()),
            propagation_pretes: RefCell::new(Vec::new()),
            panneau: RefCell::new(None),
        });
        unsafe { msg_send![super(this), init] }
    }

    /// Etat, avant que les vues existent.
    fn definir_etat(&self, etat: *mut Etat) {
        self.ivars().etat.set(etat);
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
        let ivars = self.ivars();
        ivars.apercu.set(apercu as *const NSImageView as *mut NSImageView);
        ivars.etat_texte.set(etat_texte as *const NSTextField as *mut NSTextField);
        ivars.nom_fichier.set(nom_fichier as *const NSTextField as *mut NSTextField);
        ivars.dimensions.set(dimensions as *const NSTextField as *mut NSTextField);
        ivars.bouton_appliquer.set(bouton_appliquer as *const NSButton as *mut NSButton);
        ivars.document.set(document as *const NSView as *mut NSView);
        ivars.defilement.set(defilement as *const NSScrollView as *mut NSScrollView);
        ivars.recherche.set(recherche as *const NSSearchField as *mut NSSearchField);
        *ivars.menu.borrow_mut() = Some(menu.retain());
    }

    /// Change de dossier : relit, regenere les vignettes, reconstruit la grille.
    fn charger_dossier(&self, dossier: PathBuf) {
        if !dossier.is_dir() {
            journal::erreur(&format!("dossier invalide : {}", dossier.display()));
            return;
        }
        {
            let mut etat = self.etat_mut();
            let cle = etat.ecrans.get(etat.ecran_courant).map(|e| e.cle.clone());
            etat.reglages.memoriser_dossier(&dossier, cle.as_deref());
            etat.dossier = dossier;
            etat.filtre.clear();
            etat.enregistrer();
            etat.recharger();
        }
        unsafe { &*self.ivars().recherche.get() }.setStringValue(&NSString::from_str(""));
        self.rafraichir_grille();
        self.rafraichir();
    }


}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------


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


/// La couche de selection a besoin du trait en portee pour ses methodes.
#[allow(dead_code)]
fn _garder(_couche: &CALayer, _objet: &dyn Fn() -> Option<Retained<Sel>>) {}

fn chemin_represente(element: &NSMenuItem) -> Option<PathBuf> {
    let objet = unsafe { element.representedObject() }?;
    objet
        .downcast_ref::<NSString>()
        .map(|s| PathBuf::from(s.to_string()))
}


/// Texte ajoute au dialogue de suppression quand l'image est encore utilisee.
fn avertissement_suppression(fond_actif: bool, dans_diaporama: bool) -> Option<String> {
    match (fond_actif, dans_diaporama) {
        (false, false) => None,
        (true, false) => Some("This image is currently a wallpaper on a display.".into()),
        (false, true) => Some("This image is part of a slideshow selection.".into()),
        (true, true) => Some(
            "This image is currently a wallpaper on a display and is part of a slideshow selection."
                .into(),
        ),
    }
}

#[cfg(test)]
mod tests_suppression {
    use super::avertissement_suppression;

    #[test]
    fn aucun_avertissement_si_l_image_n_est_pas_utilisee() {
        assert_eq!(avertissement_suppression(false, false), None);
    }

    #[test]
    fn fond_actif_et_diaporama_sont_signales() {
        assert!(avertissement_suppression(true, false).unwrap().contains("wallpaper"));
        assert!(avertissement_suppression(false, true).unwrap().contains("slideshow"));
        let les_deux = avertissement_suppression(true, true).unwrap();
        assert!(les_deux.contains("wallpaper") && les_deux.contains("slideshow"));
    }
}

