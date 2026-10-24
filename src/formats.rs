//! Capacites de decodage et capacite d'encodage reelle par ImageIO.
//!
//! Aucune hypothese sur le numero de macOS : la seule source de verite est la
//! liste d'identifiants UTI fournie par ImageIO, lue une fois par processus
//! (`OnceLock`). AVIF est reconnu par `public.avif`.

use std::sync::OnceLock;

use objc2_core_foundation::{CFArray, CFString};
use objc2_image_io::{CGImageDestination, CGImageSource};

/// Identifiant UTI de l'AVIF.
pub const IDENTIFIANT_AVIF: &str = "public.avif";

/// Vrai si `identifiants` annonce `identifiant` (comparaison insensible a la casse).
pub fn annonce(identifiants: &[String], identifiant: &str) -> bool {
    identifiants
        .iter()
        .any(|annonce| annonce.eq_ignore_ascii_case(identifiant))
}

/// Copie les identifiants UTI d'un tableau Core Foundation en chaines Rust.
fn identifiants(array: &CFArray) -> Vec<String> {
    let mut liste = Vec::with_capacity(array.count().max(0) as usize);
    for index in 0..array.count() {
        // Surete : ImageIO renvoie un tableau de CFString.
        let pointeur = unsafe { array.value_at_index(index) };
        if pointeur.is_null() {
            continue;
        }
        let texte: &CFString = unsafe { &*(pointeur as *const CFString) };
        liste.push(texte.to_string());
    }
    liste
}

/// Vrai si ImageIO sait DECODER l'AVIF sur cette machine.
pub fn avif_lisible() -> bool {
    static CELLULE: OnceLock<bool> = OnceLock::new();
    *CELLULE.get_or_init(|| {
        let liste = unsafe { CGImageSource::type_identifiers() };
        annonce(&identifiants(&liste), IDENTIFIANT_AVIF)
    })
}

/// Vrai si ImageIO annonce et reussit effectivement un encodage AVIF sur cette machine.
///
/// L'annonce seule peut etre optimiste : un encodage miniature en fichier temporaire
/// verifie le chemin reel utilise par la conversion. Le resultat est memoise.
pub fn avif_encodable() -> bool {
    static CELLULE: OnceLock<bool> = OnceLock::new();
    *CELLULE.get_or_init(|| {
        let liste = unsafe { CGImageDestination::type_identifiers() };
        annonce(&identifiants(&liste), IDENTIFIANT_AVIF) && crate::avif::encodage_fonctionnel()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annonce_avif_insensible_a_la_casse() {
        assert!(annonce(&["public.avif".to_string()], IDENTIFIANT_AVIF));
        assert!(annonce(&["PUBLIC.AVIF".to_string()], IDENTIFIANT_AVIF));
        assert!(!annonce(
            &["public.heic".to_string(), "public.jpeg".to_string()],
            IDENTIFIANT_AVIF
        ));
        assert!(!annonce(&[], IDENTIFIANT_AVIF));
    }
}
