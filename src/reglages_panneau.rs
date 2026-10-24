//! Panneau de reglages natif (fenetre non modale, quatre onglets).
//!
//! Reprend les quatre onglets de `mural/app.py` (Display, Slideshow, Folders,
//! AVIF) dans une fenetre AppKit compacte. Aucun controle factice :
//!
//! - les cases et listes modifient les vrais reglages par les actions de
//!   l'application (ou par des wrappers typés quand l'action d'origine attend
//!   un `NSMenuItem`) ;
//! - `actualiser` ne fait que relire l'etat : elle ne sauvegarde rien et ne
//!   declenche aucune action ;
//! - les listes dynamiques (ecrans, participants, favoris, associations) ne
//!   sont reconstruites que lorsque leur signature change, pour ne pas perdre
//!   la selection ni reconstruire l'interface a chaque apercu.
//!
//! Les controles qui visent un ecran gardent son UUID ColorSync : jamais un
//! index. Un controle devenu obsolete apres une reenumeration n'est donc
//! jamais rattache a un autre ecran (il est ignore).

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSButtonType, NSFont, NSLineBreakMode,
    NSPopUpButton, NSScrollView, NSTabView, NSTabViewItem, NSTextField, NSView, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize, NSString};

use crate::app::Etat;

/// Largeur confortable du panneau.
pub const LARGEUR: f64 = 600.0;
/// Hauteur du panneau (contenu compact, listes defilantes).
pub const HAUTEUR: f64 = 500.0;

fn cadre(x: f64, y: f64, largeur: f64, hauteur: f64) -> NSRect {
    NSRect {
        origin: NSPoint { x, y },
        size: NSSize {
            width: largeur,
            height: hauteur,
        },
    }
}

fn etiquette(
    parent: &NSView,
    mtm: MainThreadMarker,
    texte: &str,
    x: f64,
    y: f64,
    largeur: f64,
    hauteur: f64,
) -> Retained<NSTextField> {
    let champ = NSTextField::labelWithString(&NSString::from_str(texte), mtm);
    champ.setFrame(cadre(x, y, largeur, hauteur));
    parent.addSubview(&champ);
    champ
}

fn titre(parent: &NSView, mtm: MainThreadMarker, texte: &str, y: f64) -> Retained<NSTextField> {
    let champ = etiquette(parent, mtm, texte, 10.0, y, 520.0, 22.0);
    champ.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    champ
}

fn texte_long(
    parent: &NSView,
    mtm: MainThreadMarker,
    texte: &str,
    x: f64,
    y: f64,
    largeur: f64,
    hauteur: f64,
) -> Retained<NSTextField> {
    let champ = NSTextField::wrappingLabelWithString(&NSString::from_str(texte), mtm);
    champ.setFrame(cadre(x, y, largeur, hauteur));
    parent.addSubview(&champ);
    champ
}

fn bouton(
    parent: &NSView,
    mtm: MainThreadMarker,
    cible: &NSObject,
    texte: &str,
    action: objc2::runtime::Sel,
    x: f64,
    y: f64,
    largeur: f64,
    hauteur: f64,
) -> Retained<NSButton> {
    let bouton = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(texte),
            Some(cible),
            Some(action),
            mtm,
        )
    };
    bouton.setFrame(cadre(x, y, largeur, hauteur));
    parent.addSubview(&bouton);
    bouton
}

fn case(
    parent: &NSView,
    mtm: MainThreadMarker,
    cible: &NSObject,
    texte: &str,
    action: objc2::runtime::Sel,
    x: f64,
    y: f64,
    largeur: f64,
) -> Retained<NSButton> {
    let bouton = bouton(parent, mtm, cible, texte, action, x, y, largeur, 22.0);
    bouton.setButtonType(NSButtonType::Switch);
    bouton
}

fn menu_item(
    mtm: MainThreadMarker,
    _cible: &NSObject,
    titre: &str,
    _action: objc2::runtime::Sel,
) -> Retained<objc2_app_kit::NSMenuItem> {
    let element = unsafe {
        objc2_app_kit::NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &NSString::from_str(titre),
            None,
            &NSString::from_str(""),
        )
    };
    element
}

/// Vue document simple pour les listes defilantes : les lignes sont posees
/// depuis le haut (coordonnees AppKit), sans dependre d'une vue retournee.
fn document_liste(mtm: MainThreadMarker, hauteur: f64) -> Retained<NSView> {
    let document = NSView::new(mtm);
    document.setFrame(cadre(0.0, 0.0, 520.0, hauteur.max(1.0)));
    document
}

