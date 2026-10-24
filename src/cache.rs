//! Cache disque des vignettes.
//!
//! Le cache est **optionnel** : toute erreur (dossier non ecrivable, PNG
//! corrompu, identite mensongere) retombe silencieusement sur le decodage
//! ImageIO d'origine. Aucun apercu n'est mis en cache.
//!
//! Format : un PNG natif encode par ImageIO (donc sans perte, transparence
//! conservee) accompagne d'un fichier `.ident` qui contient l'identite complete
//! de la source. Le nom de fichier est une empreinte 128 bits de cette meme
//! identite ; le `.ident` sert a valider l'identite **complete** et evite
//! qu'une collision d'empreinte ne montre une autre image.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use objc2::rc::Retained;
use objc2_core_foundation::{CFDictionary, CFString, CFURL};
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use objc2_image_io::{CGImageDestination, CGImageSource, kCGImageSourceShouldCacheImmediately};

use crate::bibliotheque::{ASPECT_VIGNETTE, Vignette};

/// Version du format de cache : a incrementer des que le decodage, le
/// recadrage ou le rapport change.
pub const VERSION_FORMAT: u32 = 1;

/// Surcharge du dossier de cache (essais), sinon `~/Library/Caches/Mural/vignettes`.
pub const VARIABLE_DOSSIER: &str = "MURAL_CACHE";

static COMPTEUR: AtomicU64 = AtomicU64::new(0);

/// Dossier de cache par defaut. `None` desactive le cache (pas de HOME).
pub fn dossier_defaut() -> Option<PathBuf> {
    dossier_pour(std::env::var_os(VARIABLE_DOSSIER), std::env::var_os("HOME"))
}

/// Variante pure, testable sans toucher a l'environnement global.
fn dossier_pour(
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
    Some(PathBuf::from(home).join("Library/Caches/Mural/vignettes"))
}

/// Identite complete d'une vignette en cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identite {
    /// Chemin absolu (resolu) de l'image source.
    pub chemin: String,
    /// Date de modification en nanosecondes depuis l'epoque UNIX.
    pub mtime_ns: i128,
    /// Taille du fichier source en octets.
    pub taille: u64,
    /// Largeur demandee, deja normalisee (`max(1)`).
    pub largeur: usize,
    /// Version du format et du pipeline de transformation.
    pub version: u32,
    /// Bits du rapport hauteur/largeur applique (`ASPECT_VIGNETTE`).
    pub aspect_bits: u64,
}

