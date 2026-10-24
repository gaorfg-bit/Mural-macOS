//! Sparkle charge depuis le bundle uniquement ; Cargo et les sondes restent hors ligne.
use objc2::{MainThreadMarker, msg_send, rc::Retained, runtime::AnyClass, sel};
use objc2_app_kit::{NSMenu, NSMenuItem};
use objc2_foundation::{NSBundle, NSObject, NSString};
use std::cell::RefCell;

struct Controleur {
    _framework: Retained<NSBundle>,
    objet: Retained<NSObject>,
}
thread_local! {
    // AppKit ne retient pas les targets des menus. Le controleur doit survivre
    // jusqu'a la fermeture de l'application et rester sur son fil principal.
    static CONTROLEUR: RefCell<Option<Controleur>> = const { RefCell::new(None) };
}

fn charger() -> Result<Controleur, String> {
    let principal = NSBundle::mainBundle();
    let dossier = principal
        .privateFrameworksPath()
        .ok_or("Lancer Mural.app pour les mises a jour")?;
    let chemin = NSString::from_str(&format!("{dossier}/Sparkle.framework"));
    let framework =
        NSBundle::bundleWithPath(&chemin).ok_or("Sparkle.framework absent du bundle")?;
    if !unsafe { framework.load() } {
        return Err("Impossible de charger Sparkle.framework".into());
    }
    let classe =
        AnyClass::get(c"SPUStandardUpdaterController").ok_or("Controleur Sparkle introuvable")?;
    let objet: Retained<NSObject> = unsafe {
        let alloue: objc2::rc::Allocated<NSObject> = msg_send![classe, alloc];
        msg_send![alloue, initWithStartingUpdater: false,
            updaterDelegate: std::ptr::null::<NSObject>(),
            userDriverDelegate: std::ptr::null::<NSObject>()]
    };
    Ok(Controleur {
        _framework: framework,
        objet,
    })
}

pub fn installer(menu: &NSMenu, mtm: MainThreadMarker) {
    let element = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &NSString::from_str("Check for Updates…"),
            Some(sel!(checkForUpdates:)),
            &NSString::from_str(""),
        )
    };
    CONTROLEUR.with(|cellule| {
        let mut controleur = cellule.borrow_mut();
        if controleur.is_none() {
            match charger() {
                Ok(nouveau) => *controleur = Some(nouveau),
                Err(erreur) => {
                    crate::journal::info(&format!("Mises a jour indisponibles : {erreur}"))
                }
            }
        }
        if let Some(controleur) = controleur.as_ref() {
            unsafe {
                element.setTarget(Some(&*controleur.objet));
            }
        } else {
            // Eviter qu'AppKit ne route l'action vers un autre premier repondeur.
            unsafe {
                element.setAction(None);
            }
            element.setEnabled(false);
        }
    });
    menu.addItem(&element);
}

/// Appele uniquement par l'application interactive, jamais par construire/les recettes.
pub fn demarrer(_mtm: MainThreadMarker) {
    CONTROLEUR.with(|cellule| {
        if let Some(controleur) = cellule.borrow().as_ref() {
            unsafe {
                let _: () = msg_send![&*controleur.objet, startUpdater];
            }
        }
    });
}