fn y_depuis_haut(hauteur_document: f64, ligne: f64, hauteur_ligne: f64) -> f64 {
    hauteur_document - (ligne + 1.0) * hauteur_ligne
}

pub struct Panneau {
    pub fenetre: Retained<NSWindow>,
    onglets: Retained<NSTabView>,

    // Display
    ecrans: Retained<NSPopUpButton>,
    meme_image: Retained<NSButton>,
    tri_nom: Retained<NSButton>,
    tri_date: Retained<NSButton>,
    cle_affichee: Retained<NSTextField>,

    // Slideshow
    compteur: Retained<NSTextField>,
    demarrer: Retained<NSButton>,
    suivante: Retained<NSButton>,
    independante: Retained<NSButton>,
    aleatoire: Retained<NSButton>,
    intervalle: Retained<NSPopUpButton>,
    filtre: Retained<NSButton>,
    participants_vue: Retained<NSView>,
    participants: RefCell<Vec<(String, Retained<NSButton>)>>,

    // Folders
    dossier_courant: Retained<NSTextField>,
    favori: Retained<NSButton>,
    favoris: Retained<NSPopUpButton>,
    retirer_favori: Retained<NSButton>,
    associations_vue: Retained<NSView>,
    associations: RefCell<Vec<(String, Retained<NSTextField>, Retained<NSButton>)>>,

    // AVIF
    avif_capacites: Retained<NSTextField>,
    avif_cache: Retained<NSTextField>,
    avif_ouvrir: Retained<NSButton>,

    /// Cible AppKit des actions (vit jusqu'a la fin du processus).
    cible: *const NSObject,

    // Signatures : reconstruire uniquement quand la donnee change.
    signature_ecrans: Cell<String>,
    signature_participants: Cell<String>,
    signature_intervalle: Cell<u64>,
    signature_favoris: Cell<String>,
    signature_associations: Cell<String>,
}

