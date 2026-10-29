//! Recette du lot bibliotheque, preferences et images factices uniquement.
//! Usage : MURAL_REGLAGES=/private/tmp/.../reglages.json recette_bibliotheque [--menus|--restauration]
//! N’appelle jamais appliquer: ni suivanteDiaporama: et n’utilise pas le
//! presse-papiers general. Le mode --menus cree ses propres images factices
//! dans le dossier de MURAL_REGLAGES (aucune fixture d’une autre recette).
use mural::{app::Etat, bibliotheque::Tri, preferences};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSButton, NSImageView, NSMenu, NSMenuItem, NSPopUpButton, NSSearchField,
    NSSegmentedControl, NSTabView, NSView, NSWindowStyleMask,
};
use objc2_foundation::{NSDate, NSNotification, NSObject, NSObjectProtocol, NSString, NSTimer};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn attendre(mut condition: impl FnMut() -> bool) {
    let fin = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < fin, "delai depasse");
        let app = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
        if let Some(evenement) = app.nextEventMatchingMask_untilDate_inMode_dequeue(
            objc2_app_kit::NSEventMask::Any,
            Some(&NSDate::dateWithTimeIntervalSinceNow(0.02)),
            unsafe { objc2_foundation::NSDefaultRunLoopMode },
            true,
        ) {
            app.sendEvent(&evenement);
        }
        app.updateWindows();
    }
}
fn objet(chemin: &Path, mtm: MainThreadMarker) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &NSString::from_str("recette"),
            None,
            &NSString::from_str(""),
        )
    };
    unsafe {
        item.setRepresentedObject(Some(&NSString::from_str(&chemin.to_string_lossy())));
    }
    item
}
fn trouver<T: objc2::Message + objc2::DowncastTarget>(vue: &NSView) -> Option<Retained<T>> {
    if let Some(v) = vue.downcast_ref::<T>() {
        return Some(v.retain());
    }
    for enfant in vue.subviews() {
        if let Some(v) = trouver::<T>(&enfant) {
            return Some(v);
        }
    }
    None
}
fn bouton_action(vue: &NSView, action: objc2::runtime::Sel) -> Option<Retained<NSButton>> {
    if let Some(bouton) = vue.downcast_ref::<NSButton>() {
        if unsafe { bouton.action() } == Some(action) {
            return Some(bouton.retain());
        }
    }
    for enfant in vue.subviews() {
        if let Some(bouton) = bouton_action(&enfant, action) {
            return Some(bouton);
        }
    }
    None
}
fn cible(etat: *mut Etat) -> Retained<AnyObject> {
    let adresse = *unsafe { &*etat }.cellules.keys().next().expect("cellules");
    unsafe { &*(adresse as *const NSButton) }
        .target()
        .expect("cible")
}
fn verifier_noms_vignettes(etat: *mut Etat) {
    for (adresse, index) in &unsafe { &*etat }.cellules {
        let bouton = unsafe { &*(*adresse as *const NSButton) };
        let label: Option<Retained<NSString>> = unsafe { msg_send![bouton, accessibilityLabel] };
        assert_eq!(label.map(|s| s.to_string()).as_deref(), Some(unsafe { &*etat }.images[*index].nom.as_str()), "vignette sans nom accessible");
    }
}

fn ouvrir(cible: &AnyObject, dossier: &Path, mtm: MainThreadMarker) {
    let item = objet(dossier, mtm);
    let _: () = unsafe { msg_send![cible, ouvrirFavori: &*item] };
}

// ---------------------------------------------------------------------------
// Mode --menus : observation AppKit des menus réels, sans validation textuelle.
// ---------------------------------------------------------------------------

struct ObservateurIvars {
    /// Menus vus par NSMenuDidAddItemNotification, pour retrouver le vrai menu
    /// construit par le produit (aucun accès à un ivar privé).
    menus: RefCell<Vec<Retained<NSMenu>>>,
    debuts: Cell<usize>,
    fins: Cell<usize>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "MuralRecetteMenus"]
    #[thread_kind = MainThreadOnly]
    #[ivars = ObservateurIvars]
    struct Observateur;

    unsafe impl NSObjectProtocol for Observateur {}

    impl Observateur {
        #[unsafe(method(menuModifie:))]
        fn menu_modifie(&self, notification: &NSNotification) {
            let nom = notification.name();
            if &*nom == unsafe { objc2_app_kit::NSMenuDidAddItemNotification } {
                if let Some(objet) = notification.object()
                    && let Some(menu) = objet.downcast_ref::<NSMenu>()
                {
                    let adresse = menu as *const NSMenu as usize;
                    let mut menus = self.ivars().menus.borrow_mut();
                    if !menus
                        .iter()
                        .any(|m| &**m as *const NSMenu as usize == adresse)
                    {
                        menus.push(menu.retain());
                    }
                }
            } else if &*nom == unsafe { objc2_app_kit::NSMenuDidBeginTrackingNotification } {
                self.ivars().debuts.set(self.ivars().debuts.get() + 1);
            } else if &*nom == unsafe { objc2_app_kit::NSMenuDidEndTrackingNotification } {
                self.ivars().fins.set(self.ivars().fins.get() + 1);
            }
        }

        /// Ferme un menu en cours de suivi sans clic ni touche physique.
        #[unsafe(method(annulerMenus:))]
        fn annuler_menus(&self, _minuterie: &NSTimer) {
            for menu in self.ivars().menus.borrow().iter() {
                menu.cancelTrackingWithoutAnimation();
            }
        }
    }
);

impl Observateur {
    fn nouveau(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(ObservateurIvars {
            menus: RefCell::new(Vec::new()),
            debuts: Cell::new(0),
            fins: Cell::new(0),
        });
        unsafe { msg_send![super(this), init] }
    }

    fn debuts(&self) -> usize {
        self.ivars().debuts.get()
    }

    fn fins(&self) -> usize {
        self.ivars().fins.get()
    }

    fn menu_deroule(&self) -> Retained<NSMenu> {
        self.ivars()
            .menus
            .borrow()
            .iter()
            .find(|menu| {
                menu.itemAtIndex(0)
                    .map(|item| item.title().to_string() == "Same image on all displays")
                    .unwrap_or(false)
            })
            .expect("menu « … » introuvable parmi les menus observes")
            .retain()
    }
}

fn titre(item: &NSMenuItem) -> String {
    item.title().to_string()
}

fn sous_menu(item: &NSMenuItem, attendu: &str) -> Retained<NSMenu> {
    assert_eq!(titre(item), attendu, "sous-menu attendu");
    item.submenu()
        .unwrap_or_else(|| panic!("le sous-menu « {attendu} » est absent"))
}

fn element(menu: &NSMenu, index: isize) -> Retained<NSMenuItem> {
    menu.itemAtIndex(index)
        .unwrap_or_else(|| panic!("element {index} absent de {}", menu.title()))
}

fn commande(menu: &NSMenu, action: objc2::runtime::Sel) -> Retained<NSMenuItem> {
    let commandes: Vec<_> = menu.itemArray().iter()
        .filter(|item| item.action() == Some(action)).collect();
    assert_eq!(commandes.len(), 1, "commande {action:?} absente ou dupliquee");
    commandes.into_iter().next().unwrap()
}

fn represente(item: &NSMenuItem) -> Option<PathBuf> {
    item.representedObject()
        .and_then(|objet| objet.downcast_ref::<NSString>().map(|s| PathBuf::from(s.to_string())))
}

fn cible_ptr(cible: &AnyObject) -> usize {
    cible as *const AnyObject as usize
}

/// Vérifie qu’une entrée active est bien branchée sur la cible réelle du
/// produit et sur le sélecteur attendu (jamais exécuté ici).
fn verifier_action(item: &NSMenuItem, attendu: objc2::runtime::Sel, cible: usize) {
    assert_eq!(
        item.action(),
        Some(attendu),
        "selecteur inattendu pour « {} »",
        titre(item)
    );
    assert_eq!(
        item.target().map(|t| &*t as *const AnyObject as usize),
        Some(cible),
        "cible inattendue pour « {} »",
        titre(item)
    );
}

fn preparer_images(dossier: &Path, largeur: u32, hauteur: u32, teinte: u8) {
    assert!(
        !dossier.exists(),
        "fixtures deja presentes : la recette exige un dossier neuf ({})",
        dossier.display()
    );
    std::fs::create_dir_all(dossier).unwrap();
    for n in 0..3u8 {
        let img = image::RgbImage::from_fn(largeur, hauteur, |x, y| {
            image::Rgb([(x % 255) as u8, (y % 255) as u8, teinte + n * 80])
        });
        img.save(dossier.join(format!("image-{n}.png"))).unwrap();
    }
}

/// Ouvre le menu « … » par le contrôle réel et le referme par un minuteur
/// AppKit (NSMenu.cancelTrackingWithoutAnimation), sans touche ni clic.
fn ouvrir_menu(
    cible: &AnyObject,
    bouton: &NSButton,
    observateur: &Observateur,
) -> Retained<NSMenu> {
    let fins_avant = observateur.fins();
    let minuterie = unsafe {
        NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
            0.4,
            observateur,
            objc2::sel!(annulerMenus:),
            None,
            false,
        )
    };
    unsafe {
        objc2_foundation::NSRunLoop::mainRunLoop().addTimer_forMode(
            &minuterie,
            objc2_foundation::NSRunLoopCommonModes,
        );
    }
    let _: () = unsafe { msg_send![cible, menuOuvrir: bouton] };
    minuterie.invalidate();
    assert!(
        observateur.debuts() > 0,
        "le menu n’a jamais commence son suivi (NSMenuDidBeginTracking)"
    );
    assert!(
        observateur.fins() > fins_avant,
        "le menu n’a pas fini son suivi (NSMenuDidEndTracking)"
    );
    observateur.menu_deroule()
}

fn executer(menu: &NSMenu, index: isize) {
    menu.performActionForItemAtIndex(index);
}

// ---------------------------------------------------------------------------
// Mode --panneau : reglages natifs, controles AppKit reels.
// Aucune action appliquer: ni suivanteDiaporama:, aucun fond modifie.
// ---------------------------------------------------------------------------

fn boutons_action(vue: &NSView, action: objc2::runtime::Sel) -> Vec<Retained<NSButton>> {
    let mut resultat = Vec::new();
    if let Some(bouton) = vue.downcast_ref::<NSButton>() {
        if unsafe { bouton.action() } == Some(action) {
            resultat.push(bouton.retain());
        }
    }
    for enfant in vue.subviews() {
        resultat.extend(boutons_action(&enfant, action));
    }
    resultat
}

