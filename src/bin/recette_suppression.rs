//! Suppression d'image de bout en bout : action du menu, confirmation, fichier,
//! reglages et galerie. Fichiers synthetiques dans un dossier temporaire,
//! reponse du dialogue imposee, aucun fond applique.
use objc2::{MainThreadMarker, msg_send};
use objc2_app_kit::{NSApplication, NSButton, NSMenuItem};
use objc2_foundation::{NSObject, NSString};
use std::path::PathBuf;

fn element(chemin: &std::path::Path) -> objc2::rc::Retained<NSMenuItem> {
    let mtm = MainThreadMarker::new().unwrap();
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &NSString::from_str("image"),
            None,
            &NSString::from_str(""),
        )
    };
    unsafe { item.setRepresentedObject(Some(&NSString::from_str(&chemin.to_string_lossy()))) };
    item
}

fn main() {
    let reglages = PathBuf::from(
        std::env::var_os("MURAL_REGLAGES").expect("Reglages temporaires obligatoires"),
    );
    assert!(reglages.starts_with("/private/tmp/"), "dossier temporaire obligatoire");
    let racine = reglages.parent().unwrap().join("images");
    assert!(!racine.exists(), "dossier de recette neuf obligatoire");
    std::fs::create_dir_all(&racine).unwrap();
    let [a, b, c] = ["a.png", "b.png", "c.png"].map(|n| racine.join(n));
    for chemin in [&a, &b, &c] {
        image::RgbImage::new(64, 32).save(chemin).unwrap();
    }
    let mtm = MainThreadMarker::new().unwrap();
    let _app = NSApplication::sharedApplication(mtm);
    let (fenetre, etat) = mural::app::construire(mtm, racine.clone(), false);
    assert_eq!(unsafe { &*etat }.images.len(), 3);
    {
        let etat = unsafe { &mut *etat };
        let ta = a.to_string_lossy().into_owned();
        etat.reglages.favoris.push(ta.clone());
        etat.reglages.favoris_par_ecran.insert("uuid".into(), vec![ta.clone()]);
        etat.reglages.image_par_ecran.insert("uuid".into(), ta);
    }
    let adresse = *unsafe { &*etat }.cellules.keys().next().unwrap();
    let bouton = unsafe { &*(adresse as *const NSButton) };
    let cible = bouton.target().unwrap().downcast::<NSObject>().unwrap();

    // 1. Refus : rien ne change.
    let _: () = unsafe { msg_send![&*cible, forcerReponseSuppression: 0isize] };
    let _: () = unsafe { msg_send![&*cible, supprimerImage: &*element(&a)] };
    assert!(a.exists(), "le refus ne doit rien supprimer");
    assert_eq!(unsafe { &*etat }.images.len(), 3);
    assert_eq!(unsafe { &*etat }.reglages.favoris.len(), 1);
    println!("OK : refus de la confirmation, fichier et reglages intacts");

    // 2. Confirmation : fichier supprime, galerie et reglages nettoyes, voisins intacts.
    let _: () = unsafe { msg_send![&*cible, forcerReponseSuppression: 1isize] };
    let _: () = unsafe { msg_send![&*cible, supprimerImage: &*element(&a)] };
    assert!(!a.exists(), "le fichier doit etre supprime");
    assert!(b.exists() && c.exists(), "les autres images restent");
    let etat_ref = unsafe { &*etat };
    let noms: Vec<_> = etat_ref.images.iter().map(|i| i.nom.as_str()).collect();
    assert_eq!(noms, ["b.png", "c.png"], "galerie non rechargee");
    assert_eq!(etat_ref.cellules.len(), 2);
    assert!(etat_ref.reglages.favoris.is_empty());
    assert!(etat_ref.reglages.favoris_par_ecran["uuid"].is_empty());
    assert!(!etat_ref.reglages.image_par_ecran.contains_key("uuid"));
    println!("OK : suppression confirmee, galerie rechargee, diaporama et fond enregistre nettoyes");

    // 3. Fichier deja absent : echec propre, voisins intacts.
    let _: () = unsafe { msg_send![&*cible, supprimerImage: &*element(&a)] };
    assert_eq!(unsafe { &*etat }.images.len(), 2);
    assert!(b.exists() && c.exists());
    println!("OK : suppression d'un fichier absent sans effet ; aucun fond applique");
    fenetre.close();
}