impl Panneau {
    pub fn nouveau(mtm: MainThreadMarker, cible: &NSObject, etat: &Etat) -> Self {
        let fenetre = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                cadre(0.0, 0.0, LARGEUR, HAUTEUR),
                NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe { fenetre.setReleasedWhenClosed(false) };
        fenetre.setTitle(&NSString::from_str("Settings"));
        fenetre.center();
        fenetre.setContentMinSize(NSSize {
            width: LARGEUR,
            height: HAUTEUR,
        });
        let contenu = fenetre.contentView().expect("Settings content view");

        let onglets = NSTabView::new(mtm);
        onglets.setFrame(cadre(10.0, 10.0, LARGEUR - 20.0, HAUTEUR - 20.0));
        contenu.addSubview(&onglets);

        // --- Onglet Display -------------------------------------------------
        let display = NSView::new(mtm);
        titre(&display, mtm, "Display", 366.0);
        etiquette(
            &display,
            mtm,
            "Current display",
            10.0,
            338.0,
            200.0,
            18.0,
        );
        let ecrans = NSPopUpButton::initWithFrame_pullsDown(
            NSPopUpButton::alloc(mtm),
            cadre(210.0, 334.0, 320.0, 26.0),
            false,
        );
        unsafe {
            ecrans.setTarget(Some(cible));
            ecrans.setAction(Some(sel!(ecranPanneau:)));
        }
        display.addSubview(&ecrans);
        let cle_affichee = etiquette(&display, mtm, "", 10.0, 310.0, 520.0, 18.0);
        cle_affichee.setTextColor(Some(&objc2_app_kit::NSColor::secondaryLabelColor()));
        let meme_image = case(
            &display,
            mtm,
            cible,
            "Same image on all displays",
            sel!(memeImagePartout:),
            10.0,
            278.0,
            520.0,
        );
        titre(&display, mtm, "Sort images", 238.0);
        let tri_nom = bouton(
            &display,
            mtm,
            cible,
            "Name",
            sel!(trierNom:),
            10.0,
            208.0,
            100.0,
            24.0,
        );
        tri_nom.setButtonType(NSButtonType::Radio);
        let tri_date = bouton(
            &display,
            mtm,
            cible,
            "Date",
            sel!(trierDate:),
            120.0,
            208.0,
            100.0,
            24.0,
        );
        tri_date.setButtonType(NSButtonType::Radio);
        texte_long(
            &display,
            mtm,
            "Each display keeps its own folder and wallpaper. Choosing a display here also updates the main window. Same image on all displays applies the selected image everywhere.",
            10.0,
            90.0,
            520.0,
            100.0,
        );

        // --- Onglet Slideshow ----------------------------------------------
        let diaporama = NSView::new(mtm);
        titre(&diaporama, mtm, "Slideshow", 366.0);
        let compteur = etiquette(&diaporama, mtm, "", 10.0, 336.0, 520.0, 20.0);
        compteur.setTextColor(Some(&objc2_app_kit::NSColor::secondaryLabelColor()));
        let demarrer = bouton(
            &diaporama,
            mtm,
            cible,
            "Start",
            sel!(basculerDiaporama:),
            10.0,
            296.0,
            120.0,
            30.0,
        );
        let suivante = bouton(
            &diaporama,
            mtm,
            cible,
            "Next",
            sel!(suivanteDiaporama:),
            140.0,
            296.0,
            100.0,
            30.0,
        );
        let independante = case(
            &diaporama,
            mtm,
            cible,
            "Independent selection per display",
            sel!(selectionIndependante:),
            10.0,
            262.0,
            520.0,
        );
        let aleatoire = case(
            &diaporama,
            mtm,
            cible,
            "Random order",
            sel!(aleatoireDiaporama:),
            10.0,
            234.0,
            520.0,
        );
        etiquette(&diaporama, mtm, "Interval", 10.0, 202.0, 80.0, 18.0);
        let intervalle = NSPopUpButton::initWithFrame_pullsDown(
            NSPopUpButton::alloc(mtm),
            cadre(100.0, 198.0, 220.0, 26.0),
            false,
        );
        unsafe {
            intervalle.setTarget(Some(cible));
            intervalle.setAction(Some(sel!(intervallePanneau:)));
        }
        diaporama.addSubview(&intervalle);
        etiquette(
            &diaporama,
            mtm,
            "Participating displays",
            10.0,
            166.0,
            220.0,
            18.0,
        );
        let defilement = NSScrollView::new(mtm);
        defilement.setFrame(cadre(10.0, 82.0, 520.0, 78.0));
        defilement.setHasVerticalScroller(true);
        defilement.setAutohidesScrollers(true);
        defilement.setDrawsBackground(false);
        let participants_vue = document_liste(mtm, 78.0);
        defilement.setDocumentView(Some(&participants_vue));
        diaporama.addSubview(&defilement);
        let filtre = case(
            &diaporama,
            mtm,
            cible,
            "Show slideshow images only",
            sel!(filtrerDiaporama:),
            10.0,
            50.0,
            520.0,
        );
        bouton(
            &diaporama,
            mtm,
            cible,
            "Add Folder to Selection",
            sel!(ajouterDossierDiaporama:),
            10.0,
            14.0,
            190.0,
            28.0,
        );
        bouton(
            &diaporama,
            mtm,
            cible,
            "Remove Folder from Selection",
            sel!(retirerDossierDiaporama:),
            210.0,
            14.0,
            210.0,
            28.0,
        );

        // --- Onglet Folders -------------------------------------------------
        let dossiers = NSView::new(mtm);
        titre(&dossiers, mtm, "Folders", 366.0);
        etiquette(&dossiers, mtm, "Current folder", 10.0, 340.0, 120.0, 18.0);
        let dossier_courant = etiquette(&dossiers, mtm, "", 10.0, 318.0, 520.0, 18.0);
        dossier_courant.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
        dossier_courant.setTextColor(Some(&objc2_app_kit::NSColor::secondaryLabelColor()));
        bouton(
            &dossiers,
            mtm,
            cible,
            "Open Folder…",
            sel!(ouvrirDossier:),
            10.0,
            280.0,
            130.0,
            30.0,
        );
        let favori = bouton(
            &dossiers,
            mtm,
            cible,
            "Add to Favorites",
            sel!(dossierFavori:),
            150.0,
            280.0,
            150.0,
            30.0,
        );
        etiquette(&dossiers, mtm, "Favorite folders", 10.0, 248.0, 180.0, 18.0);
        let favoris = NSPopUpButton::initWithFrame_pullsDown(
            NSPopUpButton::alloc(mtm),
            cadre(10.0, 218.0, 380.0, 26.0),
            false,
        );
        unsafe {
            favoris.setTarget(Some(cible));
            favoris.setAction(Some(sel!(favoriPanneau:)));
        }
        dossiers.addSubview(&favoris);
        let retirer_favori = bouton(
            &dossiers,
            mtm,
            cible,
            "Remove Favorite",
            sel!(retirerFavoriPanneau:),
            400.0,
            216.0,
            130.0,
            30.0,
        );
        etiquette(
            &dossiers,
            mtm,
            "Folder used by each display (current folder stays unchanged)",
            10.0,
            186.0,
            520.0,
            18.0,
        );
        let defilement_dossiers = NSScrollView::new(mtm);
        defilement_dossiers.setFrame(cadre(10.0, 34.0, 520.0, 146.0));
        defilement_dossiers.setHasVerticalScroller(true);
        defilement_dossiers.setAutohidesScrollers(true);
        defilement_dossiers.setDrawsBackground(false);
        let associations_vue = document_liste(mtm, 146.0);
        defilement_dossiers.setDocumentView(Some(&associations_vue));
        dossiers.addSubview(&defilement_dossiers);

        // --- Onglet AVIF ----------------------------------------------------
        let avif = NSView::new(mtm);
        titre(&avif, mtm, "AVIF", 366.0);
        let avif_capacites = texte_long(&avif, mtm, "", 10.0, 300.0, 520.0, 56.0);
        let avif_cache = etiquette(&avif, mtm, "", 10.0, 272.0, 520.0, 18.0);
        avif_cache.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
        avif_cache.setTextColor(Some(&objc2_app_kit::NSColor::secondaryLabelColor()));
        texte_long(
            &avif,
            mtm,
            "The AVIF panel converts the current folder into compressed AVIF copies stored in Mural’s cache. Originals never change, and copies are reused when they are still valid. No wallpaper is applied.",
            10.0,
            130.0,
            520.0,
            120.0,
        );
        let avif_ouvrir = bouton(
            &avif,
            mtm,
            cible,
            "Open AVIF Conversion…",
            sel!(cacheAvif:),
            10.0,
            80.0,
            210.0,
            32.0,
        );

        for (vue, libelle) in [
            (&display, "Display"),
            (&diaporama, "Slideshow"),
            (&dossiers, "Folders"),
            (&avif, "AVIF"),
        ] {
            vue.setFrame(cadre(0.0, 0.0, 560.0, 400.0));
            let element = unsafe {
                NSTabViewItem::initWithIdentifier(
                    mtm.alloc::<NSTabViewItem>(),
                    Some(&*NSString::from_str(libelle)),
                )
            };
            element.setLabel(&NSString::from_str(libelle));
            element.setView(Some(vue));
            onglets.addTabViewItem(&element);
        }
        onglets.selectTabViewItemAtIndex(0);

        let panneau = Self {
            fenetre,
            onglets,
            cible: cible as *const NSObject,
            ecrans,
            meme_image,
            tri_nom,
            tri_date,
            cle_affichee,
            compteur,
            demarrer,
            suivante,
            independante,
            aleatoire,
            intervalle,
            filtre,
            participants_vue,
            participants: RefCell::new(Vec::new()),
            dossier_courant,
            favori,
            favoris,
            retirer_favori,
            associations_vue,
            associations: RefCell::new(Vec::new()),
            avif_capacites,
            avif_cache,
            avif_ouvrir,
            signature_ecrans: Cell::new(String::new()),
            signature_participants: Cell::new(String::new()),
            signature_intervalle: Cell::new(u64::MAX),
            signature_favoris: Cell::new(String::new()),
            signature_associations: Cell::new(String::new()),
        };
        panneau.actualiser(etat);
        panneau
    }

