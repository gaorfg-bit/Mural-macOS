//! Application du fond d'ecran — equivalent de `mural/wallpaper_ops.py`, cote macOS.
//!
//! Sur macOS, chaque ecran recoit SA propre image : il n'y a pas de composite a
//! fabriquer comme sous GNOME. On utilise uniquement `NSWorkspace`.
//! Regle absolue : l'image d'origine n'est jamais modifiee, deplacee ni supprimee.

use std::fmt;
use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSScreen, NSWorkspace, NSWorkspaceDesktopImageOptionKey};
use objc2_foundation::{NSDictionary, NSError, NSString, NSURL};

/// Extensions historiques — reprises telles quelles de `Config._BASE_EXT`
/// (`mural/config.py`).
///
/// `.avif` s'y ajoute **uniquement** si ImageIO annonce son decodage
/// (`formats::avif_lisible`), sans hypothese sur la version de macOS.
pub const EXTENSIONS_ACCEPTEES: [&str; 6] = ["png", "jpg", "jpeg", "bmp", "webp", "svg"];

/// Erreurs possibles lors de l'application d'un fond d'ecran.
#[derive(Debug)]
pub enum ErreurFond {
    /// Le fichier image n'existe pas.
    FichierManquant(PathBuf),
    /// L'ecran demande n'existe plus.
    EcranIntrouvable(usize),
    /// L'extension du fichier n'est pas geree.
    ExtensionRefusee(String),
    /// L'appel Apple a renvoye une erreur.
    EchecApple(String),
}

impl fmt::Display for ErreurFond {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErreurFond::FichierManquant(p) => write!(f, "File not found: {}", p.display()),
            ErreurFond::EcranIntrouvable(i) => write!(f, "Display not found: index {i}"),
            ErreurFond::ExtensionRefusee(e) => write!(f, "Unsupported file extension: {e}"),
            ErreurFond::EchecApple(m) => write!(f, "macOS error: {m}"),
        }
    }
}

impl std::error::Error for ErreurFond {}

/// Verifie qu'un chemin porte une extension lisible (insensible a la casse).
pub fn extension_acceptee(chemin: &Path) -> Option<String> {
    extension_acceptee_avec(chemin, crate::formats::avif_lisible())
}

/// Variante pure : la capacite AVIF est passee en parametre.
///
/// Permet de verifier le refus quand le decodage manque sans toucher a
/// l'environnement global ni simuler ImageIO.
pub fn extension_acceptee_avec(chemin: &Path, avif_lisible: bool) -> Option<String> {
    let ext = chemin
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())?;
    let acceptee = EXTENSIONS_ACCEPTEES.contains(&ext.as_str()) || (ext == "avif" && avif_lisible);
    acceptee.then_some(ext)
}

/// Applique une image comme fond d'ecran d'un ecran donne.
///
/// `setDesktopImageURL:forScreen:options:error:` renvoie un `Result` dans les
/// liaisons objc2 : `Err` porte l'`NSError` d'Apple.
pub fn appliquer(image: &Path, ecran: &NSScreen) -> Result<(), ErreurFond> {
    if !image.is_file() {
        return Err(ErreurFond::FichierManquant(image.to_path_buf()));
    }
    if extension_acceptee(image).is_none() {
        let brut = image
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        return Err(ErreurFond::ExtensionRefusee(brut));
    }

    let chemin = NSString::from_str(&image.to_string_lossy());
    let url = NSURL::fileURLWithPath(&chemin);
    let options: Retained<NSDictionary<NSWorkspaceDesktopImageOptionKey, AnyObject>> =
        NSDictionary::from_slices::<NSWorkspaceDesktopImageOptionKey>(&[], &[]);
    let espace = NSWorkspace::sharedWorkspace();

    // Surete : `options` est bien un dictionnaire du type attendu par la methode.
    match unsafe { espace.setDesktopImageURL_forScreen_options_error(&url, ecran, &options) } {
        Ok(()) => Ok(()),
        Err(erreur) => Err(ErreurFond::EchecApple(decrire(&erreur))),
    }
}

/// Applique la meme image a plusieurs ecrans, en renvoyant les echecs.
pub fn appliquer_a_tous(image: &Path, ecrans: &[Retained<NSScreen>]) -> Vec<(usize, ErreurFond)> {
    ecrans
        .iter()
        .enumerate()
        .filter_map(|(i, ecran)| appliquer(image, ecran).err().map(|e| (i, e)))
        .collect()
}

/// Image de fond actuelle d'un ecran, en lecture seule.
///
/// L'API Apple ne renvoie pas d'erreur ici : elle renvoie `nil` s'il n'y a rien.
pub fn image_courante(ecran: &NSScreen) -> Option<PathBuf> {
    let espace = NSWorkspace::sharedWorkspace();
    let url = espace.desktopImageURLForScreen(ecran)?;
    let chemin = url.path()?;
    Some(PathBuf::from(chemin.to_string()))
}

/// Resume lisible d'une `NSError` Apple.
fn decrire(erreur: &NSError) -> String {
    let domaine = erreur.domain().to_string();
    format!("{} (code {})", domaine, erreur.code())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avif_refuse_quand_la_capacite_manque() {
        assert_eq!(
            extension_acceptee_avec(Path::new("photo.avif"), false),
            None,
            "sans decodeur AVIF annonce, l'extension doit rester refusee"
        );
        assert_eq!(
            extension_acceptee_avec(Path::new("photo.AVIF"), false),
            None
        );
    }

    #[test]
    fn avif_accepte_quand_la_capacite_existe_insensible_a_la_casse() {
        assert_eq!(
            extension_acceptee_avec(Path::new("photo.avif"), true),
            Some("avif".to_string())
        );
        assert_eq!(
            extension_acceptee_avec(Path::new("photo.AvIf"), true),
            Some("avif".to_string())
        );
    }

    #[test]
    fn formats_historiques_independants_de_la_capacite_avif() {
        assert_eq!(
            extension_acceptee_avec(Path::new("photo.PNG"), false),
            Some("png".to_string())
        );
        assert_eq!(extension_acceptee_avec(Path::new("note.txt"), true), None);
    }
}
