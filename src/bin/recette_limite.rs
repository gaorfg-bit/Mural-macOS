//! Limite d'affichage : au-dela de 500 images, la barre d'etat le dit.
//! Images synthetiques dans un dossier temporaire, aucun fond applique.
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSTextField, NSView};
use std::path::PathBuf;

fn contient_texte(vue: &NSView, texte: &str) -> bool {
    vue.downcast_ref::<NSTextField>()
        .is_some_and(|v| v.stringValue().to_string().contains(texte))
        || vue.subviews().iter().any(|v| contient_texte(&v, texte))
}

fn main() {
    let reglages = PathBuf::from(std::env::var_os("MURAL_REGLAGES").expect("Reglages temporaires obligatoires"));
    assert!(reglages.starts_with("/private/tmp/"), "dossier temporaire obligatoire");
    let racine = reglages.parent().unwrap().join("images");
    assert!(!racine.exists(), "dossier de recette neuf obligatoire");
    std::fs::create_dir_all(&racine).unwrap();
    // 503 petites images : le PNG de 1 pixel suffit, le contenu n'a pas d'importance.
    let modele = racine.join("modele.png");
    image::RgbImage::new(4, 4).save(&modele).unwrap();
    for i in 0..503 {
        std::fs::copy(&modele, racine.join(format!("img{i:04}.png"))).unwrap();
    }
    std::fs::remove_file(&modele).unwrap();
    let mtm = MainThreadMarker::new().unwrap();
    let _app = NSApplication::sharedApplication(mtm);
    let (fenetre, etat) = mural::app::construire(mtm, racine.clone(), false);
    let etat = unsafe { &*etat };
    assert_eq!(etat.images.len(), 500, "la galerie est limitee a 500 images");
    assert_eq!(etat.total_images, 503, "le total du dossier doit etre connu");
    let contenu = fenetre.contentView().unwrap();
    assert!(
        contient_texte(&contenu, "showing 500 of 503 images"),
        "la barre d'etat ne signale pas la limite"
    );
    println!("OK : 503 images, 500 affichees, limite signalee dans la barre d'etat");
    fenetre.close();
}