    /// Relit l'etat et met les controles a jour. N'ecrit jamais les reglages
    /// et n'appelle aucune action (les `setState` programmatiques n'en
    /// declenchent pas).
    pub fn actualiser(&self, etat: &Etat) {
        self.actualiser_ecrans(etat);
        self.actualiser_diaporama(etat);
        self.actualiser_dossiers(etat);
        self.actualiser_avif();
    }

    fn actualiser_ecrans(&self, etat: &Etat) {
        let signature = etat
            .ecrans
            .iter()
            .map(|e| e.cle.clone())
            .collect::<Vec<_>>()
            .join("\u{1f}");
        let ancienne = self.signature_ecrans.replace(signature.clone());
        if ancienne != signature || self.ecrans.numberOfItems() == 0 {
            self.reconstruire_ecrans(etat);
        }
        // La selection courante est relue a chaque fois : elle peut avoir
        // change depuis la barre principale ou les menus.
        let clecourante = etat.cle_ecran_courant().map(str::to_owned);
        let mut selection = -1isize;
        for index in 0..self.ecrans.numberOfItems() {
            if let Some(item) = self.ecrans.itemAtIndex(index) {
                let cle = item
                    .representedObject()
                    .and_then(|o| o.downcast_ref::<NSString>().map(|s| s.to_string()));
                if cle.is_some() && cle == clecourante {
                    selection = index;
                }
            }
        }
        if selection < 0 && !etat.ecrans.is_empty() {
            selection = etat.ecran_courant as isize;
        }
        self.ecrans.selectItemAtIndex(selection);
        self.cle_affichee.setStringValue(&NSString::from_str(
            &etat.ecrans.get(etat.ecran_courant)
                .map(|e| format!("{} × {} points", e.largeur, e.hauteur))
                .unwrap_or_else(|| "No display".to_string()),
        ));
        self.meme_image
            .setState(if etat.reglages.meme_image_partout { 1 } else { 0 });
        self.tri_nom.setState(if etat.tri == crate::bibliotheque::Tri::Nom {
            1
        } else {
            0
        });
        self.tri_date.setState(if etat.tri == crate::bibliotheque::Tri::Date {
            1
        } else {
            0
        });
    }