fn controle_action<T: objc2::Message + objc2::DowncastTarget>(
    vue: &NSView,
    action: objc2::runtime::Sel,
) -> Option<Retained<T>> {
    if let Some(controle) = vue.downcast_ref::<T>() {
        // Toutes les classes visees ici heritent de NSControl.
        let controle_control: &objc2_app_kit::NSControl =
            unsafe { &*(controle as *const T as *const objc2_app_kit::NSControl) };
        if unsafe { controle_control.action() } == Some(action) {
            return Some(controle.retain());
        }
    }
    for enfant in vue.subviews() {
        if let Some(controle) = controle_action::<T>(&enfant, action) {
            return Some(controle);
        }
    }
    None
}

fn instantane(cible: &AnyObject) -> serde_json::Value {
    let pointeur: *mut NSString = unsafe {
        msg_send![cible, instantaneReglages: std::ptr::null::<NSObject>()]
    };
    let texte = unsafe { Retained::from_raw(pointeur) }
        .expect("instantane du panneau")
        .to_string();
    serde_json::from_str(&texte).expect("instantane JSON")
}

fn texte_json(valeur: &serde_json::Value, cle: &str) -> String {
    valeur[cle].as_str().unwrap_or_default().to_string()
}

fn fenetres_reglages(app: &NSApplication) -> Vec<Retained<objc2_app_kit::NSWindow>> {
    app.windows()
        .iter()
        .filter(|w| w.title().to_string() == "Settings")
        .map(|w| w.retain())
        .collect()
}

fn activer(
    app: &NSApplication,
    bouton: &NSButton,
) {
    let action = unsafe { bouton.action() }.expect("action du controle");
    let cible = bouton.target();
    assert!(
        unsafe {
            app.sendAction_to_from(action, cible.as_deref(), Some(bouton))
        },
        "l'action du controle n'a pas ete acceptee par AppKit"
    );
}

