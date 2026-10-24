//! Bibliotheque d'images — equivalent de `mural/thumbnails.py` et du
//! `_gallery_worker` de `mural/app.py`.
//!
//! Les vignettes et les apercus passent par **ImageIO**
//! (`CGImageSourceCreateThumbnailAtIndex`) : le decodeur d'Apple travaille
//! directement a l'echelle demandee et respecte l'orientation EXIF. Mesure du
//! 22/10 sur une image 8176x4096 : 766 ms en decodage plein puis reduction
//! (crate `image`), contre quelques dizaines de ms avec ImageIO.
//!
//! Les fichiers JSON/texte ne sont jamais touches ; les images d'origine ne sont
//! ni modifiees, ni deplacees, ni supprimees.

use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_core_foundation::{CFDictionary, CFString, CFURL};
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use objc2_image_io::{
    CGImageSource, kCGImagePropertyPixelHeight, kCGImagePropertyPixelWidth,
    kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceCreateThumbnailWithTransform,
    kCGImageSourceThumbnailMaxPixelSize,
};

/// Plafond de `Config.MAX_IMAGES` (`mural/config.py`).
pub const LIMITE_DOSSIER: usize = 500;

/// `Config.THUMBNAIL_ASPECT` : hauteur = largeur x 0,611.
pub const ASPECT_VIGNETTE: f64 = 220.0 / 360.0;

/// Une image trouvee dans le dossier courant.
#[derive(Debug, Clone)]
pub struct Image {
    pub chemin: PathBuf,
    pub nom: String,
    pub date: SystemTime,
    pub taille: u64,
}

/// Ordre de tri, comme `_sort_mode` de `mural/app.py`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tri {
    Nom,
    Date,
}

/// Image decodee par ImageIO, prete a etre presentee par l'interface.
#[derive(Debug)]
pub struct Vignette {
    /// Image Core Graphics (a envelopper dans une NSImage cote interface).
    pub image: objc2_core_foundation::CFRetained<CGImage>,
    pub largeur: usize,
    pub hauteur: usize,
}

/// Liste les images du PREMIER NIVEAU du dossier.
///
/// Comme `folder.iterdir()` en Python : aucune recursion dans les sous-dossiers.
pub fn lister(dossier: &Path, tri: Tri) -> std::io::Result<Vec<Image>> {
    let mut images: Vec<Image> = Vec::new();
    if !dossier.is_dir() {
        return Ok(images);
    }

    for entree in fs::read_dir(dossier)? {
        let Ok(entree) = entree else { continue };
        let chemin = entree.path();
        let Ok(meta) = entree.metadata() else {
            continue;
        };
        if !meta.is_file() || super::fond_ecran::extension_acceptee(&chemin).is_none() {
            continue;
        }
        let nom = chemin
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        images.push(Image {
            chemin,
            nom,
            date: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            taille: meta.len(),
        });
    }

    match tri {
        Tri::Nom => images.sort_by(|a, b| a.nom.to_lowercase().cmp(&b.nom.to_lowercase())),
        Tri::Date => images.sort_by(|a, b| b.date.cmp(&a.date)),
    }
    images.truncate(LIMITE_DOSSIER);
    Ok(images)
}

/// Dimensions en pixels, lues dans l'en-tete du fichier (aucun decodage).
///
/// Les formats courants passent par la crate `image` (0 ms). Les formats
/// qu'elle ignore — notamment AVIF — retombent sur les proprietes ImageIO
/// `PixelWidth`/`PixelHeight`, sans decoder la pleine resolution.
pub fn dimensions(chemin: &Path) -> Option<(u32, u32)> {
    if let Ok(dimensions) = image::image_dimensions(chemin) {
        return Some(dimensions);
    }
    dimensions_imageio(chemin)
}

/// Dimensions lues par ImageIO dans l'en-tete, sans decodage des pixels.
fn dimensions_imageio(chemin: &Path) -> Option<(u32, u32)> {
    let source = source(chemin)?;
    // Surete : aucune option particuliere n'est demandee a ImageIO.
    let proprietes = unsafe { source.properties_at_index(0, None) }?;
    // Surete : les cles sont des `extern static` ImageIO, lues une fois.
    let cle_largeur = unsafe { kCGImagePropertyPixelWidth };
    let cle_hauteur = unsafe { kCGImagePropertyPixelHeight };
    let largeur = propriete_entiere(&proprietes, cle_largeur)?;
    let hauteur = propriete_entiere(&proprietes, cle_hauteur)?;
    Some((largeur, hauteur))
}