    fn reconstruire_ecrans(&self, etat: &Etat) {
        let mtm = MainThreadMarker::new().expect("panneau sur le fil principal");
        self.ecrans.removeAllItems();
        if etat.ecrans.is_empty() {
            self.ecrans.addItemWithTitle(&NSString::from_str("No display"));
            self.ecrans.setEnabled(false);
            return;
        }
        self.ecrans.setEnabled(true);
        let Some(menu) = self.ecrans.menu() else { return; };
        for ecran in &etat.ecrans {
            let element = menu_item(
                mtm,
                unsafe { &*self.cible },
                &format!("Display {} — {}", ecran.index + 1, ecran.nom),
                sel!(ecranPanneau:),
            );
            unsafe {
                element.setRepresentedObject(Some(&NSString::from_str(&ecran.cle)));
            }
            element.setTag(ecran.index as isize);
            element.setToolTip(Some(&NSString::from_str(&ecran.cle)));
            menu.addItem(&element);
        }
    }

    fn actualiser_diaporama(&self, etat: &Etat) {
        // Compteur de la bonne liste et etat de lecture.
        let compteur = etat
            .reglages
            .favoris_selection(etat.cle_ecran_courant())
            .len();
        let mode = if etat.reglages.selection_independante {
            "independent lists"
        } else {
            "shared list"
        };
        self.compteur.setStringValue(&NSString::from_str(&format!(
            "{compteur} image(s) · {mode} · {}",
            if etat.diaporama_actif {
                "playing"
            } else {
                "paused"
            }
        )));
        let plan_vide = etat.plan_diaporama().is_empty();
        self.demarrer
            .setTitle(&NSString::from_str(if etat.diaporama_actif {
                "Pause"
            } else {
                "Start"
            }));
        self.demarrer.setEnabled(etat.diaporama_actif || !plan_vide);
        self.suivante.setEnabled(!plan_vide);
        self.independante
            .setState(if etat.reglages.selection_independante {
                1
            } else {
                0
            });
        self.aleatoire
            .setState(if etat.reglages.diaporama_aleatoire {
                1
            } else {
                0
            });
        self.filtre
            .setState(if etat.filtre_diaporama { 1 } else { 0 });

        // Intervalle : presets + valeur existante hors presets conservee.
        let secondes = etat.reglages.diaporama_intervalle.clamp(60, 86_400);
        let ancien = self.signature_intervalle.replace(secondes);
        if ancien != secondes || self.intervalle.numberOfItems() == 0 {
            self.reconstruire_intervalle(secondes);
        }
        self.intervalle.selectItemWithTag(secondes as isize);

        // Participants : uniquement si les ecrans ou la participation changent.
        let signature = format!(
            "{}|{}",
            etat.ecrans
                .iter()
                .map(|e| e.cle.clone())
                .collect::<Vec<_>>()
                .join("\u{1f}"),
            etat.reglages
                .diaporama_ecrans
                .as_ref()
                .map(|cles| cles.join("\u{1f}"))
                .unwrap_or_else(|| "*".to_string())
        );
        let ancienne = self.signature_participants.replace(signature.clone());
        if ancienne != signature || self.participants.borrow().len() != etat.ecrans.len() {
            self.reconstruire_participants(etat);
        }
        for (cle, bouton) in self.participants.borrow().iter() {
            let participe = etat
                .reglages
                .diaporama_ecrans
                .as_ref()
                .map(|cles| cles.contains(cle))
                .unwrap_or(true);
            bouton.setState(if participe { 1 } else { 0 });
        }
    }

