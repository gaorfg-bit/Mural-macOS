//! Rendu hors ecran de l'interface, pour verification visuelle.
//!
//! Construit la meme fenetre que l'application, sans l'afficher, puis dessine
//! son contenu dans un bitmap et l'ecrit en PNG. Aucune permission systeme
//! n'est necessaire : c'est un dessin en memoire, pas une capture d'ecran.
//!
//! Usage : rendu <sortie.png> [dossier]

use std::path::PathBuf;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSApplication, NSBitmapImageFileType, NSView,
};
use objc2_foundation::{NSDictionary, NSString};

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let sortie = arguments
        .get(1)
        .cloned()
        .unwrap_or_else(|| "rendu.png".to_string());
    let dossier = arguments.get(2).map(PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join("Pictures")
    });

    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("rendu doit tourner sur le fil principal");
        std::process::exit(1);
    };
    let _application = NSApplication::sharedApplication(mtm);
    // Le rendu hors ecran doit fixer une apparence : sans cela, les couleurs
    // semantiques (texte des boutons) suivent le systeme tandis que les fonds
    // restent en Aqua, et les libelles disparaissent.
    if let Ok(apparence) = std::env::var("MURAL_APPARENCE") {
        unsafe {
            _application.setAppearance(Some(
                &NSAppearance::appearanceNamed(&NSString::from_str(if apparence == "dark" {
                    "NSAppearanceNameDarkAqua"
                } else {
                    "NSAppearanceNameAqua"
                }))
                .unwrap_or_else(|| {
                    NSAppearance::appearanceNamed(&NSString::from_str("NSAppearanceNameAqua"))
                        .unwrap()
                }),
            ));
        }
    }

    let (fenetre, _etat) = mural::app::construire(mtm, dossier, false);
    if std::env::var("MURAL_APPARENCE").as_deref() == Ok("dark") {
        // Dans un binaire sans bundle, la fenetre reste en Aqua : on force
        // l'apparence pour que fonds et texte soient coherents.
        unsafe {
            fenetre.setAppearance(Some(
                &NSAppearance::appearanceNamed(&NSString::from_str("NSAppearanceNameDarkAqua"))
                    .unwrap(),
            ));
        }
    }
    let Some(contenu) = fenetre.contentView() else {
        eprintln!("pas de vue de contenu");
        std::process::exit(2);
    };

    contenu.layoutSubtreeIfNeeded();
    let cadre = contenu.bounds();
    let Some(bitmap) = contenu.bitmapImageRepForCachingDisplayInRect(cadre) else {
        eprintln!("pas de bitmap");
        std::process::exit(3);
    };
    contenu.cacheDisplayInRect_toBitmapImageRep(cadre, &bitmap);

    let vide: Retained<NSDictionary<objc2_foundation::NSString, objc2::runtime::AnyObject>> =
        NSDictionary::from_slices::<objc2_foundation::NSString>(&[], &[]);
    let Some(donnees) =
        (unsafe { bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &vide) })
    else {
        eprintln!("encodage PNG impossible");
        std::process::exit(4);
    };

    let octets = donnees.to_vec();
    match std::fs::write(&sortie, &octets) {
        Ok(()) => println!(
            "rendu ecrit: {} ({} octets, {}x{})",
            sortie,
            octets.len(),
            cadre.size.width as i64,
            cadre.size.height as i64
        ),
        Err(erreur) => {
            eprintln!("ecriture impossible : {erreur}");
            std::process::exit(5);
        }
    }
    let _ = NSView::new(mtm);
}
