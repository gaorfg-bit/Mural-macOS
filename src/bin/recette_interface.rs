//! Recette native de composition. Aucun appel aux actions de fond d'ecran.
use objc2::{MainThreadMarker, Message, msg_send};
use objc2_app_kit::{NSApplication, NSButton, NSMenuItem, NSSplitView, NSView};
use objc2_foundation::{NSObjectProtocol, NSSize};
fn compter(vue: &NSView) -> usize {
    vue.subviews().iter().map(|v| compter(&v)).sum::<usize>()
        + usize::from(
            vue.downcast_ref::<NSButton>()
                .is_some_and(|b| b.action().is_some()),
        )
}
fn main() {
    assert!(
        std::env::var("MURAL_REGLAGES").is_ok(),
        "MURAL_REGLAGES obligatoire"
    );
    let mtm = MainThreadMarker::new().unwrap();
    let _app = NSApplication::sharedApplication(mtm);
    let dossier = std::env::args()
        .nth(1)
        .unwrap_or("maquettes/mural-v1/assets".into());
    let (fenetre, etat) = mural::app::construire(mtm, dossier.into(), false);
    let racine = fenetre.contentView().unwrap();
    assert!(racine.class().to_string().contains("MuralSurface"));
    let enfants = racine.subviews();
    assert_eq!(
        enfants.count(),
        4,
        "split et trois panneaux, sans barre permanente"
    );
    let premier = enfants.objectAtIndex(0);
    let split = premier.downcast_ref::<NSSplitView>().unwrap();
    assert!(split.isFlipped());
    assert_eq!(split.dividerThickness(), 1.0);
    for index in 1..4 {
        assert!(enfants.objectAtIndex(index).isHidden());
    }
    split.setPosition_ofDividerAtIndex(20.0, 0);
    assert!(split.subviews().objectAtIndex(0).frame().size.height >= 110.0);
    split.setPosition_ofDividerAtIndex(2000.0, 0);
    assert!(split.subviews().objectAtIndex(1).frame().size.height >= 139.0);
    split.setPosition_ofDividerAtIndex(384.0, 0);
    let avant = split.subviews().objectAtIndex(0).frame().size.height / split.bounds().size.height;
    fenetre.setContentSize(NSSize::new(1100.0, 700.0));
    racine.layoutSubtreeIfNeeded();
    let apres = split.subviews().objectAtIndex(0).frame().size.height / split.bounds().size.height;
    assert!(
        (avant - apres).abs() < 0.01,
        "proportion d'apercu perdue au redimensionnement"
    );
    let adresse = *unsafe { &*etat }.cellules.keys().next().unwrap();
    let cible = unsafe { &*(adresse as *const NSButton) }.target().unwrap();
    let elements_avant = compter(&racine);
    for tag in [0, 1, 0, 1] {
        let expediteur = NSMenuItem::new(mtm);
        expediteur.setTag(tag);
        unsafe {
            let _: () = msg_send![&*cible, basculerMatiere: &*expediteur];
        }
        assert_eq!(
            unsafe { &*etat }.reglages.champs_inconnus["interface_verre"].as_bool(),
            Some(tag == 1)
        );
        // Les enveloppes internes de NSGlassEffectView different, les boutons restent presents.
        assert_eq!(
            compter(&racine),
            elements_avant,
            "commande perdue au changement de matiere"
        );
        assert_eq!(
            racine.subviews().count(),
            4,
            "panneaux accumules au changement d'apparence"
        );
    }
    assert_eq!(
        mural::preferences::charger().champs_inconnus["interface_verre"].as_bool(),
        Some(true)
    );
    println!(
        "OK : panneaux caches, apercu borne et redimensionnable, proportions conservees, Flat/Glass alterne et persiste ; aucun fond applique"
    );
}