    fn reconstruire_intervalle(&self, secondes: u64) {
        let mtm = MainThreadMarker::new().expect("panneau sur le fil principal");
        let cible = unsafe { &*self.cible };
        self.intervalle.removeAllItems();
        let Some(menu) = self.intervalle.menu() else { return; };
        let mut presets = vec![60u64, 300, 900, 1800, 3600];
        if !presets.contains(&secondes) {
            presets.push(secondes);
        }
        for valeur in presets {
            let libelle = if valeur == secondes && ![60, 300, 900, 1800, 3600].contains(&valeur) {
                format!("Custom — {} s (kept)", valeur)
            } else if valeur < 60 {
                format!("{} s", valeur)
            } else {
                format!("{} min", valeur / 60)
            };
            let element = menu_item(mtm, cible, &libelle, sel!(intervallePanneau:));
            element.setTag(valeur as isize);
            menu.addItem(&element);
        }
    }

    fn reconstruire_participants(&self, etat: &Etat) {
        let mtm = MainThreadMarker::new().expect("panneau sur le fil principal");
        // Les anciens boutons sont detaches avant d'etre remplaces.
        for sous_vue in self.participants_vue.subviews().iter() {
            sous_vue.removeFromSuperview();
        }
        let mut boutons = Vec::new();
        let hauteur_ligne = 24.0;
        let lignes = etat.ecrans.len().max(1) as f64;
        let hauteur_document = (lignes * hauteur_ligne).max(78.0);
        self.participants_vue
            .setFrame(cadre(0.0, 0.0, 520.0, hauteur_document));
        let cible = unsafe { &*self.cible };
        for (index, ecran) in etat.ecrans.iter().enumerate() {
            let y = y_depuis_haut(hauteur_document, index as f64, hauteur_ligne);
            let bouton = bouton(
                &self.participants_vue,
                mtm,
                cible,
                &format!("Display {} — {}", ecran.index + 1, ecran.nom),
                sel!(participantPanneau:),
                0.0,
                y,
                510.0,
                20.0,
            );
            bouton.setButtonType(NSButtonType::Switch);
            bouton.setToolTip(Some(&NSString::from_str(&format!(
                "Include {} in the slideshow",
                ecran.nom
            ))));
            boutons.push((ecran.cle.clone(), bouton));
        }
        *self.participants.borrow_mut() = boutons;
    }