/// Lit une propriete entiere d'un dictionnaire ImageIO.
///
/// ImageIO stocke ces valeurs en CFNumber, « toll-free bridged » avec
/// `NSNumber`.
fn propriete_entiere(proprietes: &CFDictionary, cle: &CFString) -> Option<u32> {
    // Surete : la cle est un CFString ImageIO, la valeur un CFNumber.
    let brut = unsafe { proprietes.value(cle as *const CFString as *const c_void) };
    if brut.is_null() {
        return None;
    }
    let nombre: &NSNumber = unsafe { &*(brut as *const NSNumber) };
    Some(nombre.unsignedIntValue())
}

/// Ouvre une source ImageIO a partir d'un chemin.
fn source(chemin: &Path) -> Option<objc2_core_foundation::CFRetained<CGImageSource>> {
    let chemin_ns = NSString::from_str(&chemin.to_string_lossy());
    let url_ns = NSURL::fileURLWithPath(&chemin_ns);
    // NSURL et CFURL sont « toll-free bridged » : meme objet, deux vues.
    let url_cf: &CFURL = unsafe { &*(Retained::as_ptr(&url_ns) as *const CFURL) };
    unsafe { CGImageSource::with_url(url_cf, None) }
}

/// Vrai objet Objective-C vu comme `AnyObject` (meme adresse, meme mise en page).
fn en_any<T: objc2::Message>(objet: &T) -> &AnyObject {
    unsafe { &*(objet as *const T as *const AnyObject) }
}