impl Identite {
    /// Texte canonique, a separation NUL (un chemin ne peut pas en contenir).
    pub fn texte(&self) -> String {
        format!(
            "mural-vignette\0{}\0{}\0{}\0{}\0{}\0{}",
            self.chemin, self.mtime_ns, self.taille, self.largeur, self.version, self.aspect_bits
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

/// Identite d'une source, ou `None` si la source est absente/illisible.
pub fn identite(chemin: &Path, largeur_cible: usize) -> Option<Identite> {
    let meta = fs::metadata(chemin).ok()?;
    if !meta.is_file() {
        return None;
    }
    Some(Identite {
        chemin: chemin_absolu(chemin).to_string_lossy().into_owned(),
        mtime_ns: mtime_ns(&meta)?,
        taille: meta.len(),
        largeur: largeur_cible.max(1),
        version: VERSION_FORMAT,
        aspect_bits: ASPECT_VIGNETTE.to_bits(),
    })
}

/// Empreinte 128 bits (deux FNV-1a 64) de l'identite complete.
fn empreinte(texte: &str) -> String {
    let mut premier = 0xcbf2_9ce4_8422_2325u64;
    let mut second = 0x8422_2325_cbf2_9ce4u64;
    for octet in texte.bytes() {
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

fn fichier_png(dossier: &Path, identite: &Identite) -> PathBuf {
    dossier.join(format!("{}.png", cle(identite)))
}

fn fichier_identite(dossier: &Path, identite: &Identite) -> PathBuf {
    dossier.join(format!("{}.ident", cle(identite)))
}

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

fn encoder_png(image: &CGImage, chemin: &Path) -> bool {
    let type_png = CFString::from_str("public.png");
    avec_url(chemin, |url| {
        let Some(destination) = (unsafe { CGImageDestination::with_url(url, &type_png, 1, None) })
        else {
            return false;
        };
        unsafe { destination.add_image(image, None) };
        unsafe { destination.finalize() }
    })
}

fn decoder_png(chemin: &Path) -> Option<Vignette> {
    let image = avec_url(chemin, |url| {
        let source = unsafe { CGImageSource::with_url(url, None) }?;
        // Decoder les pixels sur le worker, sans reporter ce travail au dessin AppKit.
        let vrai = NSNumber::new_bool(true);
        let cle: &NSString = unsafe {
            &*(kCGImageSourceShouldCacheImmediately as *const CFString as *const NSString)
        };
        let options = NSDictionary::from_slices(&[cle], &[&*vrai]);
        let options_cf: &CFDictionary =
            unsafe { &*(Retained::as_ptr(&options) as *const CFDictionary) };
        unsafe { source.image_at_index(0, Some(options_cf)) }
    })?;
    let largeur = CGImage::width(Some(&image));
    let hauteur = CGImage::height(Some(&image));
    if largeur == 0 || hauteur == 0 {
        return None;
    }
    Some(Vignette {
        image,
        largeur,
        hauteur,
    })
}

/// Lit une vignette du cache si l'identite complete correspond.
pub fn lire(dossier: &Path, identite: &Identite) -> Option<Vignette> {
    // Le `.ident` est lu AVANT le PNG : il porte l'identite complete, donc une
    // empreinte qui collisionnerait ne peut pas montrer une autre image.
    let meta = fs::read_to_string(fichier_identite(dossier, identite)).ok()?;
    if meta != identite.texte() {
        return None;
    }
    decoder_png(&fichier_png(dossier, identite))
}

/// Publie une vignette dans le cache. Toute erreur est ignoree.
pub fn ecrire(dossier: &Path, identite: &Identite, image: &CGImage) {
    if fs::create_dir_all(dossier).is_err() {
        return;
    }

    // 1. PNG dans un temporaire unique du meme dossier, puis renommage atomique.
    let temporaire_png = temporaire(dossier, identite, "png");
    if !encoder_png(image, &temporaire_png) {
        let _ = fs::remove_file(&temporaire_png);
        return;
    }
    if fs::rename(&temporaire_png, fichier_png(dossier, identite)).is_err() {
        let _ = fs::remove_file(&temporaire_png);
        return;
    }

    // 2. Identite ensuite : un lecteur qui voit l'ancienne identite avec le
    //    nouveau PNG retombe sur le decodage (jamais une image erronee).
    let temporaire_identite = temporaire(dossier, identite, "ident");
    if fs::write(&temporaire_identite, identite.texte().as_bytes()).is_err() {
        let _ = fs::remove_file(&temporaire_identite);
        return;
    }
    if fs::rename(&temporaire_identite, fichier_identite(dossier, identite)).is_err() {
        let _ = fs::remove_file(&temporaire_identite);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn dossier_essai(nom: &str) -> Gardien {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base =
            std::env::temp_dir().join(format!("mural-cache-{nom}-{}-{unique}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        Gardien(base)
    }

    /// Supprime le dossier d'essai a la fin du test, meme en cas d'echec.
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

    fn ecrire_image(chemin: &Path, largeur: u32, hauteur: u32, alpha: bool) {
        if alpha {
            let image = image::RgbaImage::from_fn(largeur, hauteur, |x, y| {
                // Un coin totalement transparent, le reste opaque.
                let a = if x < largeur / 2 && y < hauteur / 2 {
                    0
                } else {
                    255
                };
                image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, a])
            });
            image.save(chemin).unwrap();
        } else {
            let image = image::RgbImage::from_fn(largeur, hauteur, |x, y| {
                image::Rgb([(x % 256) as u8, (y % 256) as u8, 64])
            });
            image.save(chemin).unwrap();
        }
    }

    fn entiers(dossier: &Path) -> Vec<PathBuf> {
        let mut liste: Vec<PathBuf> = fs::read_dir(dossier)
            .map(|lecture| {
                lecture
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.is_file())
                    .collect()
            })
            .unwrap_or_default();
        liste.sort();
        liste
    }

    #[test]
    fn dossier_par_defaut_et_surcharge() {
        use std::ffi::OsString;
        assert_eq!(
            dossier_pour(None, Some(OsString::from("/Users/essai"))),
            Some(PathBuf::from("/Users/essai/Library/Caches/Mural/vignettes"))
        );
        assert_eq!(
            dossier_pour(
                Some(OsString::from("/tmp/cache-essai")),
                Some(OsString::from("/Users/essai"))
            ),
            Some(PathBuf::from("/tmp/cache-essai"))
        );
        assert_eq!(
            dossier_pour(Some(OsString::new()), Some(OsString::from("/Users/essai"))),
            None
        );
        assert_eq!(dossier_pour(None, None), None);
    }

    #[test]
    fn froid_puis_chaud_ecrit_et_relit_le_cache() {
        let base = dossier_essai("froid-chaud");
        let source = base.join("source.png");
        let cache = base.join("cache");
        ecrire_image(&source, 720, 440, false);

        let froid = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        assert_eq!((froid.largeur, froid.hauteur), (360, 220));

        let fichiers = entiers(&cache);
        assert_eq!(
            fichiers.len(),
            2,
            "PNG + identite attendus, vu {fichiers:?}"
        );
        assert!(
            fichiers
                .iter()
                .any(|f| f.extension().is_some_and(|e| e == "png"))
        );
        assert!(
            fichiers
                .iter()
                .any(|f| f.extension().is_some_and(|e| e == "ident"))
        );

        let chaud = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        assert_eq!((chaud.largeur, chaud.hauteur), (360, 220));
    }

    #[test]
    fn chaud_lit_vraiment_le_png_du_cache() {
        let base = dossier_essai("lecture-cache");
        let source = base.join("source.png");
        let cache = base.join("cache");
        let marqueur = base.join("marqueur.png");
        ecrire_image(&source, 720, 440, false);
        ecrire_image(&marqueur, 64, 40, false);

        let _ = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        let identite = identite(&source, 180).unwrap();
        fs::copy(&marqueur, fichier_png(&cache, &identite)).unwrap();

        let chaud = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        assert_eq!(
            (chaud.largeur, chaud.hauteur),
            (64, 40),
            "le cache doit etre relu, pas redecode"
        );
    }

    #[test]
    fn source_modifiee_invalide_l_entree() {
        let base = dossier_essai("source-modifiee");
        let source = base.join("source.png");
        let cache = base.join("cache");
        ecrire_image(&source, 720, 440, false);
        let avant = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        assert_eq!((avant.largeur, avant.hauteur), (360, 220));

        // Meme chemin, nouveau contenu et nouvelle taille.
        ecrire_image(&source, 220, 440, false);
        let apres = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        assert_ne!(
            (apres.largeur, apres.hauteur),
            (avant.largeur, avant.hauteur),
            "une source modifiee ne doit jamais relire l'ancienne vignette"
        );
        assert_eq!(
            entiers(&cache).len(),
            4,
            "deux entrees distinctes attendues"
        );
    }

    #[test]
    fn largeur_differente_ne_reutilise_pas() {
        let base = dossier_essai("largeur");
        let source = base.join("source.png");
        let cache = base.join("cache");
        ecrire_image(&source, 720, 440, false);

        let grand = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        let petit = crate::bibliotheque::vignette_avec_cache(&source, 90, Some(&cache)).unwrap();
        assert!(
            petit.largeur < grand.largeur,
            "{} vs {}",
            petit.largeur,
            grand.largeur
        );
        assert_eq!(entiers(&cache).len(), 4);
    }

    #[test]
    fn cache_corrompu_retombe_sur_le_decodage() {
        let base = dossier_essai("corrompu");
        let source = base.join("source.png");
        let cache = base.join("cache");
        ecrire_image(&source, 720, 440, false);
        let _ = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        let identite = identite(&source, 180).unwrap();

        fs::write(fichier_png(&cache, &identite), b"pas un png").unwrap();
        let apres = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        assert_eq!((apres.largeur, apres.hauteur), (360, 220));

        // Le cache a ete reecrit : une nouvelle lecture doit reussir.
        fs::write(fichier_identite(&cache, &identite), b"identite fausse").unwrap();
        let encore = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        assert_eq!((encore.largeur, encore.hauteur), (360, 220));
    }

    #[test]
    fn cache_non_ecrivable_ne_casse_rien() {
        let base = dossier_essai("non-ecrivable");
        let source = base.join("source.png");
        let obstacle = base.join("obstacle");
        ecrire_image(&source, 720, 440, false);
        fs::write(&obstacle, b"fichier").unwrap();

        let vignette =
            crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&obstacle.join("cache")));
        assert!(vignette.is_some());
        let vignette = vignette.unwrap();
        assert_eq!((vignette.largeur, vignette.hauteur), (360, 220));
    }

    #[test]
    fn identite_mensongere_ne_montre_pas_une_autre_image() {
        let base = dossier_essai("collision");
        let source_a = base.join("a.png");
        let source_b = base.join("b.png");
        let cache = base.join("cache");
        ecrire_image(&source_a, 720, 440, false);
        ecrire_image(&source_b, 220, 440, false); // portrait, sortie differente

        let _ = crate::bibliotheque::vignette_avec_cache(&source_a, 180, Some(&cache)).unwrap();
        let identite_a = identite(&source_a, 180).unwrap();
        let identite_b = identite(&source_b, 180).unwrap();
        assert_ne!(cle(&identite_a), cle(&identite_b));

        // Simule une collision : le PNG de A publie sous la cle de B, avec
        // l'identite de A recopiee (donc differente de celle attendue).
        fs::copy(
            fichier_png(&cache, &identite_a),
            fichier_png(&cache, &identite_b),
        )
        .unwrap();
        fs::write(
            fichier_identite(&cache, &identite_b),
            identite_a.texte().as_bytes(),
        )
        .unwrap();

        let b = crate::bibliotheque::vignette_avec_cache(&source_b, 180, Some(&cache)).unwrap();
        assert_eq!(
            (b.largeur, b.hauteur),
            (180, 110),
            "B doit etre decode, jamais l'image de A"
        );
    }

    #[test]
    fn transparence_conservee() {
        let base = dossier_essai("alpha");
        let source = base.join("source.png");
        let cache = base.join("cache");
        ecrire_image(&source, 720, 440, true);

        let froid = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        assert_ne!(
            CGImage::alpha_info(Some(&froid.image)),
            objc2_core_graphics::CGImageAlphaInfo::None
        );
        let chaud = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        assert_ne!(
            CGImage::alpha_info(Some(&chaud.image)),
            objc2_core_graphics::CGImageAlphaInfo::None
        );
        let id = identite(&source, 180).unwrap();
        let png = image::open(fichier_png(&cache, &id)).unwrap().to_rgba8();
        assert!(
            png.pixels().any(|p| p[3] == 0),
            "pixels transparents conserves"
        );
        assert!(
            png.pixels().any(|p| p[3] == 255),
            "pixels opaques conserves"
        );
    }

    #[test]
    fn concurrence_sur_la_meme_cle() {
        let base = dossier_essai("concurrence");
        let source = Arc::new(base.join("source.png"));
        let cache = Arc::new(base.join("cache"));
        ecrire_image(&source, 720, 440, false);

        std::thread::scope(|portee| {
            for _ in 0..8 {
                let source = Arc::clone(&source);
                let cache = Arc::clone(&cache);
                portee.spawn(move || {
                    let vignette =
                        crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache))
                            .expect("vignette concurrente");
                    assert_eq!((vignette.largeur, vignette.hauteur), (360, 220));
                });
            }
        });

        let restants: Vec<PathBuf> = entiers(&cache)
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "tmp"))
            .collect();
        assert!(restants.is_empty(), "temporaires restants : {restants:?}");
        let chaud = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        assert_eq!((chaud.largeur, chaud.hauteur), (360, 220));
    }

    #[test]
    fn sources_jamais_modifiees() {
        let base = dossier_essai("sources");
        let source = base.join("source.png");
        let cache = base.join("cache");
        ecrire_image(&source, 720, 440, false);
        let avant = fs::read(&source).unwrap();
        let liste_avant = entiers(&base);

        for _ in 0..3 {
            let _ = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        }

        assert_eq!(fs::read(&source).unwrap(), avant);
        let liste_apres: Vec<PathBuf> = entiers(&base)
            .into_iter()
            .filter(|p| !p.starts_with(&cache))
            .collect();
        assert_eq!(liste_avant, liste_apres);
    }

    #[test]
    fn identite_change_avec_la_source() {
        let base = dossier_essai("identite-change");
        let source = base.join("source.png");
        ecrire_image(&source, 720, 440, false);
        let avant = identite(&source, 180).unwrap();

        ecrire_image(&source, 720, 440, true); // nouvelle taille et nouvelle date
        let apres = identite(&source, 180).unwrap();
        assert_ne!(avant.texte(), apres.texte());
        assert_ne!(cle(&avant), cle(&apres));
    }

    #[test]
    fn date_seule_invalide_le_cache() {
        let base = dossier_essai("date-seule");
        let source = base.join("source.png");
        let cache = base.join("cache");
        ecrire_image(&source, 720, 440, false);
        let _ = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        let avant = identite(&source, 180).unwrap();
        let date = fs::metadata(&source).unwrap().modified().unwrap();
        fs::File::options()
            .write(true)
            .open(&source)
            .unwrap()
            .set_modified(date + std::time::Duration::from_secs(2))
            .unwrap();
        let apres = identite(&source, 180).unwrap();
        assert_eq!(avant.taille, apres.taille);
        assert_ne!(avant.mtime_ns, apres.mtime_ns);
        assert!(lire(&cache, &apres).is_none());
        let _ = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        assert!(lire(&cache, &apres).is_some());
    }

    #[test]
    fn taille_seule_invalide_le_cache() {
        use std::io::Write;
        let base = dossier_essai("taille-seule");
        let source = base.join("source.png");
        let cache = base.join("cache");
        ecrire_image(&source, 720, 440, false);
        let _ = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        let avant = identite(&source, 180).unwrap();
        let date = fs::metadata(&source).unwrap().modified().unwrap();
        let mut fichier = fs::OpenOptions::new().append(true).open(&source).unwrap();
        fichier.write_all(b"after-png").unwrap();
        fichier.set_modified(date).unwrap();
        let apres = identite(&source, 180).unwrap();
        assert_eq!(avant.mtime_ns, apres.mtime_ns);
        assert_ne!(avant.taille, apres.taille);
        assert!(lire(&cache, &apres).is_none());
        let _ = crate::bibliotheque::vignette_avec_cache(&source, 180, Some(&cache)).unwrap();
        assert!(lire(&cache, &apres).is_some());
    }
}
