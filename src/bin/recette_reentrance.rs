//! Change de dossier pendant une notification synchrone de construction du menu.
//! Images synthetiques, aucun appel d'application de fond.
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel,
};
use objc2_app_kit::{NSApplication, NSButton, NSMenu, NSMenuItem};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol, NSString,
};
use std::{cell::Cell, path::PathBuf};

struct Ivars {
    cible: Retained<NSObject>,
    dossier: Retained<NSMenuItem>,
    declenche: Cell<bool>,
    favoris: Option<String>,
}
define_class!(
    #[unsafe(super(NSObject))]
    #[name = "MuralRecetteReentrance"]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct Observateur;
    unsafe impl NSObjectProtocol for Observateur {}
    impl Observateur {
        #[unsafe(method(menuAjoute:))]
        fn menu_ajoute(&self, notification: &NSNotification) {
            if self.ivars().declenche.get() { return; }
            let Some(objet) = notification.object() else { return; };
            let Some(menu) = objet.downcast_ref::<NSMenu>() else { return; };
            let vise = match self.ivars().favoris.as_ref() {
                Some(dossier) => menu.itemArray().iter().any(|i| i.representedObject()
                    .and_then(|o| o.downcast_ref::<NSString>().map(|s| s.to_string())).as_ref() == Some(dossier)),
                None => menu.itemArray().iter().any(|i| i.action() == Some(sel!(appliquer:))),
            };
            if vise {
                self.ivars().declenche.set(true);
                let _: () = unsafe { msg_send![&*self.ivars().cible, ouvrirFavori: &*self.ivars().dossier] };
                if self.ivars().favoris.is_some() {
                    let _: () = unsafe { msg_send![&*self.ivars().cible, dossierFavori: &*self.ivars().cible] };
                }
            }
        }
    }
);
fn main() {
    let chemin = PathBuf::from(
        std::env::var_os("MURAL_REGLAGES").expect("Reglages temporaires obligatoires"),
    );
    assert!(chemin.starts_with("/private/tmp/"));
    let racine = chemin.parent().unwrap();
    let images = racine.join("images");
    let vide = racine.join("vide");
    assert!(!images.exists(), "dossier de recette neuf obligatoire");
    std::fs::create_dir_all(&images).unwrap();
    std::fs::create_dir_all(&vide).unwrap();
    image::RgbImage::new(64, 32)
        .save(images.join("unique.png"))
        .unwrap();
    let mtm = MainThreadMarker::new().unwrap();
    let _app = NSApplication::sharedApplication(mtm);
    let mode = std::env::args().nth(1).unwrap_or_default();
    let panneau = mode.starts_with("--panneau");
    let autre = racine.join("autre");
    let troisieme = racine.join("troisieme");
    std::fs::create_dir_all(&autre).unwrap();
    std::fs::create_dir_all(&troisieme).unwrap();
    if panneau {
        let r = mural::preferences::Reglages {
            dossiers_favoris: vec![
                images.to_string_lossy().into_owned(),
                autre.to_string_lossy().into_owned(),
            ],
            ..Default::default()
        };
        mural::preferences::sauvegarder(&r).unwrap();
    }
    if mode == "--preferences" {
        std::fs::write(&chemin, b"{\"favoris\": [").unwrap();
    }
    let (fenetre, etat) = mural::app::construire(mtm, images.clone(), false);
    if mode == "--preferences" {
        assert!(
            unsafe { &*etat }
                .erreur_sauvegarde
                .as_ref()
                .is_some_and(|e| e.contains("Original file preserved"))
        );
        fenetre.close();
        assert_eq!(std::fs::read(&chemin).unwrap(), b"{\"favoris\": [");
        println!(
            "OK : erreur de lecture visible dans l'etat, JSON original intact apres fermeture"
        );
        return;
    }
    let adresse = *unsafe { &*etat }.cellules.keys().next().unwrap();
    let bouton = unsafe { &*(adresse as *const NSButton) };
    let cible = bouton.target().unwrap().downcast::<NSObject>().unwrap();
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &NSString::from_str("vide"),
            None,
            &NSString::from_str(""),
        )
    };
    unsafe {
        item.setRepresentedObject(Some(&NSString::from_str(&vide.to_string_lossy())));
    }
    if mode == "--panneau" {
        let _: () = unsafe { msg_send![&*cible, ouvrirReglages: &*cible] };
        let choix = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &NSString::from_str("troisieme"),
                None,
                &NSString::from_str(""),
            )
        };
        unsafe {
            choix.setRepresentedObject(Some(&NSString::from_str(&troisieme.to_string_lossy())));
        }
        let _: () = unsafe { msg_send![&*cible, ouvrirFavori: &*choix] };
    }
    let observateur = mtm.alloc::<Observateur>().set_ivars(Ivars {
        cible: cible.clone(),
        dossier: item,
        declenche: Cell::new(false),
        favoris: panneau.then(|| images.to_string_lossy().into_owned()),
    });
    let observateur: Retained<Observateur> = unsafe { msg_send![super(observateur), init] };
    let centre = NSNotificationCenter::defaultCenter();
    unsafe {
        centre.addObserver_selector_name_object(
            &observateur,
            sel!(menuAjoute:),
            Some(objc2_app_kit::NSMenuDidAddItemNotification),
            None,
        );
    }
    if mode == "--panneau" {
        let _: () = unsafe { msg_send![&*cible, dossierFavori: &*cible] };
    } else if panneau {
        let _: () = unsafe { msg_send![&*cible, ouvrirReglages: &*cible] };
    } else {
        let _: () = unsafe { msg_send![&*cible, actualiser: &*cible] };
    }
    unsafe {
        centre.removeObserver(&observateur);
    }
    assert!(
        observateur.ivars().declenche.get(),
        "notification non exercee"
    );
    assert_eq!(unsafe { &*etat }.dossier, vide);
    assert!(unsafe { &*etat }.images.is_empty());
    assert!(
        unsafe { &*etat }.cellules.is_empty(),
        "anciennes cellules republiees apres le changement imbrique"
    );
    if panneau {
        let brut: *mut NSString = unsafe { msg_send![&*cible, instantaneReglages: &*cible] };
        let texte = unsafe { Retained::from_raw(brut) }.unwrap();
        let valeur: serde_json::Value = serde_json::from_str(&texte.to_string()).unwrap();
        assert_eq!(valeur["dossier_infobulle"], vide.to_string_lossy().as_ref());
        let cles: Vec<_> = valeur["favoris"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["cle"].as_str().unwrap().to_owned())
            .collect();
        let mut attendus = vec![
            images.to_string_lossy().into_owned(),
            autre.to_string_lossy().into_owned(),
        ];
        if mode == "--panneau" {
            attendus.push(troisieme.to_string_lossy().into_owned());
        }
        attendus.push(vide.to_string_lossy().into_owned());
        assert_eq!(
            cles, attendus,
            "favoris perimes ou dupliques apres callback imbrique"
        );
    }
    fenetre.close();
    println!(
        "OK : changement de dossier imbrique sans crash ni cellules obsoletes ; aucun fond applique"
    );
}