#[allow(clippy::too_many_lines)]
fn mode_panneau(app: &NSApplication, mtm: MainThreadMarker, racine: &Path) {
    let a = racine.join("panneau-Paysages");
    let b = racine.join("panneau-Portraits");
    preparer_images(&a, 360, 220, 60);
    preparer_images(&b, 220, 360, 30);
    let images_a: Vec<String> = (0..3)
        .map(|n| a.join(format!("image-{n}.png")).to_string_lossy().into_owned())
        .collect();
    let reglages = preferences::Reglages {
        dossier: a.to_string_lossy().into_owned(),
        tri: "nom".into(),
        diaporama_intervalle: 1234,
        dossiers_favoris: vec![a.to_string_lossy().into_owned()],
        largeur_fenetre: 1024.0,
        hauteur_fenetre: 700.0,
        favoris: images_a.clone(),
        ..Default::default()
    };
    preferences::sauvegarder(&reglages).unwrap();

    let (fenetre, etat) = mural::app::construire(mtm, a.clone(), true);
    let contenu = fenetre.contentView().unwrap();
    let bouton_menu = bouton_action(&contenu, objc2::sel!(menuOuvrir:)).unwrap();
    let cible = bouton_menu.target().expect("cible du menu");
    let cible_ptr = cible_ptr(&cible);
    let segments = trouver::<NSSegmentedControl>(&contenu).unwrap();

    // Le menu ⋯ garde ses indices et ajoute Settings… en dernier.
    let observateur = Observateur::nouveau(mtm);
    let centre = objc2_foundation::NSNotificationCenter::defaultCenter();
    for nom in [
        unsafe { objc2_app_kit::NSMenuDidAddItemNotification },
        unsafe { objc2_app_kit::NSMenuDidBeginTrackingNotification },
        unsafe { objc2_app_kit::NSMenuDidEndTrackingNotification },
    ] {
        unsafe {
            centre.addObserver_selector_name_object(
                &observateur,
                objc2::sel!(menuModifie:),
                Some(nom),
                None,
            );
        }
    }
    let menu = ouvrir_menu(&cible, &bouton_menu, &observateur);
    assert_eq!(menu.numberOfItems(), 13);
    let settings_item = commande(&menu, objc2::sel!(ouvrirReglages:));
    assert_eq!(titre(&settings_item), "Settings…");
    verifier_action(&settings_item, objc2::sel!(ouvrirReglages:), cible_ptr);

    // Menu application : Settings… avec ⌘,.
    let principal = app.mainMenu().expect("menu principal");
    let menu_mural = sous_menu(&element(&principal, 0), "Mural");
    let version = menu_mural.itemArray().iter()
        .find(|i| titre(i) == format!("Version {}", env!("CARGO_PKG_VERSION")))
        .expect("version absente du menu Mural");
    assert!(!version.isEnabled(), "la version est informative");
    // Les raccourcis texte doivent rester dans la chaine des repondeurs :
    // une cible Mural explicite empecherait le champ de recherche de coller.
    let edition = principal.itemArray().iter()
        .find(|i| titre(i) == "Edit")
        .expect("menu Edit absent : les raccourcis texte ne sont pas routes");
    let edition = sous_menu(&edition, "Edit");
    for (nom, action, cle) in [
        ("Cut", objc2::sel!(cut:), "x"),
        ("Copy", objc2::sel!(copy:), "c"),
        ("Paste", objc2::sel!(paste:), "v"),
        ("Select All", objc2::sel!(selectAll:), "a"),
    ] {
        let item = edition.itemArray().iter().find(|i| titre(i) == nom)
            .unwrap_or_else(|| panic!("commande {nom} absente"));
        assert_eq!(item.action(), Some(action));
        assert_eq!(item.keyEquivalent().to_string(), cle);
        assert!(item.target().is_none(), "{nom} doit viser le premier repondeur");
    }
    let fichier = sous_menu(&element(&principal, 1), "File");
    fn contient_texte(vue: &NSView, attendu: &str) -> bool {
        if let Some(texte) = vue.downcast_ref::<objc2_app_kit::NSTextField>() {
            if texte.stringValue().to_string() == attendu { return true; }
        }
        vue.subviews().iter().any(|v| contient_texte(&v, attendu))
    }
    let cellules_avant = unsafe { &*etat }.cellules.clone();
    let limite = Instant::now() + Duration::from_secs(3);
    while unsafe { &*etat }.vignettes.iter().any(Option::is_none) && Instant::now() < limite {
        let _: () = unsafe { msg_send![&*cible, vignettesPretes: std::ptr::null::<NSObject>()] };
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(contient_texte(&contenu, "Selected: image-0.png · 3 slideshow image(s)"), "statut final non retabli");
    assert_eq!(unsafe { &*etat }.cellules, cellules_avant, "les notifications ont reconstruit la grille");
    println!("OK : statut final retabli par le worker, boutons de grille conserves");
    assert!(contient_texte(&contenu, "360 x 220 px · 2.3 KiB"), "taille absente de la barre d’etat");
    let bouton_premier = unsafe { &*etat }.cellules.iter().find(|(_, i)| **i == 0)
        .map(|(adresse, _)| unsafe { &*(*adresse as *const NSButton) }).unwrap();
    let original = unsafe { { let e = &mut *etat; std::mem::replace(&mut e.images[0].chemin, racine.join("fichier-absent.png")) } };
    let _: () = unsafe { msg_send![&*cible, celluleCliquee: bouton_premier] };
    assert!(contient_texte(&contenu, "Size unavailable"), "fichier indisponible sans message de taille");
    unsafe { let e = &mut *etat; e.images[0].chemin = original; }
    let _: () = unsafe { msg_send![&*cible, celluleCliquee: bouton_premier] };
    let journal = fichier.itemArray().iter().find(|i| titre(i) == "Show log in Finder")
        .expect("commande de journal absente");
    verifier_action(&journal, objc2::sel!(revelerJournal:), cible_ptr);
    // Inspection seulement : la recette ne lance pas Finder.
    let recherche_menu = fichier.itemArray().iter()
        .find(|i| i.keyEquivalent().to_string() == "f")
        .expect("raccourci recherche ⌘F absent");
    verifier_action(&recherche_menu, objc2::sel!(ciblerRecherche:), cible_ptr);
    let champ = trouver::<NSSearchField>(&contenu).unwrap();
    assert!(unsafe { app.sendAction_to_from(objc2::sel!(ciblerRecherche:), Some(&*cible), None) });
    assert!(champ.currentEditor().is_some(), "⌘F ne donne pas le focus au champ");

    // Etat simule en memoire : aucun fond reel applique par cette recette.
    let (fonds_avant, reglages_avant) = unsafe {
        let e = &mut *etat;
        let avant = (e.fonds_actifs.clone(), e.reglages.clone());
        e.fonds_actifs = vec![images_a[0].clone()];
        e.reglages.favoris = vec![images_a[1].clone()];
        e.reglages.selection_independante = false;
        avant
    };
    let _: () = unsafe { msg_send![&*cible, redessinerGrille: std::ptr::null::<NSObject>()] };
    for (&adresse, &index) in &unsafe { &*etat }.cellules {
        let bouton = unsafe { &*(adresse as *const NSButton) };
        for (tag, visible) in [(8101, index == 0), (8102, index == 1)] {
            let badge = bouton.subviews().iter().find(|v| v.tag() == tag)
                .expect("badge de vignette absent");
            assert_eq!(!badge.isHidden(), visible, "badge {tag}, image {index}");
            assert!(badge.hitTest(badge.frame().origin).is_none(), "un badge ne doit pas intercepter les clics");
        }
    }
    let courant_avant = unsafe { (*etat).ecran_courant };
    for courant in 0..unsafe { &*etat }.ecrans.len() {
        unsafe {
            let e = &mut *etat;
            e.reglages.selection_independante = true;
            e.reglages.favoris_par_ecran.clear();
            e.reglages.favoris_par_ecran.insert(e.ecrans[0].cle.clone(), vec![images_a[2].clone()]);
            // Une intention sauvegardee ne suffit pas pour le badge actif.
            e.reglages.image_par_ecran.insert(e.ecrans[0].cle.clone(), images_a[2].clone());
            e.ecran_courant = courant;
        }
        let _: () = unsafe { msg_send![&*cible, redessinerGrille: std::ptr::null::<NSObject>()] };
        for (&adresse, &index) in &unsafe { &*etat }.cellules {
            let bouton = unsafe { &*(adresse as *const NSButton) };
            let vues = bouton.subviews();
            let actif = vues.iter().find(|v| v.tag() == 8101).unwrap();
            let inclus = vues.iter().find(|v| v.tag() == 8102).unwrap();
            assert_eq!(!actif.isHidden(), index == 0);
            assert_eq!(!inclus.isHidden(), courant == 0 && index == 2);
        }
    }
    unsafe { (*etat).ecran_courant = courant_avant; }
    unsafe { (*etat).fonds_actifs = fonds_avant; (*etat).reglages = reglages_avant; }
    let _: () = unsafe { msg_send![&*cible, redessinerGrille: std::ptr::null::<NSObject>()] };
    println!("OK : focus recherche et badges actif/diaporama distincts sans appliquer de fond");
    let app_item = element(&principal, 0);
    let app_menu = sous_menu(&app_item, "Mural");
    let reglages_menu = app_menu
        .itemArray()
        .iter()
        .find(|i| titre(i) == "Settings…")
        .expect("Settings… absent du menu Mural");
    verifier_action(&reglages_menu, objc2::sel!(ouvrirReglages:), cible_ptr);
    assert_eq!(
        reglages_menu.keyEquivalent().to_string(),
        ",",
        "raccourci ⌘, absent"
    );

    // Ouverture par la vraie action de menu.
    let avant = std::fs::read(preferences::chemin_fichier().unwrap()).unwrap();
    let _: () = unsafe { msg_send![&*cible, ouvrirReglages: std::ptr::null::<NSObject>()] };
    let fenetres = fenetres_reglages(app);
    assert_eq!(fenetres.len(), 1, "fenetre de reglages unique");
    let panneau = fenetres[0].retain();
    assert!(panneau.isVisible());
    let vue = panneau.contentView().unwrap();
    let onglets = trouver::<NSTabView>(&vue).expect("onglets du panneau");

    let s = instantane(&cible);
    assert_eq!(
        s["onglets"],
        serde_json::json!(["Display", "Slideshow", "Folders", "AVIF"]),
        "onglets du panneau"
    );
    assert_eq!(s["onglet"], 0);
    assert_eq!(s["meme_image"], 0);
    assert_eq!(s["tri_nom"], 1);
    assert_eq!(s["tri_date"], 0);
    // Intervalle hors presets : conserve tel quel, avec une entree dediee.
    assert_eq!(s["intervalle"], 1234, "intervalle existant ecrase a l'ouverture");
    assert!(
        texte_json(&s, "intervalle_titre").contains("1234 s"),
        "valeur hors presets non signalee : {}",
        s["intervalle_titre"]
    );
    assert!(
        texte_json(&s, "compteur").contains("3 image(s)"),
        "compteur initial : {}",
        s["compteur"]
    );
    assert!(
        texte_json(&s, "compteur").contains("shared list")
    );
    assert!(
        texte_json(&s, "compteur").contains("paused")
    );
    assert!(s["demarrer"]["actif"].as_bool().unwrap(), "lecture indisponible avec 3 images");
    assert!(s["suivante_active"].as_bool().unwrap());
    assert!(
        texte_json(&s, "dossier_courant") == a.to_string_lossy(),
        "dossier courant affiche"
    );
    assert_eq!(
        texte_json(&s, "dossier_infobulle"),
        a.to_string_lossy(),
        "infobulle du dossier complet"
    );
    assert_eq!(
        s["participants"].as_array().unwrap().len(),
        unsafe { &*etat }.ecrans.len()
    );
    assert!(
        s["participants"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["etat"] == 1),
        "tous les ecrans participent par defaut"
    );
    assert_eq!(s["ecrans"].as_array().unwrap().len(), unsafe { &*etat }.ecrans.len());
    assert_eq!(
        texte_json(&s, "cle_affichee"),
        format!("{} × {} points", unsafe { &*etat }.ecrans[0].largeur, unsafe { &*etat }.ecrans[0].hauteur)
    );
    assert_eq!(
        s["associations"].as_array().unwrap().len(),
        unsafe { &*etat }.ecrans.len()
    );
    assert!(texte_json(&s, "avif_capacites").contains("AVIF reading:"));
    assert!(texte_json(&s, "avif_capacites").contains("AVIF encoding:"));
    let avif_autorise = mural::formats::avif_encodable() && mural::avif::dossier_cache().is_some();
    assert_eq!(
        s["avif_bouton_actif"].as_bool().unwrap(),
        avif_autorise,
        "le bouton AVIF doit suivre la capacite reelle et la disponibilite du cache"
    );
    assert_eq!(
        texte_json(&s, "favori_titre"),
        "Remove from Favorites",
        "un dossier deja favori doit proposer le retrait"
    );
    assert_eq!(
        s["favoris"].as_array().unwrap().len(),
        1,
        "liste des favoris reelle attendue"
    );
    println!("OK : panneau ouvert, quatre onglets, display/compteur/intervalle hors presets/favoris/AVIF reflechis");

    // Onglets selectionnables, sans ecriture de reglages ni action.
    for (index, libelle) in ["Display", "Slideshow", "Folders", "AVIF"].iter().enumerate() {
        onglets.selectTabViewItemAtIndex(index as isize);
        let s = instantane(&cible);
        assert_eq!(s["onglet"], index as i64);
        assert_eq!(
            onglets.tabViewItemAtIndex(index as isize).label().to_string(),
            *libelle
        );
    }
    onglets.selectTabViewItemAtIndex(0);
    assert_eq!(
        std::fs::read(preferences::chemin_fichier().unwrap()).unwrap(),
        avant,
        "ouvrir/rafraichir le panneau ne doit rien sauvegarder"
    );

    // Modification via les controles : prefs, panneau et menu restent coherents.
    let meme = bouton_action(&vue, objc2::sel!(memeImagePartout:)).unwrap();
    activer(app, &meme);
    let p = preferences::charger();
    assert!(p.meme_image_partout, "case non persistee");
    assert_eq!(instantane(&cible)["meme_image"], 1);
    let menu_meme = ouvrir_menu(&cible, &bouton_menu, &observateur);
    assert_eq!(element(&menu_meme, 0).state(), 1, "menu non synchronise");
    executer(&menu_meme, 3); // tri par date depuis le menu
    let s = instantane(&cible);
    assert_eq!(s["tri_date"], 1, "panneau non synchronise au tri du menu");
    assert_eq!(s["tri_nom"], 0);
    assert_eq!(preferences::charger().tri, "date");

    onglets.selectTabViewItemAtIndex(1);
    let aleatoire = bouton_action(&vue, objc2::sel!(aleatoireDiaporama:)).unwrap();
    activer(app, &aleatoire);
    assert!(preferences::charger().diaporama_aleatoire);
    assert_eq!(instantane(&cible)["aleatoire"], 1);
    let menu_aleatoire = ouvrir_menu(&cible, &bouton_menu, &observateur);
    let diapo_menu = element(&menu_aleatoire, 5);
    let diapo_sous = sous_menu(&diapo_menu, &titre(&diapo_menu));
    executer(&diapo_sous, 2); // desactive l'ordre aleatoire depuis le menu
    assert!(!preferences::charger().diaporama_aleatoire);
    assert_eq!(
        instantane(&cible)["aleatoire"],
        0,
        "panneau non synchronise a l'ordre aleatoire du menu"
    );

    let filtre = bouton_action(&vue, objc2::sel!(filtrerDiaporama:)).unwrap();
    activer(app, &filtre);
    assert!(unsafe { &*etat }.filtre_diaporama);
    assert_eq!(instantane(&cible)["filtre"], 1);
    activer(app, &filtre);
    assert!(!unsafe { &*etat }.filtre_diaporama);
    println!("OK : controles -> reglages/menus et menus -> panneau, sans divergence");

    // Selection independante : compteur de la bonne liste.
    let independante = bouton_action(&vue, objc2::sel!(selectionIndependante:)).unwrap();
    activer(app, &independante);
    assert!(preferences::charger().selection_independante);
    let s = instantane(&cible);
    assert!(
        texte_json(&s, "compteur").contains("independent lists"),
        "mode independant non affiche : {}",
        s["compteur"]
    );
    assert!(
        texte_json(&s, "compteur").contains("0 image(s)"),
        "liste dediee vide attendue : {}",
        s["compteur"]
    );
    assert!(!s["demarrer"]["actif"].as_bool().unwrap());
    assert!(!s["suivante_active"].as_bool().unwrap());
    let ajouter = bouton_action(&vue, objc2::sel!(ajouterDossierDiaporama:)).unwrap();
    activer(app, &ajouter);
    let s = instantane(&cible);
    assert!(texte_json(&s, "compteur").contains("3 image(s)"));
    assert!(s["demarrer"]["actif"].as_bool().unwrap());
    assert!(s["suivante_active"].as_bool().unwrap());
    assert_eq!(
        preferences::charger()
            .favoris_par_ecran
            .get(unsafe { &*etat }.cle_ecran_courant().unwrap())
            .map(Vec::len),
        Some(3)
    );
    // Retrait puis remise du dossier via les actions existantes.
    let retirer_selection =
        bouton_action(&vue, objc2::sel!(retirerDossierDiaporama:)).unwrap();
    activer(app, &retirer_selection);
    assert!(
        texte_json(&instantane(&cible), "compteur").contains("0 image(s)"),
        "retrait du dossier non reflechi"
    );
    activer(app, &ajouter);
    assert!(texte_json(&instantane(&cible), "compteur").contains("3 image(s)"));

    // Ecran 2 : le panneau change d'ecran, la barre principale suit, le
    // compteur affiche la liste de CET ecran (vide).
    let mut second_ecran = false;
    if unsafe { &*etat }.ecrans.len() >= 2 {
        second_ecran = true;
        let cle1 = unsafe { &*etat }.ecrans[0].cle.clone();
        let cle2 = unsafe { &*etat }.ecrans[1].cle.clone();
        onglets.selectTabViewItemAtIndex(0);
        let popup_ecrans = controle_action::<NSPopUpButton>(&vue, objc2::sel!(ecranPanneau:)).unwrap();
        popup_ecrans.selectItemAtIndex(1);
        activer(app, &popup_ecrans);
        assert_eq!(unsafe { &*etat }.ecran_courant, 1);
        assert_eq!(segments.selectedSegment(), 1, "barre principale non synchronisee");
        assert_eq!(preferences::charger().ecran_selectionne.as_deref(), Some(cle2.as_str()));
        let s = instantane(&cible);
        assert_eq!(s["ecran_selectionne"], 1);
        assert_eq!(texte_json(&s, "cle_affichee"), format!("{} × {} points", unsafe { &*etat }.ecrans[1].largeur, unsafe { &*etat }.ecrans[1].hauteur));
        assert!(
            texte_json(&s, "compteur").contains("0 image(s)"),
            "compteur global au lieu de la liste de l'ecran 2 : {}",
            s["compteur"]
        );
        // Le plan reste global : l'ecran 1 a des images, donc Start peut
        // rester disponible meme si la liste de l'ecran 2 est vide.
        activer(app, &ajouter);
        assert!(texte_json(&instantane(&cible), "compteur").contains("3 image(s)"));

        // Association de dossier : UUID de l'ecran 1, sans changer d'ecran.
        onglets.selectTabViewItemAtIndex(2);
        let dossier_avant = unsafe { &*etat }.dossier.clone();
        let courant_avant = unsafe { &*etat }.ecran_courant;
        let assoc1 = boutons_action(&vue, objc2::sel!(dossierEcranPanneau:))
            .into_iter()
            .find(|b| {
                b.toolTip()
                    .map(|t| t.to_string().contains("Display 1 —"))
                    .unwrap_or(false)
            })
            .expect("bouton d'association de l'ecran 1");
        activer(app, &assoc1);
        assert_eq!(unsafe { &*etat }.ecran_courant, courant_avant, "ecran courant modifie");
        assert_eq!(unsafe { &*etat }.dossier, dossier_avant, "dossier courant modifie");
        let dossier_attendu = unsafe { &*etat }.dossier.to_string_lossy().into_owned();
        assert_eq!(
            preferences::charger().dossier_par_ecran.get(&cle1),
            Some(&dossier_attendu),
            "association non enregistree pour le bon UUID"
        );
        let s = instantane(&cible);
        let ligne1 = s["associations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["cle"] == cle1)
            .expect("association ecran 1 absente");
        assert_eq!(
            texte_json(ligne1, "texte"),
            unsafe { &*etat }
                .dossier
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(
            texte_json(ligne1, "infobulle"),
            unsafe { &*etat }.dossier.to_string_lossy().to_string()
        );

        // L'ecran 1 charge bien le dossier associe quand on y revient.
        segments.setSelectedSegment(0);
        let _: () = unsafe { msg_send![&*cible, ecranChange: &*segments] };
        assert_eq!(unsafe { &*etat }.ecran_courant, 0);
        assert_eq!(unsafe { &*etat }.dossier, dossier_avant);
        println!("OK : ecran 2, compteur dedie, barre synchronisee et association par UUID sans effet de bord");
    } else {
        println!("NON VERIFIE : ecran 2 absent, compteur dedie et association non verifies");
    }

    // Favoris : ajout, ouverture puis retrait par les vrais controles.
    onglets.selectTabViewItemAtIndex(2);
    ouvrir(&cible, &b, mtm); // dossier temporaire non favori
    assert_eq!(unsafe { &*etat }.dossier, b);
    let dossier_favori = bouton_action(&vue, objc2::sel!(dossierFavori:)).unwrap();
    activer(app, &dossier_favori);
    let p = preferences::charger();
    assert_eq!(p.dossiers_favoris.len(), 2, "favori courant non ajoute");
    let s = instantane(&cible);
    assert_eq!(s["favoris"].as_array().unwrap().len(), 2);
    assert_eq!(texte_json(&s, "favori_titre"), "Remove from Favorites");
    let popup_favoris = controle_action::<NSPopUpButton>(&vue, objc2::sel!(favoriPanneau:)).unwrap();
    popup_favoris.selectItemAtIndex(0); // premier favori : dossier A
    activer(app, &popup_favoris);
    assert_eq!(unsafe { &*etat }.dossier, a, "le favori A n'a pas ete ouvert");
    assert_eq!(
        preferences::charger().dossier,
        a.to_string_lossy(),
        "ouverture de favori non persistee"
    );
    assert_eq!(
        texte_json(&instantane(&cible), "dossier_courant"),
        a.to_string_lossy()
    );
    let retirer = bouton_action(&vue, objc2::sel!(retirerFavoriPanneau:)).unwrap();
    activer(app, &retirer);
    assert_eq!(
        preferences::charger().dossiers_favoris,
        vec![b.to_string_lossy().into_owned()],
        "retrait du favori selectionne"
    );
    let s = instantane(&cible);
    assert_eq!(s["favoris"].as_array().unwrap().len(), 1);
    assert_eq!(texte_json(&s, "favori_titre"), "Add to Favorites");
    println!("OK : favoris ajoutes, ouverts et retires par les controles du panneau");

    // Fermeture/reouverture : meme fenetre, aucun doublon, onglet conserve.
    onglets.selectTabViewItemAtIndex(2);
    panneau.close();
    let _: () = unsafe { msg_send![&*cible, ouvrirReglages: std::ptr::null::<NSObject>()] };
    let apres = fenetres_reglages(app);
    assert_eq!(apres.len(), 1, "doublon de fenetre de reglages");
    assert_eq!(
        &*apres[0] as *const objc2_app_kit::NSWindow,
        &*panneau as *const objc2_app_kit::NSWindow,
        "une autre fenetre a ete creee"
    );
    assert!(apres[0].isVisible());
    let s = instantane(&cible);
    assert_eq!(s["onglet"], 2, "onglet selectionne perdu");
    assert_eq!(s["intervalle"], 1234, "intervalle hors presets ecrase");
    println!("OK : reouverture sans doublon, onglet conserve, intervalle personnalise intact");

    // Reenumeration : listes reconstruites sans doublon ; un ancien controle
    // (retire) est ignore et ne peut pas viser un autre ecran.
    let cle1 = unsafe { &*etat }.ecrans[0].cle.clone();
    let ancien = boutons_action(&vue, objc2::sel!(dossierEcranPanneau:))
        .into_iter()
        .find(|b| {
            b.toolTip()
                .map(|t| t.to_string().contains("Display 1 —"))
                .unwrap_or(false)
        })
        .expect("ancien bouton d'association");
    let association_avant = preferences::charger().dossier_par_ecran.get(&cle1).cloned();
    unsafe {
        centre.postNotificationName_object(
            objc2_app_kit::NSApplicationDidChangeScreenParametersNotification,
            None,
        );
    }
    let s = instantane(&cible);
    let ecrans = unsafe { &*etat }.ecrans.len();
    assert_eq!(s["participants"].as_array().unwrap().len(), ecrans);
    assert_eq!(s["associations"].as_array().unwrap().len(), ecrans);
    assert_eq!(s["ecrans"].as_array().unwrap().len(), ecrans);
    // Ancien bouton (detache) : son action doit etre ignoree.
    let dossier_avant = unsafe { &*etat }.dossier.clone();
    let _: () = unsafe {
        msg_send![&*cible, dossierEcranPanneau: &*ancien]
    };
    assert_eq!(
        preferences::charger().dossier_par_ecran.get(&cle1).cloned(),
        association_avant,
        "un ancien controle a modifie une association"
    );
    assert_eq!(unsafe { &*etat }.dossier, dossier_avant);
    println!("OK : reenumeration sans doublon, ancien controle ignore (UUID intact)");

    // AVIF : le bouton ouvre le vrai panneau existant, sans conversion.
    onglets.selectTabViewItemAtIndex(3);
    let avif = bouton_action(&vue, objc2::sel!(cacheAvif:)).unwrap();
    assert!(avif.isEnabled());
    activer(app, &avif);
    let avif_fenetre = app
        .windows()
        .iter()
        .find(|w| w.title().to_string() == "AVIF Cache")
        .expect("panneau AVIF absent")
        .retain();
    assert!(avif_fenetre.isVisible());
    avif_fenetre.close();
    println!("OK : bouton AVIF branche sur le panneau existant (aucune conversion)");

    // Fin : diaporama jamais demarre, valeurs personnalisees intactes.
    assert!(!unsafe { &*etat }.diaporama_actif, "le panneau a demarre le diaporama");
    let finales = preferences::charger();
    assert_eq!(finales.diaporama_intervalle, 1234);
    assert!(finales.selection_independante);
    assert!(finales.meme_image_partout);
    assert_eq!(finales.tri, "date");
    assert!(!finales.diaporama_aleatoire);
    assert_eq!(finales.favoris.len(), 3, "liste globale modifiee par erreur");
    if !second_ecran {
        println!("NON VERIFIE : second ecran absent");
    }
    unsafe { centre.removeObserver(&observateur) };
    panneau.close();
    let _: () = unsafe { msg_send![&*cible, apercuPleinEcran: &*cible] };
    let plein = app.windows().iter().find(|w| w.title().to_string().contains("Esc to close"))
        .expect("apercu absent").retain();
    attendre(|| plein.styleMask().contains(NSWindowStyleMask::FullScreen));
    let espace = NSString::from_str(" ");
    let evenement = objc2_app_kit::NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
        objc2_app_kit::NSEventType::KeyDown, objc2_foundation::NSPoint::ZERO,
        objc2_app_kit::NSEventModifierFlags::empty(), 0.0, plein.windowNumber(), None,
        &espace, &espace, false, 49).expect("evenement espace");
    plein.sendEvent(&evenement);
    assert!(!plein.isVisible(), "Espace ne ferme pas l'apercu via le routage de fenetre");
    println!("OK : Espace ferme l'apercu via le routage de fenetre");
    let selection_avant = unsafe { &*etat }.selection;
    let cellules_avant = unsafe { &*etat }.cellules.clone();
    fenetre.close();
    assert!(!fenetre.isVisible(), "la fermeture laisse la fenetre visible");
    let _: () = unsafe { msg_send![&*cible, afficherFenetre: std::ptr::null::<NSObject>()] };
    assert!(fenetre.isVisible(), "Show window ne rouvre pas la fenetre");
    assert_eq!(unsafe { &*etat }.selection, selection_avant);
    assert_eq!(unsafe { &*etat }.cellules, cellules_avant);
    fenetre.close();
    let rouvert: bool = unsafe { msg_send![&*cible, applicationShouldHandleReopen: app, hasVisibleWindows: false] };
    assert!(rouvert && fenetre.isVisible(), "le delegate Dock ne rouvre pas la fenetre");
    assert_eq!(unsafe { &*etat }.selection, selection_avant);
    assert!(!unsafe { &*etat }.diaporama_actif, "la reouverture demarre le diaporama");
    fenetre.close();
    println!("OK : fermeture reelle et reouverture Show window/Dock, selection et cellules conservees, diaporama arrete");
    println!("OK : mode --panneau autonome, aucun fond applique, diaporama jamais declenche");
}

fn etat_menu_diaporama(
    item: &NSMenuItem,
) -> (bool, bool, bool, Vec<(isize, bool)>, Vec<(String, bool)>) {
    let menu = sous_menu(item, &titre(item));
    let demarrer = element(&menu, 0);
    let suivante = element(&menu, 1);
    let aleatoire = element(&menu, 2);
    let intervalle = element(&menu, 3);
    let intervalle_menu = sous_menu(&intervalle, "Interval");
    let intervalles: Vec<(isize, bool)> = intervalle_menu
        .itemArray()
        .iter()
        .map(|i| (i.tag(), i.state() == 1))
        .collect();
    let ecrans_item = element(&menu, 4);
    let ecrans_menu = sous_menu(&ecrans_item, "Participating displays");
    let ecrans: Vec<(String, bool)> = ecrans_menu
        .itemArray()
        .iter()
        .map(|i| {
            let cle = i
                .representedObject()
                .and_then(|o| o.downcast_ref::<NSString>().map(|s| s.to_string()))
                .expect("cle d’ecran representee");
            (cle, i.state() == 1)
        })
        .collect();
    (
        demarrer.isEnabled(),
        suivante.isEnabled(),
        aleatoire.state() == 1,
        intervalles,
        ecrans,
    )
}

#[allow(clippy::too_many_lines)]
fn mode_menus(app: &NSApplication, mtm: MainThreadMarker, racine: &Path) {
    // Fixtures propres au mode menus, dans le dossier temporaire fourni.
    let a = racine.join("menus-Paysages");
    let b = racine.join("menus-Portraits");
    preparer_images(&a, 360, 220, 50);
    preparer_images(&b, 220, 360, 40);
    let reglages = preferences::Reglages {
        tri: "nom".into(),
        diaporama_intervalle: 300,
        largeur_fenetre: 1024.0,
        hauteur_fenetre: 700.0,
        ..Default::default()
    };
    preferences::sauvegarder(&reglages).unwrap();

    let (fenetre, etat) = mural::app::construire(mtm, a.clone(), true);
    let contenu = fenetre.contentView().unwrap();
    let bouton = bouton_action(&contenu, objc2::sel!(menuOuvrir:)).unwrap();
    let cible = bouton.target().expect("cible du bouton menu");
    let cible_ptr = cible_ptr(&cible);
    let bouton_favori = bouton_action(&contenu, objc2::sel!(dossierFavori:)).unwrap();
    let bouton_diaporama = bouton_action(&contenu, objc2::sel!(basculerDiaporama:)).unwrap();
    let bouton_suivante = bouton_action(&contenu, objc2::sel!(suivanteDiaporama:)).unwrap();

    let observateur = Observateur::nouveau(mtm);
    let centre = objc2_foundation::NSNotificationCenter::defaultCenter();
    for nom in [
        unsafe { objc2_app_kit::NSMenuDidAddItemNotification },
        unsafe { objc2_app_kit::NSMenuDidBeginTrackingNotification },
        unsafe { objc2_app_kit::NSMenuDidEndTrackingNotification },
    ] {
        unsafe {
            centre.addObserver_selector_name_object(
                &observateur,
                objc2::sel!(menuModifie:),
                Some(nom),
                None,
            );
        }
    }

    // 1. Ouverture initiale : structure, états vides, aucune cible.
    let m1 = ouvrir_menu(&cible, &bouton, &observateur);
    assert_eq!(m1.numberOfItems(), 13, "menu « … » incomplet");
    assert_eq!(titre(&commande(&m1, objc2::sel!(cacheAvif:))), "AVIF cache…");
    verifier_action(&commande(&m1, objc2::sel!(cacheAvif:)), objc2::sel!(cacheAvif:), cible_ptr);
    assert_eq!(
        titre(&commande(&m1, objc2::sel!(ouvrirReglages:))),
        "Settings…",
        "commande Settings… absente"
    );
    verifier_action(&commande(&m1, objc2::sel!(ouvrirReglages:)), objc2::sel!(ouvrirReglages:), cible_ptr);
    let zero = element(&m1, 0);
    assert_eq!(titre(&zero), "Same image on all displays");
    verifier_action(&zero, objc2::sel!(memeImagePartout:), cible_ptr);
    assert_eq!(zero.state(), 0, "image commune cochee par defaut");
    let independante = element(&m1, 1);
    assert_eq!(titre(&independante), "Independent selection per display");
    verifier_action(
        &independante,
        objc2::sel!(selectionIndependante:),
        cible_ptr,
    );
    assert_eq!(
        independante.state(),
        0,
        "selection independante cochee par defaut"
    );
    let nom = element(&m1, 2);
    verifier_action(&nom, objc2::sel!(trierNom:), cible_ptr);
    assert_eq!(nom.state(), 1, "tri par nom non coche");
    let date = element(&m1, 3);
    verifier_action(&date, objc2::sel!(trierDate:), cible_ptr);
    assert_eq!(date.state(), 0, "tri par date coche a tort");
    assert!(element(&m1, 4).isSeparatorItem());
    let diapo = element(&m1, 5);
    assert_eq!(titre(&diapo), "Slideshow — 0 image(s)");
    let (demarrer, suivante, aleatoire, intervalles, ecrans) = etat_menu_diaporama(&diapo);
    assert!(!demarrer, "demarrage disponible sans image");
    assert!(!suivante, "image suivante disponible sans image");
    assert!(!bouton_diaporama.isEnabled(), "bouton lecture actif sans image");
    assert!(!bouton_suivante.isEnabled(), "bouton suivant actif sans image");
    assert!(!aleatoire, "ordre aleatoire coche par defaut");
    assert_eq!(
        intervalles,
        vec![(60, false), (300, true), (900, false), (1800, false), (3600, false)],
        "etats d’intervalle incoherents"
    );
    assert_eq!(ecrans.len(), unsafe { &*etat }.ecrans.len());
    assert!(ecrans.iter().all(|(_, coche)| *coche), "tous les ecrans doivent participer par defaut");
    let menu_diapo = sous_menu(&diapo, "Slideshow — 0 image(s)");
    verifier_action(&element(&menu_diapo, 0), objc2::sel!(basculerDiaporama:), cible_ptr);
    verifier_action(&element(&menu_diapo, 1), objc2::sel!(suivanteDiaporama:), cible_ptr);
    verifier_action(&element(&menu_diapo, 2), objc2::sel!(aleatoireDiaporama:), cible_ptr);
    let intervalle_item = element(&menu_diapo, 3);
    let sous_intervalles = sous_menu(&intervalle_item, "Interval");
    for item in sous_intervalles.itemArray().iter() {
        verifier_action(&item, objc2::sel!(intervalleDiaporama:), cible_ptr);
    }
    let ecrans_item = element(&menu_diapo, 4);
    let sous_ecrans = sous_menu(&ecrans_item, "Participating displays");
    for item in sous_ecrans.itemArray().iter() {
        verifier_action(&item, objc2::sel!(ecranDiaporama:), cible_ptr);
        assert!(represente(&item).is_some(), "cle d’ecran non representee");
    }
    for (index, attendu) in [
        (6, objc2::sel!(ajouterDossierDiaporama:)),
        (7, objc2::sel!(retirerDossierDiaporama:)),
        (8, objc2::sel!(filtrerDiaporama:)),
    ] {
        verifier_action(&element(&menu_diapo, index), attendu, cible_ptr);
    }
    let favoris_item = element(&m1, 6);
    let favoris = sous_menu(&favoris_item, "Favorite folders");
    assert_eq!(favoris.numberOfItems(), 1, "favoris vides attendus");
    assert!(!element(&favoris, 0).isEnabled(), "favori vide doit etre inactif");
    assert!(element(&favoris, 0).action().is_none());
    for (index, attendu) in [
        (7, objc2::sel!(apercuPleinEcran:)),
        (8, objc2::sel!(revelerImage:)),
        (9, objc2::sel!(copierChemin:)),
        (10, objc2::sel!(actualiser:)),
    ] {
        verifier_action(&element(&m1, index), attendu, cible_ptr);
    }
    println!(
        "OK : menu ouvert/ferme par AppKit (suivi debut/fin), structure et sous-menus diaporama, intervalle, participants et favoris vides"
    );

    // 2. Menu contextuel d’image : les entrées qui touchent le bureau sont
    //    inspectées, jamais exécutées.
    let (adresse, index) = unsafe { &*etat }
        .cellules
        .iter()
        .find(|(_, i)| **i == 0)
        .map(|(p, i)| (*p, *i))
        .unwrap();
    let vignette = unsafe { &*(adresse as *const NSButton) };
    let menu_image = vignette.menu().expect("menu contextuel de vignette");
    assert_eq!(menu_image.numberOfItems(), 5);
    verifier_action(&element(&menu_image, 0), objc2::sel!(appliquer:), cible_ptr);
    verifier_action(&element(&menu_image, 1), objc2::sel!(apercuPleinEcran:), cible_ptr);
    for item in menu_image.itemArray().iter() {
        assert_eq!(
            represente(&item),
            Some(unsafe { &*etat }.images[index].chemin.clone()),
            "menu image non lie a sa propre image"
        );
    }
    assert_eq!(
        represente(&element(&menu_image, 0)),
        Some(unsafe { &*etat }.images[index].chemin.clone())
    );
    println!("OK : menu d’image inspecte (appliquer: et suivante jamais invoques)");

    // 3. Actions réelles sur les NSMenuItem, sans effet bureau.
    executer(&m1, 0); // même image partout
    executer(&m1, 3); // tri par date
    let diapo1 = element(&m1, 5);
    let menu_diapo1 = sous_menu(&diapo1, &titre(&diapo1));
    executer(&menu_diapo1, 2); // ordre aléatoire
    let intervalle1 = element(&menu_diapo1, 3);
    let sous_intervalles1 = sous_menu(&intervalle1, "Interval");
    executer(&sous_intervalles1, 2); // 15 min = 900 s
    let ecrans1 = element(&menu_diapo1, 4);
    let sous_ecrans1 = sous_menu(&ecrans1, "Participating displays");
    assert!(sous_ecrans1.numberOfItems() > 1, "second ecran absent");
    executer(&sous_ecrans1, 1); // retire l’ecran 2
    executer(&menu_diapo1, 6); // ajoute le dossier à la sélection
    let p = preferences::charger();
    assert!(p.meme_image_partout);
    assert_eq!(p.tri, "date");
    assert!(p.diaporama_aleatoire);
    assert_eq!(p.diaporama_intervalle, 900);
    assert_eq!(p.favoris.len(), 3);
    assert_eq!(p.diaporama_ecrans.as_ref().map(Vec::len), Some(1));

    // 4. Nouvelle ouverture : UI ↔ préférences, cibles disponibles.
    let m2 = ouvrir_menu(&cible, &bouton, &observateur);
    assert_eq!(element(&m2, 0).state(), 1, "image commune non cochee");
    assert_eq!(element(&m2, 1).state(), 0);
    assert_eq!(element(&m2, 3).state(), 1, "tri date non coche");
    let diapo2 = element(&m2, 5);
    assert_eq!(titre(&diapo2), "Slideshow — 3 image(s)");
    let (demarrer2, suivante2, aleatoire2, intervalles2, ecrans2) = etat_menu_diaporama(&diapo2);
    assert!(demarrer2, "demarrage indisponible avec images et ecran");
    assert!(suivante2, "image suivante indisponible avec images et ecran");
    assert!(bouton_diaporama.isEnabled());
    assert!(bouton_suivante.isEnabled());
    assert!(aleatoire2);
    assert_eq!(
        intervalles2,
        vec![(60, false), (300, false), (900, true), (1800, false), (3600, false)]
    );
    assert_eq!(ecrans2.iter().filter(|(_, c)| *c).count(), 1);
    assert_eq!(
        unsafe { &*etat }.cibles_application().len(),
        unsafe { &*etat }.ecrans.len(),
        "image commune non refletee dans les cibles"
    );
    assert_eq!(unsafe { &*etat }.cibles_diaporama().len(), 1);
    // L’ancien élément ne doit plus être attaché au menu courant : la recette
    // ne réutilise que les éléments relus après chaque ouverture.
    assert!(
        unsafe { zero.menu() }
            .map(|m| &*m as *const NSMenu as usize)
            .is_none_or(|adresse| adresse != &*m2 as *const NSMenu as usize),
        "un ancien element est encore attache au menu rouvert"
    );
    assert!(
        m2.itemArray()
            .iter()
            .all(|i| &*i as *const NSMenuItem as usize
                != &*zero as *const NSMenuItem as usize),
        "element invalide reutilise dans le nouveau menu"
    );
    println!(
        "OK : etats coches (image commune, tri, aleatoire, intervalle, participation) et preferences concordantes, aucune application de fond"
    );

    // 5. Aucun écran participant : lecture/suivant indisponibles.
    let diapo2m = sous_menu(&diapo2, &titre(&diapo2));
    let ecrans2m = element(&diapo2m, 4);
    let sous_ecrans2 = sous_menu(&ecrans2m, "Participating displays");
    executer(&sous_ecrans2, 0); // retire le dernier écran participant
    assert_eq!(
        preferences::charger().diaporama_ecrans,
        Some(Vec::new()),
        "participation vide non persistee"
    );
    let m3 = ouvrir_menu(&cible, &bouton, &observateur);
    let diapo3 = element(&m3, 5);
    assert_eq!(titre(&diapo3), "Slideshow — 3 image(s)");
    let (demarrer3, suivante3, _, _, ecrans3) = etat_menu_diaporama(&diapo3);
    assert!(!demarrer3, "demarrage possible sans ecran participant");
    assert!(!suivante3, "image suivante possible sans ecran participant");
    assert!(!bouton_diaporama.isEnabled(), "bouton lecture actif sans ecran");
    assert!(!bouton_suivante.isEnabled(), "bouton suivant actif sans ecran");
    assert!(ecrans3.iter().all(|(_, c)| !*c));
    assert!(unsafe { &*etat }.plan_diaporama().is_empty());
    println!("OK : lecture et image suivante indisponibles sans ecran participant");

    // 6. Réactivation d’un écran : lecture/suivant de nouveau disponibles.
    let diapo3m = sous_menu(&diapo3, &titre(&diapo3));
    let ecrans3m = element(&diapo3m, 4);
    let sous_ecrans3 = sous_menu(&ecrans3m, "Participating displays");
    executer(&sous_ecrans3, 0);
    let m4 = ouvrir_menu(&cible, &bouton, &observateur);
    let diapo4 = element(&m4, 5);
    let (demarrer4, suivante4, _, _, ecrans4) = etat_menu_diaporama(&diapo4);
    assert!(demarrer4 && suivante4, "lecture non retablie apres reactivation");
    assert!(bouton_diaporama.isEnabled() && bouton_suivante.isEnabled());
    assert_eq!(ecrans4.iter().filter(|(_, c)| *c).count(), 1);
    assert!(!unsafe { &*etat }.diaporama_actif, "le diaporama ne doit pas demarrer");
    println!("OK : lecture et image suivante disponibles avec images et ecran valides (jamais declenches)");

    // 6 bis. Sélection indépendante par écran : aucune liste supprimée ;
    // ajout, retrait, filtre et compteur visent la liste pertinente.
    let cle_courante = unsafe { &*etat }
        .cle_ecran_courant()
        .expect("ecran courant")
        .to_owned();
    let m_ind = ouvrir_menu(&cible, &bouton, &observateur);
    assert_eq!(
        titre(&element(&m_ind, 1)),
        "Independent selection per display"
    );
    executer(&m_ind, 1); // active le mode indépendant
    let p = preferences::charger();
    assert!(p.selection_independante);
    assert_eq!(p.favoris.len(), 3, "le mode ne supprime pas la liste globale");
    let m_ind2 = ouvrir_menu(&cible, &bouton, &observateur);
    assert_eq!(
        element(&m_ind2, 1).state(),
        1,
        "indicateur de mode non coche"
    );
    let diapo_ind = element(&m_ind2, 5);
    assert_eq!(
        titre(&diapo_ind),
        "Slideshow — 0 image(s)",
        "un ecran sans liste dediee ne replie pas sur la liste globale"
    );
    let (demarrer_ind, suivante_ind, _, _, _) = etat_menu_diaporama(&diapo_ind);
    assert!(!demarrer_ind && !suivante_ind);
    assert!(unsafe { &*etat }.plan_diaporama().is_empty());
    // Ajout du dossier : uniquement dans la liste de l'écran courant.
    let menu_ind = sous_menu(&diapo_ind, &titre(&diapo_ind));
    executer(&menu_ind, 6);
    let p = preferences::charger();
    assert_eq!(p.favoris.len(), 3, "la liste globale reste intacte");
    assert_eq!(
        p.favoris_par_ecran.get(&cle_courante).map(Vec::len),
        Some(3),
        "liste dediee non remplie"
    );
    let m_ind3 = ouvrir_menu(&cible, &bouton, &observateur);
    let diapo_ind3 = element(&m_ind3, 5);
    assert_eq!(titre(&diapo_ind3), "Slideshow — 3 image(s)");
    // Filtre : la galerie suit la liste pertinente.
    let menu_ind3 = sous_menu(&diapo_ind3, &titre(&diapo_ind3));
    executer(&menu_ind3, 8);
    assert_eq!(
        unsafe { &*etat }.cellules.len(),
        3,
        "filtre hors liste dediee"
    );
    executer(&menu_ind3, 8);
    // Retrait d'une image : uniquement dans la liste de l'écran courant.
    let item_image = objet(&unsafe { &*etat }.images[0].chemin, mtm);
    let _: () = unsafe { msg_send![&*cible, imageDiaporama: &*item_image] };
    let p = preferences::charger();
    assert_eq!(p.favoris.len(), 3, "la liste globale ne doit pas perdre d'image");
    assert_eq!(
        p.favoris_par_ecran.get(&cle_courante).map(Vec::len),
        Some(2)
    );
    // Retour au mode global : aucune liste dédiée n'est effacée.
    let m_global = ouvrir_menu(&cible, &bouton, &observateur);
    executer(&m_global, 1);
    let p = preferences::charger();
    assert!(!p.selection_independante);
    assert_eq!(
        p.favoris_par_ecran.get(&cle_courante).map(Vec::len),
        Some(2),
        "les listes dediees survivent au changement de mode"
    );
    assert_eq!(p.favoris.len(), 3);
    println!(
        "OK : selection independante par ecran, compteur, filtre, ajout/retrait cibles et listes conservees"
    );

    // Régression : l'écran de référence est le second, avec une image déjà
    // au milieu de sa liste. Le mode commun doit avancer depuis CET écran.
    if unsafe { &*etat }.ecrans.len() >= 2 {
        let (reglages_avant, ecran_avant, dernier) = {
            let e = unsafe { &mut *etat };
            let reglages = e.reglages.clone();
            let courant = e.ecran_courant;
            let fichiers: Vec<String> = e.images.iter().map(|i| i.chemin.to_string_lossy().into_owned()).collect();
            assert_eq!(fichiers.len(), 3);
            let premier = e.ecrans[0].cle.clone();
            let reference = e.ecrans[1].cle.clone();
            e.reglages.selection_independante = true;
            e.reglages.meme_image_partout = true;
            e.reglages.diaporama_aleatoire = false;
            e.reglages.diaporama_ecrans = None;
            e.reglages.favoris_par_ecran.clear();
            e.reglages.favoris_par_ecran.insert(reference.clone(), fichiers.clone());
            e.reglages.image_par_ecran.insert(premier, "hors-selection".into());
            e.reglages.image_par_ecran.insert(reference, fichiers[1].clone());
            e.ecran_courant = 1;
            (reglages, courant, fichiers[2].clone())
        };
        let plan = unsafe { &*etat }.plan_diaporama();
        assert!(!plan.is_empty());
        assert!(plan.iter().all(|(_, image)| image == &dernier),
                "le mode commun doit suivre l'écran sélectionné, pas le premier : {plan:?}");
        let e = unsafe { &mut *etat };
        e.reglages = reglages_avant;
        e.ecran_courant = ecran_avant;
        println!("OK : mode commun séquentiel depuis le second écran de référence");
    }

    // 7. Favoris de dossiers : vide -> peuplé -> chemin représenté correct.
    let action = bouton_favori.action().expect("action du bouton favori");
    assert!(unsafe {
        app.sendAction_to_from(action, bouton_favori.target().as_deref(), Some(&bouton_favori))
    });
    assert_eq!(preferences::charger().dossiers_favoris, vec![a.to_string_lossy().into_owned()]);
    let m5 = ouvrir_menu(&cible, &bouton, &observateur);
    let favoris5 = sous_menu(&element(&m5, 6), "Favorite folders");
    assert_eq!(favoris5.numberOfItems(), 3, "favori + separateur + retrait attendus");
    let favori5 = element(&favoris5, 0);
    assert_eq!(titre(&favori5), a.to_string_lossy());
    verifier_action(&favori5, objc2::sel!(ouvrirFavori:), cible_ptr);
    assert_eq!(represente(&favori5), Some(a.clone()));
    assert_eq!(favori5.state(), 1, "dossier courant non coche");
    let retirer5 = element(&favoris5, 2);
    assert_eq!(titre(&retirer5), "Remove favorite folder");
    let sous_retirer5 = sous_menu(&retirer5, "Remove favorite folder");
    assert_eq!(sous_retirer5.numberOfItems(), 1);
    verifier_action(&element(&sous_retirer5, 0), objc2::sel!(retirerFavori:), cible_ptr);
    assert_eq!(represente(&element(&sous_retirer5, 0)), Some(a.clone()));

    // Navigation autorisée : uniquement des dossiers temporaires.
    ouvrir(&cible, &b, mtm);
    assert_eq!(unsafe { &*etat }.dossier, b);
    assert_eq!(preferences::charger().dossier, b.to_string_lossy().into_owned());
    assert!(unsafe {
        app.sendAction_to_from(action, bouton_favori.target().as_deref(), Some(&bouton_favori))
    });
    let m6 = ouvrir_menu(&cible, &bouton, &observateur);
    let favoris6 = sous_menu(&element(&m6, 6), "Favorite folders");
    assert_eq!(favoris6.numberOfItems(), 4, "deux favoris + separateur + retrait");
    assert_eq!(titre(&element(&favoris6, 0)), a.to_string_lossy());
    assert_eq!(titre(&element(&favoris6, 1)), b.to_string_lossy());
    assert_eq!(element(&favoris6, 0).state(), 0);
    assert_eq!(element(&favoris6, 1).state(), 1, "dossier courant non coche");
    assert_eq!(represente(&element(&favoris6, 0)), Some(a.clone()));
    assert_eq!(represente(&element(&favoris6, 1)), Some(b.clone()));
    // Vérifier la navigation avec les vraies entrées des favoris, pas seulement
    // avec un NSMenuItem fabriqué par le helper de préparation des fixtures.
    executer(&favoris6, 0);
    assert_eq!(unsafe { &*etat }.dossier, a);
    assert_eq!(preferences::charger().dossier, a.to_string_lossy());
    let menu_navigation = ouvrir_menu(&cible, &bouton, &observateur);
    let favoris_navigation = sous_menu(&element(&menu_navigation, 6), "Favorite folders");
    assert_eq!(element(&favoris_navigation, 0).state(), 1);
    assert_eq!(element(&favoris_navigation, 1).state(), 0);
    executer(&favoris_navigation, 1);
    assert_eq!(unsafe { &*etat }.dossier, b);
    assert_eq!(preferences::charger().dossier, b.to_string_lossy());
    let menu_retrait = ouvrir_menu(&cible, &bouton, &observateur);
    let favoris_retrait = sous_menu(&element(&menu_retrait, 6), "Favorite folders");
    let retirer6 = sous_menu(&element(&favoris_retrait, 3), "Remove favorite folder");
    assert_eq!(retirer6.numberOfItems(), 2);
    assert_eq!(represente(&element(&retirer6, 1)), Some(b.clone()));
    executer(&retirer6, 1); // retire Portraits
    assert_eq!(preferences::charger().dossiers_favoris, vec![a.to_string_lossy().into_owned()]);
    println!("OK : favoris vides puis peuples, chemins representes et retrait persistes");

    // 8. Ouvertures/fermetures répétées : éléments relus à chaque ouverture.
    // L’ancien élément est retenu pendant la réouverture : son adresse ne peut
    // donc pas être recyclée, et l’on vérifie qu’il n’est plus dans le menu.
    let mut ancien_element = Some(m6.itemAtIndex(0).expect("element initial"));
    for tour in 0..4 {
        let menu = ouvrir_menu(&cible, &bouton, &observateur);
        assert_eq!(menu.numberOfItems(), 13, "menu instable au tour {tour}");
        assert_eq!(titre(&element(&menu, 0)), "Same image on all displays");
        assert_eq!(sous_menu(&element(&menu, 6), "Favorite folders").numberOfItems(), 3);
        if let Some(ancien) = &ancien_element {
            assert!(
                menu.itemArray().iter().all(|i| (&*i as *const NSMenuItem)
                    != (&**ancien as *const NSMenuItem)),
                "un element du menu precedent est encore present (tour {tour})"
            );
        }
        ancien_element = menu.itemAtIndex(0);
    }
    println!("OK : ouvertures/fermetures repetees sans crash ni reutilisation d’un ancien element");

    // Cache AVIF : controles reels et conversion seulement si le cache est isole.
    ouvrir(&cible, &a, mtm);
    let _: () = unsafe { msg_send![&*cible, cacheAvif: &*bouton] };
    let panneau_avif = app.windows().iter().find(|w| w.title().to_string() == "AVIF Cache")
        .expect("panneau AVIF absent").retain();
    let contenu_avif = panneau_avif.contentView().unwrap();
    let convertir = bouton_action(&contenu_avif, objc2::sel!(convertirAvif:)).unwrap();
    let annuler = bouton_action(&contenu_avif, objc2::sel!(annulerAvif:)).unwrap();
    assert!(!annuler.isEnabled());
    let capable = mural::formats::avif_encodable() && mural::avif::dossier_cache().is_some();
    assert_eq!(convertir.isEnabled(), capable);
    if let Some(cache) = std::env::var_os("MURAL_AVIF_CACHE").map(PathBuf::from)
        .filter(|p| p.starts_with(racine)) {
        if capable {
            let originaux: Vec<_> = unsafe { &*etat }.images.iter()
                .map(|i| (i.chemin.clone(), std::fs::read(&i.chemin).unwrap())).collect();
            ouvrir(&cible, &b, mtm); // le panneau affiche encore le dossier A
            let _: () = unsafe { msg_send![&*cible, convertirAvif: &*convertir] };
            assert!(!convertir.isEnabled(), "double demarrage non desactive");
            assert!(annuler.isEnabled());
            // Une seconde demande pendant le travail doit etre ignoree.
            let _: () = unsafe { msg_send![&*cible, convertirAvif: &*convertir] };
            attendre(|| convertir.isEnabled());
            assert!(!annuler.isEnabled());
            assert!(bouton_action(&contenu_avif, objc2::sel!(ouvrirCacheAvif:)).unwrap().isEnabled());
            let fichiers: Vec<_> = std::fs::read_dir(&cache).unwrap().filter_map(|e|e.ok().map(|e|e.path()))
                .filter(|p|p.extension().is_some_and(|x| x == "avif")).collect();
            assert_eq!(fichiers.len(), 3);
            for f in &fichiers { assert!(mural::bibliotheque::dimensions(f).is_some()); }
            for (path,bytes) in &originaux { assert_eq!(std::fs::read(path).unwrap(), *bytes); }
            // Cache chaud : controle de reutilisation via les fichiers du backend.
            for (path,_) in &originaux { assert!(mural::avif::convertir(path, &cache).unwrap().reutilisee); }
            let _: () = unsafe { msg_send![&*cible, convertirAvif: &*convertir] };
            let _: () = unsafe { msg_send![&*cible, annulerAvif: &*annuler] };
            assert!(!annuler.isEnabled());
            attendre(|| convertir.isEnabled());
            // Fermer/reouvrir : meme fenetre retenue et controles utilisables.
            panneau_avif.close();
            let _: () = unsafe { msg_send![&*cible, cacheAvif: &*bouton] };
            assert!(panneau_avif.isVisible());
            println!("OK : panneau AVIF, conversion/copies relues, originaux intacts, cache chaud, refus double depart, annulation et reouverture");
        } else { println!("NON VERIFIE : conversion AVIF native indisponible"); }
    } else { println!("NON VERIFIE : conversion du panneau (MURAL_AVIF_CACHE isole requis)"); }
    panneau_avif.close();

    // 9. Fin : aucun minuteur de diaporama, préférences relues.
    assert!(!unsafe { &*etat }.diaporama_actif);
    let finales = preferences::charger();
    assert!(finales.diaporama_intervalle >= 60, "intervalle sous le minimum");
    assert_eq!(finales.diaporama_intervalle, 900);
    assert!(finales.meme_image_partout);
    assert_eq!(finales.tri, "date");
    assert!(finales.diaporama_aleatoire);
    // L’observateur est encore vivant et correspond aux inscriptions ci-dessus.
    unsafe { centre.removeObserver(&observateur) };
    fenetre.close();
    println!("OK : mode --menus autonome, terminaison sans interaction, diaporama jamais demarre");
}

fn main() {
    let chemin = std::env::var_os("MURAL_REGLAGES")
        .map(PathBuf::from)
        .expect("preferences de recette obligatoires");
    assert!(
        chemin.starts_with("/private/tmp/"),
        "recette limitee aux donnees temporaires"
    );
    let racine = chemin.parent().unwrap();
    let mtm = MainThreadMarker::new().unwrap();
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Regular);
    app.finishLaunching();
    if app.respondsToSelector(objc2::sel!(activate)) {
        app.activate();
    } else {
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(false);
    }
    if std::env::args().any(|a| a == "--panneau") {
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(120));
            eprintln!("ECHEC : recette --panneau au-dela de 120 s");
            std::process::exit(70);
        });
        mode_panneau(&app, mtm, racine);
        return;
    }
    if std::env::args().any(|a| a == "--menus") {
        // Bornage dur : la recette ne doit jamais dependre d’un clic humain.
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(120));
            eprintln!("ECHEC : recette --menus au-dela de 120 s (menu bloque ?)");
            std::process::exit(70);
        });
        mode_menus(&app, mtm, racine);
        return;
    }
    let restauration = std::env::args().any(|a| a == "--restauration");
    if restauration {
        let r = preferences::charger();
        assert_eq!(r.tri, "date");
        assert!(r.meme_image_partout);
        assert_eq!(r.dossiers_favoris.len(), 1);
        assert_eq!(
            r.favoris.len(),
            3,
            "selection images distincte des dossiers favoris"
        );
        let dossier = r
            .ecran_selectionne
            .as_deref()
            .and_then(|cle| r.dossier_ecran(cle))
            .expect("dossier ecran persistant");
        let (fenetre, etat) = mural::app::construire(mtm, dossier.clone(), false);
        assert_eq!(unsafe { &*etat }.tri, Tri::Date);
        assert!(
            !unsafe { &*etat }.diaporama_actif,
            "demarrage manuel apres relance"
        );
        assert_eq!(unsafe { &*etat }.dossier, dossier);
        assert!((fenetre.contentView().unwrap().bounds().size.width - 1120.0).abs() < 2.0);
        fenetre.close();
        println!(
            "OK : restauration dans un second processus, dossier, tri, ecran, favoris, option commune et dimensions"
        );
        return;
    }
    let a = racine.join("Paysages");
    let b = racine.join("Portraits");
    for (i, dossier) in [&a, &b].iter().enumerate() {
        std::fs::create_dir_all(dossier).unwrap();
        for n in 0..3 {
            let img = image::RgbImage::from_fn(
                if i == 0 { 360 } else { 220 },
                if i == 0 { 220 } else { 360 },
                |x, y| image::Rgb([(x % 255) as u8, (y % 255) as u8, (50 + n * 80) as u8]),
            );
            img.save(dossier.join(format!("image-{n}.png"))).unwrap();
        }
    }
    let reglages = preferences::Reglages {
        tri: "date".into(),
        largeur_fenetre: 1024.0,
        hauteur_fenetre: 700.0,
        ..Default::default()
    };
    preferences::sauvegarder(&reglages).unwrap();
    let (fenetre, etat) = mural::app::construire(mtm, a.clone(), true);
    let contenu = fenetre.contentView().unwrap();
    let cible = cible(etat);
    verifier_noms_vignettes(etat);
    println!("OK : chaque vignette expose son nom de fichier a l'accessibilite");
    assert_eq!(unsafe { &*etat }.tri, Tri::Date);
    let _: () = unsafe { msg_send![&*cible, dossierFavori: &*cible] };
    assert_eq!(
        preferences::charger().dossiers_favoris,
        vec![a.to_string_lossy().into_owned()]
    );
    let _: () = unsafe { msg_send![&*cible, dossierFavori: &*cible] };
    assert!(preferences::charger().dossiers_favoris.is_empty());
    let _: () = unsafe { msg_send![&*cible, dossierFavori: &*cible] };
    println!("OK : ajout, retrait et sauvegarde du dossier favori");
    let champ = trouver::<NSSearchField>(&contenu).unwrap();
    champ.setStringValue(&NSString::from_str("motif"));
    let _: () = unsafe { msg_send![&*cible, rechercher: &*champ] };
    ouvrir(&cible, &a, mtm);
    assert!(champ.stringValue().to_string().is_empty());
    assert!(unsafe { &*etat }.filtre.is_empty());
    let segments = trouver::<NSSegmentedControl>(&contenu).unwrap();
    if unsafe { &*etat }.ecrans.len() > 1 {
        segments.setSelectedSegment(1);
        let _: () = unsafe { msg_send![&*cible, ecranChange: &*segments] };
        ouvrir(&cible, &b, mtm);
        segments.setSelectedSegment(0);
        let _: () = unsafe { msg_send![&*cible, ecranChange: &*segments] };
        assert_eq!(unsafe { &*etat }.dossier, a);
        segments.setSelectedSegment(1);
        let _: () = unsafe { msg_send![&*cible, ecranChange: &*segments] };
        assert_eq!(unsafe { &*etat }.dossier, b);
        println!("OK : dossiers independants sur les deux ecrans et retour automatique");
    } else {
        ouvrir(&cible, &b, mtm);
        println!("NON VERIFIE : second ecran absent");
    }
    for _ in 0..5 {
        ouvrir(&cible, &a, mtm);
        ouvrir(&cible, &b, mtm);
    }
    attendre(|| unsafe { &*etat }.vignettes.iter().all(|v| v.is_some()));
    assert_eq!(unsafe { &*etat }.dossier, b);
    assert!(
        unsafe { &*etat }
            .images
            .iter()
            .all(|i| i.chemin.starts_with(&b))
    );
    verifier_noms_vignettes(etat);
    println!("OK : changements rapides, vignettes et noms accessibles du bon dossier apres decodage");
    let avant = unsafe { &*etat }.dossier.clone();
    ouvrir(&cible, &racine.join("absent"), mtm);
    assert_eq!(unsafe { &*etat }.dossier, avant);
    assert!(preferences::charger().dossier.contains("Portraits"));
    assert_eq!(unsafe { &*etat }.cibles_application().len(), 1);
    let _: () = unsafe { msg_send![&*cible, memeImagePartout: &*cible] };
    assert_eq!(
        unsafe { &*etat }.cibles_application().len(),
        unsafe { &*etat }.ecrans.len()
    );
    assert!(preferences::charger().meme_image_partout);
    println!("OK : cibles une/toutes, sans appliquer de fond");
    let (adresse, index) = unsafe { &*etat }
        .cellules
        .iter()
        .find(|(_, i)| **i == 2)
        .map(|(p, i)| (*p, *i))
        .unwrap();
    let bouton = unsafe { &*(adresse as *const NSButton) };
    let menu = bouton.menu().expect("menu contextuel");
    assert_eq!(menu.numberOfItems(), 5);
    for item in menu.itemArray() {
        let chemin = item
            .representedObject()
            .unwrap()
            .downcast::<NSString>()
            .unwrap()
            .to_string();
        assert_eq!(
            PathBuf::from(chemin),
            unsafe { &*etat }.images[index].chemin
        );
    }
    let item = menu.itemAtIndex(1).unwrap();
    let _: () = unsafe { msg_send![&*cible, apercuPleinEcran: &*item] };
    let plein = app
        .windows()
        .iter()
        .find(|w| w.title().to_string().contains("Esc"))
        .expect("fenetre apercu");
    attendre(|| plein.styleMask().contains(NSWindowStyleMask::FullScreen));
    assert_eq!(unsafe { &*etat }.selection, Some(2));
    let vue = plein.contentView().unwrap();
    assert!(vue.downcast_ref::<NSImageView>().unwrap().image().is_some());
    let _: () = unsafe { msg_send![&*plein, cancelOperation: std::ptr::null::<NSObject>()] };
    attendre(|| !plein.isVisible());
    println!("OK : menu contextuel cible sa propre image, plein ecran natif, fermeture par Echap");
    // Selection du diaporama et cycle de vie, sans appeler suivanteDiaporama:.
    let _: () = unsafe { msg_send![&*cible, ajouterDossierDiaporama: &*cible] };
    let _: () = unsafe { msg_send![&*cible, ajouterDossierDiaporama: &*cible] };
    assert_eq!(unsafe { &*etat }.reglages.favoris.len(), 3);
    let item_image = objet(&unsafe { &*etat }.images[0].chemin, mtm);
    let _: () = unsafe { msg_send![&*cible, imageDiaporama: &*item_image] };
    assert_eq!(unsafe { &*etat }.reglages.favoris.len(), 2);
    let _: () = unsafe { msg_send![&*cible, filtrerDiaporama: &*cible] };
    assert_eq!(unsafe { &*etat }.cellules.len(), 2);
    verifier_noms_vignettes(etat);
    let _: () = unsafe { msg_send![&*cible, filtrerDiaporama: &*cible] };
    let _: () = unsafe { msg_send![&*cible, imageDiaporama: &*item_image] };
    assert_eq!(unsafe { &*etat }.reglages.favoris.len(), 3);
    let cles: Vec<_> = unsafe { &*etat }
        .ecrans
        .iter()
        .map(|e| e.cle.clone())
        .collect();
    for cle in &cles {
        let item = objet(Path::new(cle), mtm);
        let _: () = unsafe { msg_send![&*cible, ecranDiaporama: &*item] };
    }
    assert!(unsafe { &*etat }.cibles_diaporama().is_empty());
    assert!(
        !bouton_action(&contenu, objc2::sel!(basculerDiaporama:))
            .unwrap()
            .isEnabled()
    );
    assert!(
        !bouton_action(&contenu, objc2::sel!(suivanteDiaporama:))
            .unwrap()
            .isEnabled()
    );
    let _: () = unsafe { msg_send![&*cible, basculerDiaporama: &*cible] };
    assert!(!unsafe { &*etat }.diaporama_actif);
    let item_ecran = objet(Path::new(&cles[0]), mtm);
    let _: () = unsafe { msg_send![&*cible, ecranDiaporama: &*item_ecran] };
    assert_eq!(unsafe { &*etat }.plan_diaporama().len(), 1);
    let intervalle = objet(&b, mtm);
    intervalle.setTag(3600);
    let _: () = unsafe { msg_send![&*cible, intervalleDiaporama: &*intervalle] };
    let _: () = unsafe { msg_send![&*cible, aleatoireDiaporama: &*cible] };
    let _: () = unsafe { msg_send![&*cible, basculerDiaporama: &*cible] };
    assert!(unsafe { &*etat }.diaporama_actif);
    // Le premier changement est dans une heure : aucun fond n’est modifie ici.
    fenetre.close();
    assert!(
        unsafe { &*etat }.diaporama_actif,
        "la fermeture ne suspend pas le diaporama"
    );
    let _: bool = unsafe {
        msg_send![&*cible, applicationShouldHandleReopen: &*app, hasVisibleWindows: false]
    };
    assert!(fenetre.isVisible());
    intervalle.setTag(900);
    let _: () = unsafe { msg_send![&*cible, intervalleDiaporama: &*intervalle] };
    assert!(unsafe { &*etat }.diaporama_actif);
    let _: () = unsafe { msg_send![&*cible, retirerDossierDiaporama: &*cible] };
    assert!(
        !unsafe { &*etat }.diaporama_actif,
        "selection vide arrete le minuteur"
    );
    let _: () = unsafe { msg_send![&*cible, ajouterDossierDiaporama: &*cible] };
    println!(
        "OK : selection images, absence de doublons, filtre, participants, refus sans cible, demarrage/pause, fermeture/reouverture et intervalle"
    );

    fenetre.setContentSize(objc2_foundation::NSSize {
        width: 1120.0,
        height: 720.0,
    });
    fenetre.close();
    let r = preferences::charger();
    assert_eq!(r.largeur_fenetre, 1120.0);
    assert_eq!(r.hauteur_fenetre, 720.0);
    assert_eq!(r.tri, "date");
    println!("OK : fermeture et ecriture des dimensions, tri et dossier");
}
