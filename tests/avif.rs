//! Verification d'integration sur un vrai AVIF synthetique.
//!
//! L'encodeur de fixture n'existe qu'ici (code de test) : aucune image
//! utilisateur n'est lue ni reecrite. Si un encodage AVIF reel echoue sur
//! cette machine, le test le dit et s'arrete sans pretendre l'avoir verifie.

use std::fs;
use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2_core_foundation::{CFString, CFURL};
use objc2_foundation::{NSString, NSURL};
use objc2_image_io::{CGImageDestination, CGImageSource};

struct Gardien(PathBuf);

impl std::ops::Deref for Gardien {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Gardien {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn dossier_essai(nom: &str) -> Gardien {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base =
        std::env::temp_dir().join(format!("mural-avif-{nom}-{}-{unique}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    Gardien(base)
}

fn avec_url<T>(chemin: &Path, action: impl FnOnce(&CFURL) -> T) -> T {
    let texte = NSString::from_str(&chemin.to_string_lossy());
    let url = NSURL::fileURLWithPath(&texte);
    let url_cf: &CFURL = unsafe { &*(Retained::as_ptr(&url) as *const CFURL) };
    action(url_cf)
}

/// Encode un PNG synthetique puis le convertit en AVIF avec ImageIO.
/// Renvoie `false` si l'encodeur AVIF n'est pas disponible.
fn ecrire_avif(chemin: &Path, largeur: u32, hauteur: u32) -> bool {
    let source_png = chemin.with_extension("fixture.png");
    let image = image::RgbImage::from_fn(largeur, hauteur, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 64])
    });
    if image.save(&source_png).is_err() {
        return false;
    }

    let encode = avec_url(&source_png, |url_source| {
        let Some(source) = (unsafe { CGImageSource::with_url(url_source, None) }) else {
            return false;
        };
        let Some(bitmap) = (unsafe { source.image_at_index(0, None) }) else {
            return false;
        };
        avec_url(chemin, |url_avif| {
            let type_avif = CFString::from_str("public.avif");
            let Some(destination) =
                (unsafe { CGImageDestination::with_url(url_avif, &type_avif, 1, None) })
            else {
                return false;
            };
            unsafe { destination.add_image(&bitmap, None) };
            unsafe { destination.finalize() }
        })
    });

    let _ = fs::remove_file(&source_png);
    encode
}

#[test]
fn avif_synthetique_traverse_dimensions_galerie_vignette_apercu_et_cache() {
    let base = dossier_essai("integration");
    let source = base.join("photo.AVIF");
    if !mural::formats::avif_encodable() {
        eprintln!("TEST NON EXECUTE : aucun encodage AVIF fonctionnel detecte sur cette machine");
        return;
    }
    assert!(
        ecrire_avif(&source, 720, 440),
        "la sonde AVIF a reussi mais la fixture synthetique n'a pas pu etre ecrite"
    );
    assert!(
        mural::formats::avif_lisible(),
        "un encodeur AVIF annonce sans decodeur ne permet pas ce test"
    );

    let original = fs::read(&source).unwrap();

    // Dimensions : l'en-tete ImageIO remplace l'ignorance de la crate `image`.
    assert_eq!(
        mural::bibliotheque::dimensions(&source),
        Some((720, 440)),
        "dimensions AVIF attendues 720x440"
    );

    // Decouverte de la galerie, insensible a la casse de l'extension.
    let images = mural::bibliotheque::lister(&base, mural::bibliotheque::Tri::Nom).unwrap();
    assert_eq!(images.len(), 1, "l'AVIF doit apparaitre dans la galerie");
    assert_eq!(images[0].nom, "photo.AVIF");

    // Vignette froide puis chaude, cache isole sous le dossier d'essai.
    let cache = base.join("cache");
    let froide = mural::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache))
        .expect("vignette AVIF froide");
    assert_eq!((froide.largeur, froide.hauteur), (360, 220));

    let ecrits: Vec<PathBuf> = fs::read_dir(&cache)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    assert_eq!(ecrits.len(), 2, "PNG + identite attendus, vu {ecrits:?}");

    // Remplace le PNG du cache par un marqueur : une lecture chaude doit le
    // servir sans redecoder la source AVIF.
    let marqueur = image::RgbImage::new(64, 40);
    let png_cache = ecrits
        .iter()
        .find(|p| p.extension().is_some_and(|e| e == "png"))
        .expect("PNG de cache");
    marqueur.save(png_cache).unwrap();
    let chaude = mural::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache))
        .expect("vignette AVIF chaude");
    assert_eq!(
        (chaude.largeur, chaude.hauteur),
        (64, 40),
        "la vignette chaude doit venir du cache"
    );

    // Apercu panoramique, jamais agrandi au-dela de l'original.
    let apercu = mural::bibliotheque::apercu(&source, 1600, 780).expect("apercu AVIF");
    assert_eq!((apercu.largeur, apercu.hauteur), (720, 440));

    // L'original n'a jamais ete touche.
    assert_eq!(fs::read(&source).unwrap(), original);
}

#[test]
fn avif_corrompu_renvoie_none_sans_paniquer() {
    let base = dossier_essai("corrompu");
    let source = base.join("casse.avif");
    fs::write(&source, b"pas un avif du tout").unwrap();

    assert!(mural::bibliotheque::dimensions(&source).is_none());
    assert!(mural::bibliotheque::vignette_avec_cache(&source, 180, Some(&base.join("cache"))).is_none());
    assert!(mural::bibliotheque::apercu(&source, 1600, 780).is_none());
}

#[test]
fn capacite_annoncee_correspond_a_un_encodage_reel() {
    let base = dossier_essai("capacite");
    let sortie = base.join("sonde.avif");
    let encodage_reussi = ecrire_avif(&sortie, 1, 1);

    assert_eq!(
        mural::formats::avif_encodable(),
        encodage_reussi,
        "la capacite annoncee doit correspondre a la possibilite d'encoder"
    );
}
