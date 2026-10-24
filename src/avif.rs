//! Conversion AVIF explicite via ImageIO (aucune dependance nouvelle).
//!
//! Le cache est **applicatif uniquement** (`~/Library/Caches/Mural/avif`,
//! surcharge `MURAL_AVIF_CACHE`) : jamais de dossier `.mural_cache` chez
//! l'utilisateur, jamais d'ecriture a cote de l'image source.
//!
//! Principes :
//! - une conversion = **une image decodee en pleine resolution** (aucune
//!   miniature pour la source), orientation EXIF appliquee aux pixels ;
//! - qualite AVIF **fixe et documentee** ([`QUALITE`]) ;
//! - identite stable (chemin absolu, date en nanosecondes, taille, qualite,
//!   version) et metadonnees completes relues avant reutilisation ;
//! - publication atomique par renommage d'un temporaire unique du meme dossier ;
//! - une conversion valide existante est relue (type `public.avif` et
//!   dimensions attendues) avant d'etre servie.
//!
//! Aucune purge ni eviction : le cache grandit, l'utilisateur decide.

use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_core_foundation::{CFDictionary, CFString, CFURL};
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use objc2_image_io::{
    kCGImageDestinationLossyCompressionQuality, kCGImagePropertyOrientation,
    kCGImagePropertyPixelHeight, kCGImagePropertyPixelWidth,
    kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceCreateThumbnailWithTransform,
    kCGImageSourceShouldCacheImmediately, kCGImageSourceThumbnailMaxPixelSize, CGImageDestination,
    CGImageSource,
};

use crate::formats::IDENTIFIANT_AVIF;

/// Version du pipeline de conversion : a incrementer si le decodage,
/// l'orientation ou les options d'encodage changent.
pub const VERSION_FORMAT: u32 = 2;

/// Qualite AVIF fixee (perte avec compression). Compromis taille/fidelite
/// de cette version : 0,80. Une conversion avec perte ne garantit pas une
/// copie visuellement identique ni un fichier plus petit pour toute source.
pub const QUALITE: f64 = 0.80;

/// Surcharge du dossier de cache (essais) : vide => conversion desactivee.
pub const VARIABLE_CACHE: &str = "MURAL_AVIF_CACHE";

const EXTENSION_AVIF: &str = "avif";
const EXTENSION_META: &str = "meta";

static COMPTEUR: AtomicU64 = AtomicU64::new(0);

/// Resultat d'une conversion (ou d'une reutilisation) AVIF.
#[derive(Debug, Clone)]
pub struct Conversion {
    /// Fichier AVIF dans le cache applicatif.
    pub chemin: PathBuf,
    /// Vrai si une conversion valide existait deja et a ete servie.
    pub reutilisee: bool,
    /// Taille de la source au moment de la conversion.
    pub octets_source: u64,
    /// Taille du fichier AVIF publie.
    pub octets_avif: u64,
}

/// Dossier de cache AVIF : `~/Library/Caches/Mural/avif` par defaut.
///
/// `None` si `MURAL_AVIF_CACHE` est defini mais vide, ou si `HOME` manque.
pub fn dossier_cache() -> Option<PathBuf> {
    dossier_cache_pour(std::env::var_os(VARIABLE_CACHE), std::env::var_os("HOME"))
}

/// Variante pure, testable sans toucher a l'environnement global.
fn dossier_cache_pour(
    surcharge: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if let Some(valeur) = surcharge {
        return if valeur.is_empty() {
            None
        } else {
            Some(PathBuf::from(valeur))
        };
    }
    let home = home?;
    Some(PathBuf::from(home).join("Library/Caches/Mural/avif"))
}

/// Vrai pour un fichier **reel** dont l'extension est png/jpg/jpeg/bmp/webp.
///
/// Les AVIF (deja convertis) et les SVG (vectoriels) ne sont pas candidats.
/// Cette fonction n'encode ni ne lit les pixels.
pub fn est_candidate(source: &Path) -> bool {
    let Some(extension) = source.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let convertible = matches!(
        extension.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "webp"
    );
    convertible && source.is_file()
}

