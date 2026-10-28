//! Recette locale : aucun téléchargement ni remplacement d'application.
use objc2::{MainThreadMarker, msg_send};
use objc2_app_kit::NSApplication;
use objc2_foundation::{NSBundle, NSObject};

fn main() {
    assert!(
        std::env::var_os("MURAL_REGLAGES").is_some(),
        "Reglages isoles requis"
    );
    let mtm = MainThreadMarker::new().unwrap();
    let app = NSApplication::sharedApplication(mtm);
    let (_fenetre, _etat) = mural::app::construire(
        mtm,
        "/private/tmp/mural-update-images-absentes".into(),
        false,
    );
    let menu = app
        .mainMenu()
        .unwrap()
        .itemAtIndex(0)
        .unwrap()
        .submenu()
        .unwrap();
    let element = menu
        .itemArray()
        .iter()
        .find(|i| i.title().to_string() == "Check for Updates…")
        .expect("Commande de mise a jour absente");
    let cible = element.target().expect("Controleur Sparkle absent");
    assert_eq!(element.action(), Some(objc2::sel!(checkForUpdates:)));
    // Demarrer le moteur sans verifier le flux : les controles automatiques sont
    // desactives dans le bundle de recette, sans modifier les preferences reelles.
    let moteur: objc2::rc::Retained<NSObject> = unsafe { msg_send![&*cible, updater] };
    let mut erreur: Option<objc2::rc::Retained<objc2_foundation::NSError>> = None;
    let ok: bool = unsafe { msg_send![&*moteur, startUpdater: &mut erreur] };
    assert!(ok, "Sparkle refuse la configuration : {erreur:?}");
    let automatique: bool = unsafe { msg_send![&*moteur, automaticallyChecksForUpdates] };
    assert!(!automatique, "La recette ne doit pas consulter le reseau");
    let flux: objc2::rc::Retained<objc2_foundation::NSURL> =
        unsafe { msg_send![&*moteur, feedURL] };
    assert_eq!(
        flux.absoluteString().unwrap().to_string(),
        "https://mural-updates.mural-updates.workers.dev/appcast.xml"
    );
    assert!(NSBundle::mainBundle().bundleIdentifier().is_some());
    println!(
        "OK : menu cible Sparkle, moteur demarre, flux correct ; aucun telechargement ni installation"
    );
}