    fn actualiser_dossiers(&self, etat: &Etat) {
        let chemin = etat.dossier.to_string_lossy().into_owned();
        self.dossier_courant
            .setStringValue(&NSString::from_str(&chemin));
        self.dossier_courant
            .setToolTip(Some(&NSString::from_str(&chemin)));
        let favori = etat.reglages.dossiers_favoris.contains(&chemin);
        self.favori.setTitle(&NSString::from_str(if favori {
            "Remove from Favorites"
        } else {
            "Add to Favorites"
        }));
        self.favori.setToolTip(Some(&NSString::from_str(if favori {
            "Remove the current folder from favorites"
        } else {
            "Add the current folder to favorites"
        })));

        let signature_favoris = etat.reglages.dossiers_favoris.join("\u{1f}");
        let ancienne = self.signature_favoris.replace(signature_favoris.clone());
        if ancienne != signature_favoris || self.favoris.numberOfItems() == 0 {
            self.reconstruire_favoris(etat);
        }
        // Garder une selection coherente, sans action.
        let index = etat
            .reglages
            .dossiers_favoris
            .iter()
            .position(|p| p == &chemin)
            .map(|i| i as isize)
            .unwrap_or_else(|| self.favoris.indexOfSelectedItem());
        let dernier = (self.favoris.numberOfItems() - 1).max(0) as isize;
        self.favoris.selectItemAtIndex(index.clamp(0, dernier));
        self.favoris.setEnabled(!etat.reglages.dossiers_favoris.is_empty());
        self.retirer_favori.setEnabled(!etat.reglages.dossiers_favoris.is_empty());

        let signature_associations = format!(
            "{}|{}",
            etat.ecrans
                .iter()
                .map(|e| e.cle.clone())
                .collect::<Vec<_>>()
                .join("\u{1f}"),
            etat.reglages
                .dossier_par_ecran
                .iter()
                .map(|(cle, dossier)| format!("{cle}={dossier}"))
                .collect::<Vec<_>>()
                .join("\u{1f}")
        );
        let ancienne = self.signature_associations.replace(signature_associations.clone());
        if ancienne != signature_associations || self.associations.borrow().len() != etat.ecrans.len() {
            self.reconstruire_associations(etat);
        }
        for (cle, libelle, _) in self.associations.borrow().iter() {
            let valeur = etat
                .reglages
                .dossier_par_ecran
                .get(cle)
                .cloned()
                .unwrap_or_else(|| "—".to_string());
            let nom = std::path::Path::new(&valeur)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| valeur.clone());
            libelle.setStringValue(&NSString::from_str(&nom));
            libelle.setToolTip(Some(&NSString::from_str(&valeur)));
        }
    }

    fn reconstruire_favoris(&self, etat: &Etat) {
        let mtm = MainThreadMarker::new().expect("panneau sur le fil principal");
        self.favoris.removeAllItems();
        let Some(menu) = self.favoris.menu() else { return; };
        let cible = unsafe { &*self.cible };
        if etat.reglages.dossiers_favoris.is_empty() {
            let element = menu_item(mtm, &cible, "No favorite folder", sel!(favoriPanneau:));
            element.setEnabled(false);
            menu.addItem(&element);
            return;
        }
        for texte in &etat.reglages.dossiers_favoris {
            let element = menu_item(mtm, &cible, texte, sel!(favoriPanneau:));
            unsafe {
                element.setRepresentedObject(Some(&NSString::from_str(texte)));
            }
            element.setToolTip(Some(&NSString::from_str(texte)));
            menu.addItem(&element);
        }
    }

    fn reconstruire_associations(&self, etat: &Etat) {
        let mtm = MainThreadMarker::new().expect("panneau sur le fil principal");
        for sous_vue in self.associations_vue.subviews().iter() {
            sous_vue.removeFromSuperview();
        }
        let cible = unsafe { &*self.cible };
        let hauteur_ligne = 26.0;
        let hauteur_document = ((etat.ecrans.len().max(1) as f64) * hauteur_ligne).max(146.0);
        self.associations_vue
            .setFrame(cadre(0.0, 0.0, 520.0, hauteur_document));
        let mut lignes = Vec::new();
        for (index, ecran) in etat.ecrans.iter().enumerate() {
            let y = y_depuis_haut(hauteur_document, index as f64, hauteur_ligne);
            etiquette(
                &self.associations_vue,
                mtm,
                &format!("Display {} — {}", ecran.index + 1, ecran.nom),
                0.0,
                y + 4.0,
                190.0,
                18.0,
            );
            let valeur = etat
                .reglages
                .dossier_par_ecran
                .get(&ecran.cle)
                .cloned()
                .unwrap_or_else(|| "—".to_string());
            let libelle = etiquette(&self.associations_vue, mtm, "", 196.0, y + 4.0, 160.0, 18.0);
            libelle.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
            let nom = std::path::Path::new(&valeur)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| valeur.clone());
            libelle.setStringValue(&NSString::from_str(&nom));
            libelle.setToolTip(Some(&NSString::from_str(&valeur)));
            let bouton = bouton(
                &self.associations_vue,
                mtm,
                cible,
                "Use Current Folder",
                sel!(dossierEcranPanneau:),
                362.0,
                y,
                150.0,
                24.0,
            );
            bouton.setToolTip(Some(&NSString::from_str(&format!(
                "Use the current folder for Display {} — {}",
                ecran.index + 1,
                ecran.nom
            ))));
            lignes.push((ecran.cle.clone(), libelle, bouton));
        }
        *self.associations.borrow_mut() = lignes;
    }

    fn actualiser_avif(&self) {
        let lecture = if crate::formats::avif_lisible() {
            "available"
        } else {
            "not available"
        };
        let encodage = if crate::formats::avif_encodable() {
            "available"
        } else {
            "not available"
        };
        self.avif_capacites.setStringValue(&NSString::from_str(&format!(
            "AVIF reading: {lecture}\nAVIF encoding: {encodage}"
        )));
        let cache = crate::avif::dossier_cache();
        let texte = cache
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Cache unavailable".to_string());
        self.avif_cache.setStringValue(&NSString::from_str(&texte));
        self.avif_cache
            .setToolTip(Some(&NSString::from_str(&texte)));
        self.avif_ouvrir
            .setEnabled(cache.is_some() && crate::formats::avif_encodable());
    }

    /// Cle UUID d'un controle (participant ou association), par identite
    /// d'objet. Aucun index n'est utilise : un ancien controle retire de la
    /// liste ne correspond plus a aucun ecran et son action est ignoree.
    pub fn cle_pour_bouton(&self, expediteur: &NSButton) -> Option<String> {
        let adresse = expediteur as *const NSButton as usize;
        for (cle, bouton) in self.participants.borrow().iter() {
            if &**bouton as *const NSButton as usize == adresse {
                return Some(cle.clone());
            }
        }
        for (cle, _, bouton) in self.associations.borrow().iter() {
            if &**bouton as *const NSButton as usize == adresse {
                return Some(cle.clone());
            }
        }
        None
    }

    /// Chemin choisi dans la liste des favoris (aucune action declenchee).
    pub fn favori_selectionne(&self) -> Option<String> {
        let item = self.favoris.selectedItem()?;
        item.representedObject()
            .and_then(|o| o.downcast_ref::<NSString>().map(|s| s.to_string()))
    }

    /// Instantane des controles reels (pour la recette AppKit). Lit les vues,
    /// pas l'etat interne.
    pub fn instantane(&self) -> String {
        let onglets: Vec<String> = self
            .onglets
            .tabViewItems()
            .iter()
            .map(|i| i.label().to_string())
            .collect();
        let onglet = self.onglets.indexOfTabViewItem(
            &self
                .onglets
                .selectedTabViewItem()
                .expect("onglet selectionne"),
        );
        let mut ecrans = Vec::new();
        for index in 0..self.ecrans.numberOfItems() {
            if let Some(item) = self.ecrans.itemAtIndex(index) {
                ecrans.push(serde_json::json!({
                    "titre": item.title().to_string(),
                    "cle": item.representedObject()
                        .and_then(|o| o.downcast_ref::<NSString>().map(|s| s.to_string())),
                    "tag": item.tag(),
                }));
            }
        }
        let participants: Vec<_> = self
            .participants
            .borrow()
            .iter()
            .map(|(cle, bouton)| {
                serde_json::json!({
                    "cle": cle,
                    "titre": bouton.title().to_string(),
                    "etat": bouton.state(),
                    "actif": bouton.isEnabled(),
                })
            })
            .collect();
        let associations: Vec<_> = self
            .associations
            .borrow()
            .iter()
            .map(|(cle, libelle, bouton)| {
                serde_json::json!({
                    "cle": cle,
                    "texte": libelle.stringValue().to_string(),
                    "infobulle": libelle.toolTip().map(|s| s.to_string()),
                    "bouton_infobulle": bouton.toolTip().map(|s| s.to_string()),
                    "bouton_actif": bouton.isEnabled(),
                })
            })
            .collect();
        let cible = unsafe { &*self.cible };
        let _ = cible;
        let favoris: Vec<_> = self
            .favoris
            .menu()
            .map(|menu| {
                menu.itemArray()
                    .iter()
                    .map(|item| {
                        serde_json::json!({
                            "titre": item.title().to_string(),
                            "cle": item.representedObject()
                                .and_then(|o| o.downcast_ref::<NSString>().map(|s| s.to_string())),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        serde_json::json!({
            "onglet": onglet,
            "onglets": onglets,
            "ecran_selectionne": self.ecrans.indexOfSelectedItem(),
            "ecrans": ecrans,
            "cle_affichee": self.cle_affichee.stringValue().to_string(),
            "meme_image": self.meme_image.state(),
            "tri_nom": self.tri_nom.state(),
            "tri_date": self.tri_date.state(),
            "compteur": self.compteur.stringValue().to_string(),
            "demarrer": {
                "titre": self.demarrer.title().to_string(),
                "actif": self.demarrer.isEnabled(),
            },
            "suivante_active": self.suivante.isEnabled(),
            "independante": self.independante.state(),
            "aleatoire": self.aleatoire.state(),
            "intervalle": self.intervalle.selectedTag(),
            "intervalle_titre": self.intervalle.selectedItem()
                .map(|i| i.title().to_string()),
            "filtre": self.filtre.state(),
            "participants": participants,
            "dossier_courant": self.dossier_courant.stringValue().to_string(),
            "dossier_infobulle": self.dossier_courant.toolTip().map(|s| s.to_string()),
            "favori_titre": self.favori.title().to_string(),
            "favoris": favoris,
            "favori_selectionne": self.favoris.indexOfSelectedItem(),
            "associations": associations,
            "avif_capacites": self.avif_capacites.stringValue().to_string(),
            "avif_cache": self.avif_cache.stringValue().to_string(),
            "avif_bouton_actif": self.avif_ouvrir.isEnabled(),
            "fenetre_visible": self.fenetre.isVisible(),
        })
        .to_string()
    }
}
