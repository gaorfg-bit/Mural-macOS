//! Recette native de composition. Aucun appel aux actions de fond d'ecran.
use objc2::{MainThreadMarker, runtime::AnyClass};
use objc2_foundation::NSObjectProtocol;
use objc2_app_kit::{NSApplication, NSSplitView, NSVisualEffectView};
use objc2_foundation::NSSize;
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
    let (fenetre, _etat) = mural::app::construire(mtm, dossier.into(), false);
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
    for index in 1..4 {
        let panneau = enfants.objectAtIndex(index);
        assert!(
            AnyClass::get(c"NSGlassEffectView").is_some_and(|classe| panneau.isKindOfClass(classe))
                || panneau.downcast_ref::<NSVisualEffectView>().is_some(),
            "chaque panneau doit utiliser Glass, meme avec un ancien reglage Flat"
        );
    }
    println!(
        "OK : panneaux caches, apercu borne et redimensionnable, proportions conservees, Glass uniquement ; aucun fond applique"
    );
}