/// Identite complete d'une conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Identite {
    /// Chemin absolu (canonique si possible) de la source.
    chemin: String,
    /// Date de modification en nanosecondes depuis l'epoque UNIX.
    mtime_ns: i128,
    /// Taille du fichier source en octets.
    taille: u64,
    /// Qualite d'encodage, en bits (stable et comparable).
    qualite_bits: u64,
    /// Version du format et du pipeline.
    version: u32,
}

impl Identite {
    /// Texte canonique a separation NUL (un chemin ne peut pas en contenir).
    fn texte(&self) -> String {
        format!(
            "mural-avif\0{}\0{}\0{}\0{}\0{}",
            self.chemin, self.mtime_ns, self.taille, self.qualite_bits, self.version
        )
    }
}

fn chemin_absolu(chemin: &Path) -> PathBuf {
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

fn mtime_ns(meta: &fs::Metadata) -> Option<i128> {
    let date = meta.modified().ok()?;
    match date.duration_since(UNIX_EPOCH) {
        Ok(duree) => Some(duree.as_nanos() as i128),
        Err(erreur) => Some(-(erreur.duration().as_nanos() as i128)),
    }
}

/// Identite de la source, ou une erreur lisible si elle est absente.
fn identite(source: &Path) -> Result<Identite, String> {
    let meta = fs::metadata(source)
        .map_err(|erreur| format!("Cannot read source image {}: {erreur}", source.display()))?;
    if !meta.is_file() {
        return Err(format!(
            "Cannot read source image {}: not a regular file",
            source.display()
        ));
    }
    Ok(Identite {
        chemin: chemin_absolu(source).to_string_lossy().into_owned(),
        mtime_ns: mtime_ns(&meta)
            .ok_or_else(|| format!("Cannot read modification date of {}", source.display()))?,
        taille: meta.len(),
        qualite_bits: QUALITE.to_bits(),
        version: VERSION_FORMAT,
    })
}

/// Empreinte 128 bits (deux FNV-1a 64) du texte d'identite.
fn empreinte(texte: &str) -> String {
    empreinte_octets(texte.as_bytes())
}

// Controle de coherence du contenu, pas une signature cryptographique.
fn empreinte_octets(octets: &[u8]) -> String {
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

fn cle(identite: &Identite) -> String {
    empreinte(&identite.texte())
}

fn fichier_avif(dossier: &Path, identite: &Identite) -> PathBuf {
    dossier.join(format!("{}.{EXTENSION_AVIF}", cle(identite)))
}

fn fichier_meta(dossier: &Path, identite: &Identite) -> PathBuf {
    dossier.join(format!("{}.{EXTENSION_META}", cle(identite)))
}

/// Temporaire unique du meme dossier (renommage atomique garanti).
fn temporaire(dossier: &Path, identite: &Identite, extension: &str) -> PathBuf {
    let numero = COMPTEUR.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duree| duree.subsec_nanos())
        .unwrap_or(0);
    dossier.join(format!(
        ".{}-{}-{numero}-{nanos}.{extension}.tmp",
        cle(identite),
        std::process::id()
    ))
}

fn avec_url<T>(chemin: &Path, action: impl FnOnce(&CFURL) -> T) -> T {
    let chemin_ns = NSString::from_str(&chemin.to_string_lossy());
    let url_ns = NSURL::fileURLWithPath(&chemin_ns);
    // NSURL et CFURL sont « toll-free bridged » : meme objet, deux vues.
    let url_cf: &CFURL = unsafe { &*(Retained::as_ptr(&url_ns) as *const CFURL) };
    action(url_cf)
}

fn ouvrir_source(chemin: &Path) -> Option<objc2_core_foundation::CFRetained<CGImageSource>> {
    avec_url(chemin, |url| unsafe { CGImageSource::with_url(url, None) })
}

/// Vrai objet Objective-C vu comme `AnyObject` (meme adresse, mise en page).
fn en_any<T: objc2::Message>(objet: &T) -> &AnyObject {
    unsafe { &*(objet as *const T as *const AnyObject) }
}

/// Enveloppe Objective-C des cles ImageIO (CFString et NSString bridged).
fn cle_ns(cle: &'static CFString) -> &'static NSString {
    unsafe { &*(cle as *const CFString as *const NSString) }
}

/// Lit une propriete entiere d'un dictionnaire ImageIO.
fn propriete_entiere(proprietes: &CFDictionary, cle: &CFString) -> Option<u32> {
    let brut = unsafe { proprietes.value(cle as *const CFString as *const c_void) };
    if brut.is_null() {
        return None;
    }
    let nombre: &NSNumber = unsafe { &*(brut as *const NSNumber) };
    Some(nombre.unsignedIntValue())
}

/// Geometrie brute de la source et orientation EXIF (1 = normale).
struct Geometrie {
    largeur: u32,
    hauteur: u32,
    orientation: u32,
}

impl Geometrie {
    /// Dimensions apres application de l'orientation EXIF.
    fn attendues(&self) -> (u32, u32) {
        // 5..=8 tournent de 90 degres : largeur et hauteur s'echangent.
        if (5..=8).contains(&self.orientation) {
            (self.hauteur, self.largeur)
        } else {
            (self.largeur, self.hauteur)
        }
    }
}

/// Lit largeur, hauteur et orientation dans l'en-tete (aucun pixel decode).
fn geometrie(source: &Path) -> Result<Geometrie, String> {
    let imageio = ouvrir_source(source)
        .ok_or_else(|| format!("Cannot read image header of {}", source.display()))?;
    // Surete : aucune option particuliere n'est demandee a ImageIO.
    let proprietes = unsafe { imageio.properties_at_index(0, None) }
        .ok_or_else(|| format!("Cannot read image properties of {}", source.display()))?;
    // Surete : les cles sont des `extern static` ImageIO, lues une fois.
    let cle_largeur = unsafe { kCGImagePropertyPixelWidth };
    let cle_hauteur = unsafe { kCGImagePropertyPixelHeight };
    let cle_orientation = unsafe { kCGImagePropertyOrientation };
    let largeur = propriete_entiere(&proprietes, cle_largeur)
        .filter(|v| *v > 0)
        .ok_or_else(|| format!("Cannot read pixel width of {}", source.display()))?;
    let hauteur = propriete_entiere(&proprietes, cle_hauteur)
        .filter(|v| *v > 0)
        .ok_or_else(|| format!("Cannot read pixel height of {}", source.display()))?;
    let orientation = propriete_entiere(&proprietes, cle_orientation)
        .filter(|v| (1..=8).contains(v))
        .unwrap_or(1);
    Ok(Geometrie {
        largeur,
        hauteur,
        orientation,
    })
}

/// Decode la source en **pleine resolution**, orientation EXIF appliquee.
///
/// Sans orientation particuliere, `CGImageSourceCreateImageAtIndex` est utilise
/// tel quel. Avec une orientation 2..8, la seule API ImageIO qui applique la
/// transformation est `CGImageSourceCreateThumbnailAtIndex` avec une taille
/// maximale egale au plus grand cote : les pixels restent donc **integralement**
/// conserves, seuls leurs axe et sens sont normalises.
fn decoder(
    source: &Path,
    geometrie: &Geometrie,
) -> Result<objc2_core_foundation::CFRetained<CGImage>, String> {
    let imageio =
        ouvrir_source(source).ok_or_else(|| format!("Cannot open image {}", source.display()))?;
    let image = if geometrie.orientation <= 1 {
        let vrai = NSNumber::new_bool(true);
        let cle = unsafe { cle_ns(kCGImageSourceShouldCacheImmediately) };
        let options = NSDictionary::from_slices(&[cle], &[en_any(&*vrai)]);
        let options_cf: &CFDictionary =
            unsafe { &*(Retained::as_ptr(&options) as *const CFDictionary) };
        unsafe { imageio.image_at_index(0, Some(options_cf)) }
    } else {
        let cote = geometrie.largeur.max(geometrie.hauteur) as u64;
        let vrai = NSNumber::new_bool(true);
        let taille = NSNumber::new_u64(cote);
        let cle_transforme = unsafe { cle_ns(kCGImageSourceCreateThumbnailWithTransform) };
        let cle_toujours = unsafe { cle_ns(kCGImageSourceCreateThumbnailFromImageAlways) };
        let cle_taille = unsafe { cle_ns(kCGImageSourceThumbnailMaxPixelSize) };
        let options = NSDictionary::from_slices(
            &[cle_transforme, cle_toujours, cle_taille],
            &[en_any(&*vrai), en_any(&*vrai), en_any(&*taille)],
        );
        let options_cf: &CFDictionary =
            unsafe { &*(Retained::as_ptr(&options) as *const CFDictionary) };
        unsafe { imageio.thumbnail_at_index(0, Some(options_cf)) }
    }
    .ok_or_else(|| format!("Cannot decode image {}", source.display()))?;

    let (largeur, hauteur) = geometrie.attendues();
    let decodee = (
        CGImage::width(Some(&image)) as u32,
        CGImage::height(Some(&image)) as u32,
    );
    if decodee != (largeur, hauteur) {
        return Err(format!(
            "Decoded image {} has unexpected dimensions {}x{} (expected {}x{})",
            source.display(),
            decodee.0,
            decodee.1,
            largeur,
            hauteur
        ));
    }
    Ok(image)
}

/// Vrai si le fichier est un AVIF lisible avec exactement les dimensions voulues.
///
/// C'est la verification de relecture qui autorise la reutilisation : la seule
/// presence du fichier ne suffit pas.
fn valide(chemin: &Path, attendues: (u32, u32)) -> bool {
    if !chemin.is_file() {
        return false;
    }
    let Some(imageio) = ouvrir_source(chemin) else {
        return false;
    };
    let type_annonce = unsafe { imageio.r#type() }
        .map(|texte| texte.to_string())
        .unwrap_or_default();
    if !type_annonce.eq_ignore_ascii_case(IDENTIFIANT_AVIF) {
        return false;
    }
    let Some(proprietes) = (unsafe { imageio.properties_at_index(0, None) }) else {
        return false;
    };
    let cle_largeur = unsafe { kCGImagePropertyPixelWidth };
    let cle_hauteur = unsafe { kCGImagePropertyPixelHeight };
    let Some(largeur) = propriete_entiere(&proprietes, cle_largeur) else {
        return false;
    };
    let Some(hauteur) = propriete_entiere(&proprietes, cle_hauteur) else {
        return false;
    };
    (largeur, hauteur) == attendues
}

/// Encode une image deja decodee en AVIF, dans un fichier temporaire.
fn encoder(image: &CGImage, chemin: &Path) -> Result<(), String> {
    let type_avif = CFString::from_str(IDENTIFIANT_AVIF);
    let qualite = NSNumber::new_f64(QUALITE);
    let cle_qualite = unsafe { cle_ns(kCGImageDestinationLossyCompressionQuality) };
    let options = NSDictionary::from_slices(&[cle_qualite], &[en_any(&*qualite)]);
    let options_cf: &CFDictionary =
        unsafe { &*(Retained::as_ptr(&options) as *const CFDictionary) };

    let ecrit = avec_url(chemin, |url| {
        let Some(destination) = (unsafe { CGImageDestination::with_url(url, &type_avif, 1, None) })
        else {
            return false;
        };
        unsafe { destination.add_image(image, Some(options_cf)) };
        unsafe { destination.finalize() }
    });
    if !ecrit {
        return Err(format!("Cannot encode AVIF file {}", chemin.display()));
    }
    Ok(())
}

/// Verifie une fois que le chemin ImageIO annonce fonctionne vraiment.
/// L'image et le fichier sont synthetiques et confines a un sous-dossier temporaire.
pub(crate) fn encodage_fonctionnel() -> bool {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let base =
        std::env::temp_dir().join(format!("mural-avif-probe-{}-{unique}", std::process::id()));
    if fs::create_dir(&base).is_err() {
        return false;
    }

    let source = base.join("sonde.png");
    let cible = base.join("sonde.avif");
    let synthetique = image::RgbImage::from_pixel(1, 1, image::Rgb([32, 192, 96]));
    let fonctionnel = synthetique.save(&source).is_ok()
        && geometrie(&source)
            .ok()
            .and_then(|g| decoder(&source, &g).ok())
            .is_some_and(|image| encoder(&image, &cible).is_ok());

    let _ = fs::remove_dir_all(&base);
    fonctionnel
}

/// Reutilise une conversion existante si l'identite **complete** et le fichier
/// AVIF relu (type + dimensions) correspondent.
///
/// Le `.meta` est lu avant l'AVIF : une empreinte qui collisionnerait ne peut
/// pas servir l'image d'une autre source.
fn reutiliser(cache: &Path, identite: &Identite, attendues: (u32, u32)) -> Option<Conversion> {
    let meta = fs::read_to_string(fichier_meta(cache, identite)).ok()?;
    let chemin = fichier_avif(cache, identite);
    let octets = fs::read(&chemin).ok()?;
    let attendue = format!("{}\0{}", identite.texte(), empreinte_octets(&octets));
    if meta != attendue {
        return None;
    }
    if !valide(&chemin, attendues) {
        return None;
    }
    let octets_avif = fs::metadata(&chemin).ok()?.len();
    Some(Conversion {
        chemin,
        reutilisee: true,
        octets_source: identite.taille,
        octets_avif,
    })
}

/// Convertit une source en AVIF dans le cache donne (une conversion bloquante).
///
/// L'appel est prevu pour un worker : aucune progression, aucune annulation
/// par fichier n'est geree ici. La source n'est jamais modifiee, deplacee ni
/// supprimee ; seule la copie AVIF du cache est ecrite.
pub fn convertir(source: &Path, cache: &Path) -> Result<Conversion, String> {
    let avant = identite(source)?;
    fs::create_dir_all(cache).map_err(|erreur| {
        format!(
            "Cannot create AVIF cache directory {}: {erreur}",
            cache.display()
        )
    })?;

    let geometrie = geometrie(source)?;
    let attendues = geometrie.attendues();

    // Cache chaud : metadonnees completes + AVIF relu (type et dimensions).
    if let Some(conversion) = reutiliser(cache, &avant, attendues) {
        return Ok(conversion);
    }

    if !crate::formats::avif_encodable() {
        return Err("AVIF encoding is not supported by ImageIO on this system".to_string());
    }

    let image = decoder(source, &geometrie)?;

    // Encodage dans un temporaire unique du dossier de cache.
    let temporaire_avif = temporaire(cache, &avant, EXTENSION_AVIF);
    if let Err(erreur) = encoder(&image, &temporaire_avif) {
        let _ = fs::remove_file(&temporaire_avif);
        return Err(erreur);
    }

    // Revalider la source : elle ne doit pas avoir change pendant la conversion.
    let apres = match identite(source) {
        Ok(apres) => apres,
        Err(erreur) => {
            let _ = fs::remove_file(&temporaire_avif);
            return Err(erreur);
        }
    };
    if apres != avant {
        let _ = fs::remove_file(&temporaire_avif);
        return Err(format!(
            "Source changed during AVIF conversion: {}",
            source.display()
        ));
    }

    // Le temporaire doit etre un AVIF lisible aux dimensions attendues avant
    // toute publication.
    if !valide(&temporaire_avif, attendues) {
        let _ = fs::remove_file(&temporaire_avif);
        return Err(format!(
            "Encoded AVIF is not readable with expected dimensions {}x{} for {}",
            attendues.0,
            attendues.1,
            source.display()
        ));
    }

    let octets_avif = match fs::metadata(&temporaire_avif) {
        Ok(meta) => meta.len(),
        Err(erreur) => {
            let _ = fs::remove_file(&temporaire_avif);
            return Err(format!(
                "Cannot read encoded AVIF {}: {erreur}",
                temporaire_avif.display()
            ));
        }
    };

    let contenu = match fs::read(&temporaire_avif) {
        Ok(contenu) => contenu,
        Err(erreur) => {
            let _ = fs::remove_file(&temporaire_avif);
            return Err(format!("Cannot read AVIF cache content: {erreur}"));
        }
    };
    let metadonnees = format!("{}\0{}", avant.texte(), empreinte_octets(&contenu));

    // Publication atomique : AVIF d'abord, metadonnees ensuite.
    let cible_avif = fichier_avif(cache, &avant);
    if let Err(erreur) = fs::rename(&temporaire_avif, &cible_avif) {
        let _ = fs::remove_file(&temporaire_avif);
        return Err(format!(
            "Cannot publish AVIF {}: {erreur}",
            cible_avif.display()
        ));
    }

    let temporaire_meta = temporaire(cache, &avant, EXTENSION_META);
    if let Err(erreur) = fs::write(&temporaire_meta, metadonnees.as_bytes()) {
        let _ = fs::remove_file(&temporaire_meta);
        return Err(format!(
            "Cannot write AVIF cache metadata {}: {erreur}",
            temporaire_meta.display()
        ));
    }
    if let Err(erreur) = fs::rename(&temporaire_meta, fichier_meta(cache, &avant)) {
        let _ = fs::remove_file(&temporaire_meta);
        return Err(format!(
            "Cannot publish AVIF cache metadata {}: {erreur}",
            fichier_meta(cache, &avant).display()
        ));
    }

    Ok(Conversion {
        chemin: cible_avif,
        reutilisee: false,
        octets_source: avant.taille,
        octets_avif,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn dossier_par_defaut_et_surcharge() {
        assert_eq!(
            dossier_cache_pour(None, Some(OsString::from("/Users/essai"))),
            Some(PathBuf::from("/Users/essai/Library/Caches/Mural/avif"))
        );
        assert_eq!(
            dossier_cache_pour(
                Some(OsString::from("/tmp/avif-essai")),
                Some(OsString::from("/Users/essai"))
            ),
            Some(PathBuf::from("/tmp/avif-essai"))
        );
        assert_eq!(
            dossier_cache_pour(Some(OsString::new()), Some(OsString::from("/Users/essai"))),
            None
        );
        assert_eq!(dossier_cache_pour(None, None), None);
    }

    #[test]
    fn identite_distincte_par_dossier_et_extension() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "mural-avif-identite-{}-{unique}",
            std::process::id()
        ));
        let a = base.join("a");
        let b = base.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let png_a = a.join("photo.png");
        let png_b = b.join("photo.png");
        let jpg_a = a.join("photo.jpg");
        for chemin in [&png_a, &png_b, &jpg_a] {
            fs::write(chemin, b"meme contenu").unwrap();
        }

        let cle_a = cle(&identite(&png_a).unwrap());
        let cle_b = cle(&identite(&png_b).unwrap());
        let cle_jpg = cle(&identite(&jpg_a).unwrap());
        assert_ne!(cle_a, cle_b, "dossiers differents");
        assert_ne!(cle_a, cle_jpg, "extensions differentes");
        assert_ne!(cle_b, cle_jpg);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn identite_change_avec_taille_ou_date() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "mural-avif-identite2-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&base).unwrap();
        let source = base.join("photo.png");
        fs::write(&source, b"a").unwrap();
        let avant = identite(&source).unwrap();

        fs::write(&source, b"ab").unwrap();
        let apres = identite(&source).unwrap();
        assert_ne!(avant.taille, apres.taille);
        assert_ne!(cle(&avant), cle(&apres));
        let _ = fs::remove_dir_all(&base);
    }
}
