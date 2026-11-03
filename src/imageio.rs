//! Aides ImageIO / Foundation partagees (cache, conversion AVIF, bibliotheque).
//!
//! Un seul endroit pour les ponts « toll-free bridged » `NSURL`/`CFURL` et
//! `NSString`/`CFString`, les identites de fichier et les ecritures atomiques.

use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_core_foundation::{CFDictionary, CFRetained, CFString, CFURL};
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSNumber, NSString, NSURL};
use objc2_image_io::CGImageSource;

/// `Config.THUMBNAIL_ASPECT` : hauteur = largeur x 0,611.
pub const ASPECT_VIGNETTE: f64 = 220.0 / 360.0;

/// Image decodee par ImageIO, prete a etre presentee par l'interface.
#[derive(Debug)]
pub struct Vignette {
    /// Image Core Graphics (a envelopper dans une NSImage cote interface).
    pub image: CFRetained<CGImage>,
    pub largeur: usize,
    pub hauteur: usize,
}

static COMPTEUR: AtomicU64 = AtomicU64::new(0);

pub(crate) fn chemin_absolu(chemin: &Path) -> PathBuf {
    fs::canonicalize(chemin).unwrap_or_else(|_| {
        if chemin.is_absolute() {
            chemin.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|dossier| dossier.join(chemin))
                .unwrap_or_else(|_| chemin.to_path_buf())
        }
    })
}

pub(crate) fn mtime_ns(meta: &fs::Metadata) -> Option<i128> {
    let date = meta.modified().ok()?;
    match date.duration_since(UNIX_EPOCH) {
        Ok(duree) => Some(duree.as_nanos() as i128),
        Err(erreur) => Some(-(erreur.duration().as_nanos() as i128)),
    }
}

/// Empreinte 128 bits (deux FNV-1a 64). Controle de coherence, pas une
/// signature cryptographique.
pub(crate) fn empreinte_octets(octets: &[u8]) -> String {
    let mut premier = 0xcbf2_9ce4_8422_2325u64;
    let mut second = 0x8422_2325_cbf2_9ce4u64;
    for octet in octets.iter().copied() {
        premier ^= octet as u64;
        premier = premier.wrapping_mul(0x100_0000_01b3);
        second = second.rotate_left(7) ^ octet as u64;
        second = second.wrapping_mul(0x100_0000_01b3);
    }
    format!("{premier:016x}{second:016x}")
}

pub(crate) fn empreinte(texte: &str) -> String {
    empreinte_octets(texte.as_bytes())
}

/// Nom de fichier temporaire unique dans `dossier` (meme volume : le renommage
/// vers la cible est atomique).
pub(crate) fn temporaire(dossier: &Path, cle: &str, extension: &str) -> PathBuf {
    let numero = COMPTEUR.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duree| duree.subsec_nanos())
        .unwrap_or(0);
    dossier.join(format!(
        ".{cle}-{}-{numero}-{nanos}.{extension}.tmp",
        std::process::id()
    ))
}

pub(crate) fn avec_url<T>(chemin: &Path, action: impl FnOnce(&CFURL) -> T) -> T {
    let chemin_ns = NSString::from_str(&chemin.to_string_lossy());
    let url_ns = NSURL::fileURLWithPath(&chemin_ns);
    // NSURL et CFURL sont « toll-free bridged » : meme objet, deux vues.
    let url_cf: &CFURL = unsafe { &*(Retained::as_ptr(&url_ns) as *const CFURL) };
    action(url_cf)
}

/// Ouvre une source ImageIO a partir d'un chemin.
pub(crate) fn ouvrir_source(chemin: &Path) -> Option<CFRetained<CGImageSource>> {
    avec_url(chemin, |url| unsafe { CGImageSource::with_url(url, None) })
}

/// Vrai objet Objective-C vu comme `AnyObject` (meme adresse, meme mise en page).
pub(crate) fn en_any<T: objc2::Message>(objet: &T) -> &AnyObject {
    unsafe { &*(objet as *const T as *const AnyObject) }
}

/// Enveloppe Objective-C des cles ImageIO (CFString et NSString sont bridged).
pub(crate) fn cle_ns(cle: &'static CFString) -> &'static NSString {
    unsafe { &*(cle as *const CFString as *const NSString) }
}

/// Lit une propriete entiere d'un dictionnaire ImageIO.
///
/// ImageIO stocke ces valeurs en CFNumber, « toll-free bridged » avec `NSNumber`.
pub(crate) fn propriete_entiere(proprietes: &CFDictionary, cle: &CFString) -> Option<u32> {
    // Surete : la cle est un CFString ImageIO, la valeur un CFNumber.
    let brut = unsafe { proprietes.value(cle as *const CFString as *const c_void) };
    if brut.is_null() {
        return None;
    }
    let nombre: &NSNumber = unsafe { &*(brut as *const NSNumber) };
    Some(nombre.unsignedIntValue())
}
