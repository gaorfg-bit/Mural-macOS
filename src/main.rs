//! Mural — application macOS (Rust + AppKit via objc2).
//!
//! Ouvre la fenetre principale. Le dossier de depart vient des reglages ; a
//! defaut, celui des images de l'utilisateur.

use std::path::PathBuf;

use objc2::MainThreadMarker;

fn dossier_initial() -> PathBuf {
    let ecrans = mural::ecrans::lister();
    let reglages = mural::preferences::charger_avec_ecrans(&ecrans);
    let ecran = ecrans
        .iter()
        .find(|e| Some(&e.cle) == reglages.ecran_selectionne.as_ref())
        .or(ecrans.first());
    if let Some(ecran) = ecran {
        if let Some(dossier) = reglages.dossier_ecran(&ecran.cle) {
            return dossier;
        }
    }
    if !reglages.dossier.is_empty() {
        let chemin = PathBuf::from(&reglages.dossier);
        if chemin.is_dir() {
            return chemin;
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("Pictures")
}

fn main() {
    let precedent = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |information| {
        mural::journal::erreur(&format!("PANIC : {information}"));
        precedent(information);
    }));
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("Mural doit demarrer sur le fil principal");
        std::process::exit(1);
    };
    mural::app::lancer(mtm, dossier_initial());
}