/// Enveloppe Objective-C des cles ImageIO (CFString et NSString sont bridged).
fn cle_ns(cle: &'static CFString) -> &'static NSString {
    unsafe { &*(cle as *const CFString as *const NSString) }
}

/// Options demandant a ImageIO une vignette a l'echelle, orientation corrigee.
fn options_vignette(cote_max: usize) -> Retained<NSDictionary<NSString, AnyObject>> {
    let vrai = NSNumber::new_bool(true);
    let taille = NSNumber::new_u64(cote_max as u64);
    let cle_transforme = unsafe { cle_ns(kCGImageSourceCreateThumbnailWithTransform) };
    let cle_toujours = unsafe { cle_ns(kCGImageSourceCreateThumbnailFromImageAlways) };
    let cle_taille = unsafe { cle_ns(kCGImageSourceThumbnailMaxPixelSize) };

    NSDictionary::from_slices(
        &[cle_transforme, cle_toujours, cle_taille],
        &[en_any(&*vrai), en_any(&*vrai), en_any(&*taille)],
    )
}

/// Produit une image a l'echelle demandee, sans jamais decoder la pleine resolution.
fn image_reduite(chemin: &Path, cote_max: usize) -> Option<Vignette> {
    let source = source(chemin)?;
    let options = options_vignette(cote_max);
    let options_cf: &CFDictionary =
        unsafe { &*(Retained::as_ptr(&options) as *const CFDictionary) };
    // Surete : `options` contient exactement les cles attendues par ImageIO.
    let image = unsafe { source.thumbnail_at_index(0, Some(options_cf)) }?;
    let largeur = CGImage::width(Some(&image));
    let hauteur = CGImage::height(Some(&image));
    Some(Vignette {
        image,
        largeur,
        hauteur,
    })
}

/// Vignette « cover » de la largeur demandee, hauteur selon `ASPECT_VIGNETTE`.
///
/// Passe d'abord par le cache disque (`~/Library/Caches/Mural/vignettes`,
/// surcharge `MURAL_CACHE`). Le cache est optionnel : toute erreur est ignoree
/// et le decodage ImageIO d'origine reste la reference.
pub fn vignette(chemin: &Path, largeur_cible: usize) -> Option<Vignette> {
    let dossier = crate::cache::dossier_defaut();
    vignette_avec_cache(chemin, largeur_cible, dossier.as_deref())
}

/// Variante a dossier de cache explicite (tests et outils de mesure).
///
/// `None` desactive le cache. Le decodage, l'orientation, le recadrage cover et
/// la nettete Retina sont identiques dans tous les cas.
pub fn vignette_avec_cache(
    chemin: &Path,
    largeur_cible: usize,
    dossier_cache: Option<&Path>,
) -> Option<Vignette> {
    let largeur_cible = largeur_cible.max(1);
    // Identite AVANT decodage : elle est revalidee apres pour ne jamais
    // publier une vignette si la source a change entre-temps.
    let identite_avant = dossier_cache.and_then(|_| crate::cache::identite(chemin, largeur_cible));
    if let (Some(dossier), Some(identite)) = (dossier_cache, identite_avant.as_ref()) {
        if let Some(vignette) = crate::cache::lire(dossier, identite) {
            return Some(vignette);
        }
    }

    let vignette = vignette_sans_cache(chemin, largeur_cible)?;
    if let (Some(dossier), Some(identite)) = (dossier_cache, identite_avant) {
        if crate::cache::identite(chemin, largeur_cible).as_ref() == Some(&identite) {
            crate::cache::ecrire(dossier, &identite, &vignette.image);
        }
    }
    Some(vignette)
}

/// Decodage d'origine, sans cache.
pub fn vignette_sans_cache(chemin: &Path, largeur_cible: usize) -> Option<Vignette> {
    let largeur_cible = largeur_cible.max(1);
    let hauteur_cible = ((largeur_cible as f64) * ASPECT_VIGNETTE).round().max(1.0) as usize;
    // On demande deux fois le cote utile : net sur un ecran Retina.
    let brute = image_reduite(chemin, (largeur_cible * 2).max(hauteur_cible * 2))?;
    // Recadrage « cover » centre, comme ImageOps.fit de Pillow.
    // ASPECT_VIGNETTE est un rapport hauteur/largeur (110/180) ; le recadrage
    // attend un rapport largeur/hauteur, d'ou l'inverse.
    let recadree = recadrer_cover(
        &brute.image,
        brute.largeur,
        brute.hauteur,
        1.0 / ASPECT_VIGNETTE,
    )?;
    let largeur = CGImage::width(Some(&recadree));
    let hauteur = CGImage::height(Some(&recadree));
    Some(Vignette {
        image: recadree,
        largeur,
        hauteur,
    })
}

/// Recadre une image au rapport hauteur/largeur demande, centree.
///
/// Une seule fonction Apple : `CGImageCreateWithImageInRect`. Aucun contexte
/// bitmap n'est necessaire.
fn recadrer_cover(
    image: &CGImage,
    largeur: usize,
    hauteur: usize,
    aspect: f64,
) -> Option<objc2_core_foundation::CFRetained<CGImage>> {
    if largeur == 0 || hauteur == 0 {
        return None;
    }
    let ratio_source = largeur as f64 / hauteur as f64;
    let (largeur_cadre, hauteur_cadre) = if ratio_source > aspect {
        ((hauteur as f64) * aspect, hauteur as f64)
    } else {
        (largeur as f64, (largeur as f64) / aspect)
    };
    let largeur_cadre = largeur_cadre.min(largeur as f64).max(1.0);
    let hauteur_cadre = hauteur_cadre.min(hauteur as f64).max(1.0);
    let x = ((largeur as f64) - largeur_cadre) / 2.0;
    let y = ((hauteur as f64) - hauteur_cadre) / 2.0;
    let cadre = objc2_core_foundation::CGRect {
        origin: objc2_core_foundation::CGPoint { x, y },
        size: objc2_core_foundation::CGSize {
            width: largeur_cadre,
            height: hauteur_cadre,
        },
    };
    CGImage::with_image_in_rect(Some(image), cadre)
}

/// Apercu panoramique, jamais agrandi au dela de l'original.///
/// Equivalent de `ImageLoader.load_for_preview`.
pub fn apercu(chemin: &Path, largeur_max: u32, hauteur_max: u32) -> Option<Vignette> {
    let cote = largeur_max.max(hauteur_max) as usize;
    let (l, h) = dimensions(chemin)?;
    let cote = if (l as usize) < cote && (h as usize) < cote {
        (l.max(h)) as usize
    } else {
        cote
    };
    image_reduite(chemin, cote)
}

/// Taille binaire lisible, sans lire ni decoder le contenu du fichier.
pub fn taille_lisible(octets: Option<u64>) -> String {
    let Some(octets) = octets else { return "Size unavailable".into(); };
    if octets < 1024 { return format!("{octets} B"); }
    let unites = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let mut valeur = octets as f64;
    let mut unite = 0;
    while valeur >= 1024.0 && unite + 1 < unites.len() {
        valeur /= 1024.0;
        unite += 1;
    }
    format!("{valeur:.1} {}", unites[unite])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn taille_lisible_aux_frontieres_et_indisponible() {
        for (octets, texte) in [(Some(0), "0 B"), (Some(1023), "1023 B"),
            (Some(1024), "1.0 KiB"), (Some(2350), "2.3 KiB"),
            (Some(1_048_576), "1.0 MiB"), (Some(1_073_741_824), "1.0 GiB"),
            (None, "Size unavailable")] {
            assert_eq!(taille_lisible(octets), texte);
        }
    }

    fn dossier_essai(nom: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("mural-test-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("sous-dossier")).unwrap();
        fs::write(base.join("b.png"), b"x").unwrap();
        fs::write(base.join("a.JPG"), b"x").unwrap();
        fs::write(base.join("note.txt"), b"x").unwrap();
        fs::write(base.join("sous-dossier/c.png"), b"x").unwrap();
        base
    }

    #[test]
    fn extension_acceptee_insensible_a_la_casse() {
        assert!(crate::fond_ecran::extension_acceptee(Path::new("photo.PNG")).is_some());
        assert!(crate::fond_ecran::extension_acceptee(Path::new("photo.jpeg")).is_some());
        assert!(crate::fond_ecran::extension_acceptee(Path::new("note.txt")).is_none());
        assert!(crate::fond_ecran::extension_acceptee(Path::new("sans_extension")).is_none());
    }

    #[test]
    fn lister_sans_recursion_et_tri_par_nom() {
        let dossier = dossier_essai("liste");
        let images = lister(&dossier, Tri::Nom).unwrap();
        let noms: Vec<&str> = images.iter().map(|i| i.nom.as_str()).collect();
        assert_eq!(noms, vec!["a.JPG", "b.png"]);
        assert!(images.iter().all(|i| i.chemin.is_file()));
        let _ = fs::remove_dir_all(&dossier);
    }

    #[test]
    fn lister_tri_par_date_decroissante() {
        let dossier = dossier_essai("dates");
        let images = lister(&dossier, Tri::Date).unwrap();
        assert_eq!(images.len(), 2);
        assert!(images[0].date >= images[1].date);
        let _ = fs::remove_dir_all(&dossier);
    }

    #[test]
    fn lister_dossier_inexistant_ne_plante_pas() {
        let images = lister(Path::new("/dossier/qui/nexiste/pas"), Tri::Nom).unwrap();
        assert!(images.is_empty());
    }

    #[test]
    fn dimensions_des_formats_courants_inchangees() {
        let chemin =
            std::env::temp_dir().join(format!("mural-dimensions-{}.png", std::process::id()));
        image::RgbImage::new(37, 23).save(&chemin).unwrap();
        let mesurees = dimensions(&chemin);
        let _ = fs::remove_file(&chemin);
        assert_eq!(mesurees, Some((37, 23)));
    }
}

#[cfg(test)]
mod regression_recadrage {
    #[test]
    fn portrait_garde_le_rapport_paysage() {
        let image = image::RgbImage::new(108, 192);
        let chemin =
            std::env::temp_dir().join(format!("mural-portrait-{}.png", std::process::id()));
        image.save(&chemin).unwrap();
        let vignette = super::vignette_avec_cache(&chemin, 180, None).unwrap();
        std::fs::remove_file(chemin).unwrap();
        let rapport = vignette.largeur as f64 / vignette.hauteur as f64;
        assert!(
            (rapport - 1.0 / super::ASPECT_VIGNETTE).abs() < 0.05,
            "rapport {rapport}"
        );
    }
}
