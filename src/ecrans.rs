//! Ecrans connectes — equivalent de `mural/monitors.py` (portage macOS).
//!
//! Identite macOS NSScreenNumber ; l’index ne sert qu’au classement visuel.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::NSScreen;

/// Un ecran connecte.
#[derive(Debug, Clone, PartialEq)]
pub struct Ecran {
    /// Position dans `NSScreen::screens` (valable pour la session courante).
    pub index: usize,
    pub identifiant: u32,
    /// Cle persistante Apple pour les reglages, distincte de l’index visuel.
    pub cle: String,
    /// Nom lisible donne par le systeme (ex. « DELL U2720Q »).
    pub nom: String,
    /// Largeur en points.
    pub largeur: i64,
    /// Hauteur en points.
    pub hauteur: i64,
    /// Origine X dans l'espace virtuel.
    pub x: f64,
    /// Origine Y dans l'espace virtuel.
    pub y: f64,
    /// Vrai pour l'ecran principal.
    pub principal: bool,
}

/// Liste les ecrans. Renvoie un vecteur vide si l'on n'est pas sur le fil principal.
pub fn lister() -> Vec<Ecran> {
    let Some(mtm) = MainThreadMarker::new() else {
        return Vec::new();
    };
    let ecrans = NSScreen::screens(mtm);
    let principal = NSScreen::mainScreen(mtm);
    let mut resultat = Vec::with_capacity(ecrans.count());

    for index in 0..ecrans.count() {
        let ecran = ecrans.objectAtIndex(index);
        let cadre = ecran.frame();
        let est_principal = match &principal {
            Some(p) => Retained::as_ptr(&ecran) == Retained::as_ptr(p),
            None => index == 0,
        };
        resultat.push(Ecran {
            index,
            identifiant: identifiant(&ecran),
            cle: cle_persistante(identifiant(&ecran)),
            nom: ecran.localizedName().to_string(),
            largeur: cadre.size.width as i64,
            hauteur: cadre.size.height as i64,
            x: cadre.origin.x,
            y: cadre.origin.y,
            principal: est_principal,
        });
    }
    resultat
}

/// Retrouve l'ecran `NSScreen` correspondant a une position de la liste.
///
/// Doit etre appele sur le fil principal (usage normal : depuis l'interface).
pub fn retrouver(index: usize) -> Option<Retained<NSScreen>> {
    let mtm = MainThreadMarker::new()?;
    let ecrans = NSScreen::screens(mtm);
    if index >= ecrans.count() {
        return None;
    }
    Some(ecrans.objectAtIndex(index))
}

/// Ecran principal, s'il existe.
pub fn principal() -> Option<Retained<NSScreen>> {
    let mtm = MainThreadMarker::new()?;
    NSScreen::mainScreen(mtm)
}

/// Identite de l’ecran dans la configuration courante, independante de l’ordre.
fn identifiant(ecran: &NSScreen) -> u32 {
    let description = ecran.deviceDescription();
    let nombre: Option<Retained<objc2_foundation::NSNumber>> = unsafe {
        objc2::msg_send![&*description, objectForKey: &*objc2_foundation::NSString::from_str("NSScreenNumber")]
    };
    nombre.map(|n| n.unsignedIntValue()).unwrap_or(0)
}

pub fn retrouver_identifiant(id: u32) -> Option<Retained<NSScreen>> {
    let mtm = MainThreadMarker::new()?;
    NSScreen::screens(mtm)
        .iter()
        .find(|e| identifiant(e) == id && id != 0)
}

/// API publique ColorSync ; la crate ne fournit pas encore ce binding.
fn cle_persistante(id: u32) -> String {
    use objc2_core_foundation::{CFRetained, CFUUID};
    #[link(name = "ColorSync", kind = "framework")]
    unsafe extern "C" {
        fn CGDisplayCreateUUIDFromDisplayID(id: u32) -> *mut CFUUID;
    }
    // Surete : ID fourni par NSScreen ; regle Create => reference possedee.
    let uuid = std::ptr::NonNull::new(unsafe { CGDisplayCreateUUIDFromDisplayID(id) });
    if let Some(uuid) = uuid {
        let uuid = unsafe { CFRetained::from_raw(uuid) };
        if let Some(texte) = CFUUID::new_string(None, Some(&uuid)) {
            return texte.to_string();
        }
    }
    format!("display-{id}")
}
