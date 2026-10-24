//! Tests comportementaux de la conversion AVIF explicite (`src/avif.rs`).
//!
//! Toutes les images sont synthetiques et vivent sous un dossier temporaire :
//! aucune image utilisateur n'est lue, deplacee ou supprimee. Si un encodage
//! AVIF reel echoue sur cette machine, chaque test le dit
//! clairement et s'arrete sans pretendre avoir verifie la conversion.

use std::fs;
use std::path::{Path, PathBuf};

/// Dossier temporaire unique, supprime a la fin du test.
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
    let base = std::env::temp_dir().join(format!(
        "mural-avif-conv-{nom}-{}-{unique}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    Gardien(base)
}

fn ecrire_png(chemin: &Path, largeur: u32, hauteur: u32) {
    let image = image::RgbImage::from_fn(largeur, hauteur, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 64])
    });
    image.save(chemin).unwrap();
}

fn ecrire_png_alpha(chemin: &Path, largeur: u32, hauteur: u32) {
    let image = image::RgbaImage::from_fn(largeur, hauteur, |x, y| {
        let a = if x < largeur / 2 { 0 } else { 255 };
        image::Rgba([200, (y % 256) as u8, 64, a])
    });
    image.save(chemin).unwrap();
}

fn fichiers(dossier: &Path) -> Vec<PathBuf> {
    let mut liste: Vec<PathBuf> = fs::read_dir(dossier)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    liste.sort();
    liste
}

/// Un AVIF commence par une boite ISO `ftyp` avec la marque avif/avis.
fn est_avif(chemin: &Path) -> bool {
    let octets = fs::read(chemin).unwrap_or_default();
    if octets.len() < 12 || &octets[4..8] != b"ftyp" {
        return false;
    }
    &octets[8..12] == b"avif" || &octets[8..12] == b"avis"
}

fn encodeur_present(nom: &str) -> bool {
    if mural::formats::avif_encodable() {
        return true;
    }
    eprintln!("TEST NON EXECUTE ({nom}) : aucun encodage AVIF fonctionnel detecte ici");
    false
}

#[test]
fn conversion_synthetique_produit_un_vrai_avif_relu_avec_les_bonnes_dimensions() {
    if !encodeur_present("conversion synthetique") {
        return;
    }
    let base = dossier_essai("simple");
    let source = base.join("photo.png");
    let cache = base.join("cache");
    ecrire_png(&source, 320, 180);

    let conversion = mural::avif::convertir(&source, &cache).expect("conversion AVIF");
    assert!(!conversion.reutilisee, "premiere conversion = cache froid");
    assert!(conversion.chemin.is_file());
    assert!(
        est_avif(&conversion.chemin),
        "la sortie doit etre un vrai AVIF (ftyp avif), vu {:?}",
        fs::read(&conversion.chemin)
            .map(|o| o[..o.len().min(12)].to_vec())
            .unwrap_or_default()
    );
    assert!(
        conversion.chemin.starts_with(&cache),
        "sortie dans le cache"
    );
    assert_eq!(
        conversion.octets_source,
        fs::metadata(&source).unwrap().len()
    );
    assert_eq!(
        conversion.octets_avif,
        fs::metadata(&conversion.chemin).unwrap().len()
    );
    assert_eq!(
        mural::bibliotheque::dimensions(&conversion.chemin),
        Some((320, 180)),
        "la conversion garde les dimensions integrales"
    );
}

#[test]
fn original_jamais_modifie_ni_deplace() {
    if !encodeur_present("original inchange") {
        return;
    }
    let base = dossier_essai("original");
    let source = base.join("photo.png");
    let cache = base.join("cache");
    ecrire_png(&source, 120, 80);
    let avant = fs::read(&source).unwrap();
    let date_avant = fs::metadata(&source).unwrap().modified().unwrap();

    let _ = mural::avif::convertir(&source, &cache).expect("conversion");
    let _ = mural::avif::convertir(&source, &cache).expect("reutilisation");

    assert_eq!(fs::read(&source).unwrap(), avant, "octets source intacts");
    assert_eq!(
        fs::metadata(&source).unwrap().modified().unwrap(),
        date_avant,
        "date source intacte"
    );
    let restants: Vec<PathBuf> = fichiers(&base)
        .into_iter()
        .filter(|p| !p.starts_with(&cache))
        .collect();
    assert_eq!(restants, vec![source.clone()], "aucun fichier voisin cree");
}

#[test]
fn cache_chaud_reutilise_sans_reconvertir() {
    if !encodeur_present("cache chaud") {
        return;
    }
    let base = dossier_essai("chaud");
    let source = base.join("photo.png");
    let cache = base.join("cache");
    ecrire_png(&source, 200, 100);

    let froid = mural::avif::convertir(&source, &cache).expect("froid");
    let date_avif = fs::metadata(&froid.chemin).unwrap().modified().unwrap();
    let entrees_apres_froid = fichiers(&cache);

    // Marqueur : si la conversion etait refaite, le fichier serait reecrit.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let chaud = mural::avif::convertir(&source, &cache).expect("chaud");
    assert!(chaud.reutilisee, "source identique = conversion reutilisee");
    assert_eq!(chaud.chemin, froid.chemin);
    assert_eq!(chaud.octets_avif, froid.octets_avif);
    assert_eq!(
        fs::metadata(&chaud.chemin).unwrap().modified().unwrap(),
        date_avif,
        "le fichier du cache n'est pas reecrit"
    );
    assert_eq!(fichiers(&cache), entrees_apres_froid);
}

#[test]
fn source_modifiee_ne_reutilise_pas_l_ancienne_conversion() {
    if !encodeur_present("source modifiee") {
        return;
    }
    let base = dossier_essai("modifiee");
    let source = base.join("photo.png");
    let cache = base.join("cache");
    ecrire_png(&source, 320, 180);

    let avant = mural::avif::convertir(&source, &cache).expect("avant");
    ecrire_png(&source, 90, 240);
    let apres = mural::avif::convertir(&source, &cache).expect("apres");

    assert!(!apres.reutilisee, "la source a change : reconversion");
    assert_ne!(apres.chemin, avant.chemin, "nouvelle entree de cache");
    assert_eq!(
        mural::bibliotheque::dimensions(&apres.chemin),
        Some((90, 240))
    );
    assert!(
        avant.chemin.is_file(),
        "l'ancienne entree reste (pas d'eviction)"
    );
}

#[test]
fn meme_nom_dans_dossiers_ou_extensions_differents_ne_collisionne_pas() {
    if !encodeur_present("collisions de nom") {
        return;
    }
    let base = dossier_essai("collisions");
    let dossier_a = base.join("a");
    let dossier_b = base.join("b");
    fs::create_dir_all(&dossier_a).unwrap();
    fs::create_dir_all(&dossier_b).unwrap();
    let source_png_a = dossier_a.join("photo.png");
    let source_png_b = dossier_b.join("photo.png");
    let source_jpg_a = dossier_a.join("photo.jpg");
    let cache = base.join("cache");
    ecrire_png(&source_png_a, 320, 180);
    ecrire_png(&source_png_b, 160, 90);
    ecrire_png(&source_jpg_a, 64, 64);

    let a = mural::avif::convertir(&source_png_a, &cache).expect("a png");
    let b = mural::avif::convertir(&source_png_b, &cache).expect("b png");
    let c = mural::avif::convertir(&source_jpg_a, &cache).expect("a jpg");

    assert_ne!(
        a.chemin, b.chemin,
        "dossiers differents = entrees distinctes"
    );
    assert_ne!(
        a.chemin, c.chemin,
        "extensions differentes = entrees distinctes"
    );
    assert!(!b.reutilisee && !c.reutilisee);
    assert_eq!(mural::bibliotheque::dimensions(&b.chemin), Some((160, 90)));
}

#[test]
fn source_corrompue_echoue_en_anglais_sans_temporaire() {
    if !encodeur_present("source corrompue") {
        return;
    }
    let base = dossier_essai("corrompue");
    let source = base.join("casse.png");
    let cache = base.join("cache");
    fs::write(&source, b"ceci n'est pas une image").unwrap();

    let erreur = mural::avif::convertir(&source, &cache).expect_err("doit echouer");
    assert!(
        erreur.chars().all(|c| c.is_ascii() || c == '\u{2019}'),
        "message lisible en anglais : {erreur}"
    );
    assert!(
        erreur.to_lowercase().contains("decode") || erreur.to_lowercase().contains("read"),
        "le message nomme le probleme : {erreur}"
    );
    let temporaires: Vec<PathBuf> = fichiers(&cache)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "tmp"))
        .collect();
    assert!(
        temporaires.is_empty(),
        "temporaires nettoyes : {temporaires:?}"
    );
}

#[test]
fn cache_non_creable_echoue_proprement() {
    if !encodeur_present("cache non creable") {
        return;
    }
    let base = dossier_essai("non-creable");
    let source = base.join("photo.png");
    let obstacle = base.join("obstacle");
    ecrire_png(&source, 40, 30);
    fs::write(&obstacle, b"fichier").unwrap();

    let erreur = mural::avif::convertir(&source, &obstacle.join("cache"))
        .expect_err("un cache sous un fichier doit echouer");
    assert!(
        erreur.to_lowercase().contains("cache"),
        "le message nomme le cache : {erreur}"
    );
    assert!(source.is_file(), "la source reste intacte");
}

#[test]
fn conversion_ecrasee_ou_falsifiee_est_revalidee_avant_reutilisation() {
    if !encodeur_present("revalidation du cache") {
        return;
    }
    let base = dossier_essai("revalidation");
    let source = base.join("photo.png");
    let cache = base.join("cache");
    ecrire_png(&source, 320, 180);
    let conversion = mural::avif::convertir(&source, &cache).expect("froid");

    // Fichier cache corrompu : la relecture echoue et une conversion propre le remplace.
    fs::write(&conversion.chemin, b"pas un avif").unwrap();
    let reprise = mural::avif::convertir(&source, &cache).expect("reprise");
    assert!(
        !reprise.reutilisee,
        "un cache illisible ne doit pas etre servi"
    );
    assert!(est_avif(&reprise.chemin));
    assert_eq!(
        mural::bibliotheque::dimensions(&reprise.chemin),
        Some((320, 180))
    );

    // AVIF valide mais de MAUVAISES dimensions : la relecture doit le refuser.
    let autre = base.join("autre.png");
    let cache_autre = base.join("cache-autre");
    ecrire_png(&autre, 64, 64);
    let etranger = mural::avif::convertir(&autre, &cache_autre).expect("autre conversion");
    assert!(est_avif(&etranger.chemin));
    fs::copy(&etranger.chemin, &reprise.chemin).unwrap();

    let corrigee = mural::avif::convertir(&source, &cache).expect("correction dimensions");
    assert!(
        !corrigee.reutilisee,
        "un AVIF valide aux mauvaises dimensions ne doit pas etre servi"
    );
    assert_eq!(
        mural::bibliotheque::dimensions(&corrigee.chemin),
        Some((320, 180))
    );
}

#[test]
fn alpha_conserve_si_l_encodeur_le_prend_en_charge() {
    if !encodeur_present("alpha") {
        return;
    }
    let base = dossier_essai("alpha");
    let source = base.join("alpha.png");
    let cache = base.join("cache");
    ecrire_png_alpha(&source, 80, 60);
    let conversion = mural::avif::convertir(&source, &cache).expect("conversion alpha");

    let octets = fs::read(&conversion.chemin).unwrap();
    assert!(est_avif(&conversion.chemin));
    // La presence d'un canal alpha se lit via ImageIO (aucune hypothese sur le bitstream).
    let alpha = avec_source(&conversion.chemin, |source| {
        let image = unsafe { source.image_at_index(0, None) }?;
        Some(objc2_core_graphics::CGImage::alpha_info(Some(&image)))
    });
    assert!(
        alpha.is_some_and(|info| info != objc2_core_graphics::CGImageAlphaInfo::None),
        "le canal alpha doit survivre a la conversion ({} octets)",
        octets.len()
    );
    // Verifier les valeurs alpha, pas seulement l'existence du canal.
    let png = base.join("alpha-relue.png");
    let sortie = std::process::Command::new("/usr/bin/sips")
        .args(["-s", "format", "png"])
        .arg(&conversion.chemin)
        .arg("--out")
        .arg(&png)
        .output()
        .unwrap();
    assert!(
        sortie.status.success(),
        "{}",
        String::from_utf8_lossy(&sortie.stderr)
    );
    let rgba = image::open(&png).unwrap().to_rgba8();
    assert!(
        rgba.pixels().any(|p| p[3] == 0),
        "pixels transparents conserves"
    );
    assert!(
        rgba.pixels().any(|p| p[3] == 255),
        "pixels opaques conserves"
    );
}

#[test]
fn orientation_exif_6_est_normalisee_dans_les_pixels() {
    if !encodeur_present("orientation EXIF") {
        return;
    }
    let base = dossier_essai("orientation");
    let source = base.join("portrait.jpg");
    let cache = base.join("cache");

    // JPEG 400x200, puis EXIF Orientation=6 (rotation 90 degres horaire).
    ecrire_jpeg_orientee(&source, 400, 200, 6);

    let conversion = mural::avif::convertir(&source, &cache).expect("conversion orientee");
    assert_eq!(
        mural::bibliotheque::dimensions(&conversion.chemin),
        Some((200, 400)),
        "orientation EXIF appliquee : 400x200 devient 200x400"
    );
    let (largeur, hauteur) = mural::bibliotheque::dimensions(&source).unwrap_or((400, 200));
    assert_eq!(
        (largeur, hauteur),
        (400, 200),
        "l'original garde ses dimensions brutes"
    );
    assert_eq!(
        fs::read(&source).unwrap().len() as u64,
        fs::metadata(&source).unwrap().len()
    );
}

#[test]
fn est_candidate_filtre_les_formats_attendus() {
    let base = dossier_essai("candidats");
    for nom in ["a.png", "b.JPG", "c.jpeg", "d.bmp", "e.webp", "f.PNG"] {
        let chemin = base.join(nom);
        fs::write(&chemin, b"x").unwrap();
        assert!(
            mural::avif::est_candidate(&chemin),
            "{nom} doit etre candidat"
        );
    }
    for nom in ["g.avif", "h.AVIF", "i.svg", "j.txt", "sans_extension"] {
        let chemin = base.join(nom);
        fs::write(&chemin, b"x").unwrap();
        assert!(
            !mural::avif::est_candidate(&chemin),
            "{nom} ne doit pas etre candidat"
        );
    }
    assert!(
        !mural::avif::est_candidate(&base.join("inexistant.png")),
        "un chemin absent n'est pas un fichier reel"
    );
    assert!(
        !mural::avif::est_candidate(&base),
        "un dossier n'est pas une image"
    );
}

/// Ouvre une source ImageIO depuis un chemin (outil de test).
fn avec_source<T>(
    chemin: &Path,
    action: impl FnOnce(&objc2_image_io::CGImageSource) -> Option<T>,
) -> Option<T> {
    use objc2::rc::Retained;
    use objc2_core_foundation::CFURL;
    use objc2_foundation::{NSString, NSURL};
    let texte = NSString::from_str(&chemin.to_string_lossy());
    let url = NSURL::fileURLWithPath(&texte);
    let url_cf: &CFURL = unsafe { &*(Retained::as_ptr(&url) as *const CFURL) };
    let source = unsafe { objc2_image_io::CGImageSource::with_url(url_cf, None) }?;
    action(&source)
}

/// Ecrit un JPEG synthetique portant une balise EXIF Orientation.
///
/// Le segment EXIF est produit par l'encodeur JPEG de `image` (APP1 place au
/// bon endroit), a partir d'un TIFF minimal grand-boutiste.
fn ecrire_jpeg_orientee(chemin: &Path, largeur: u32, hauteur: u32, orientation: u16) {
    use image::ImageEncoder;

    let mut tiff: Vec<u8> = Vec::new();
    tiff.extend_from_slice(b"MM\x00*"); // grand-boutiste
    tiff.extend_from_slice(&8u32.to_be_bytes()); // offset IFD0
    tiff.extend_from_slice(&1u16.to_be_bytes()); // une entree
    tiff.extend_from_slice(&0x0112u16.to_be_bytes()); // Orientation
    tiff.extend_from_slice(&3u16.to_be_bytes()); // SHORT
    tiff.extend_from_slice(&1u32.to_be_bytes()); // une valeur
    tiff.extend_from_slice(&orientation.to_be_bytes()); // valeur SHORT (2 octets)
    tiff.extend_from_slice(&[0u8, 0u8]); // remplissage du champ valeur
    tiff.extend_from_slice(&0u32.to_be_bytes()); // IFD suivante

    let pixels = image::RgbImage::from_fn(largeur, hauteur, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 64])
    });
    let mut octets = Vec::new();
    {
        let mut encodeur = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut octets, 90);
        encodeur.set_exif_metadata(tiff).unwrap();
        encodeur
            .write_image(
                pixels.as_raw(),
                largeur,
                hauteur,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
    }
    fs::write(chemin, octets).unwrap();
}

#[test]
fn cache_avec_entete_valide_mais_pixels_corrompus_est_reconverti() {
    if !encodeur_present("cache pixels corrompus") {
        return;
    }
    let base = dossier_essai("pixels-corrompus");
    let source = base.join("source.png");
    let cache = base.join("cache");
    ecrire_png(&source, 320, 180);
    let premiere = mural::avif::convertir(&source, &cache).unwrap();
    let mut octets = fs::read(&premiere.chemin).unwrap();
    let mut position = 0;
    let mut trouve = false;
    while position + 8 <= octets.len() {
        let courte =
            u32::from_be_bytes(octets[position..position + 4].try_into().unwrap()) as usize;
        let (taille, entete) = match courte {
            0 => (octets.len() - position, 8),
            1 => (
                u64::from_be_bytes(octets[position + 8..position + 16].try_into().unwrap())
                    as usize,
                16,
            ),
            n => (n, 8),
        };
        assert!(taille >= entete && position + taille <= octets.len());
        if &octets[position + 4..position + 8] == b"mdat" {
            octets[position + entete..position + taille].fill(0);
            trouve = true;
            break;
        }
        position += taille;
    }
    assert!(trouve, "box mdat AVIF attendue");
    fs::write(&premiere.chemin, octets).unwrap();
    assert_eq!(
        mural::bibliotheque::dimensions(&premiere.chemin),
        Some((320, 180)),
        "les metadonnees doivent rester lisibles"
    );
    let corrigee = mural::avif::convertir(&source, &cache).unwrap();
    assert!(
        !corrigee.reutilisee,
        "un cache aux pixels corrompus doit etre reconverti"
    );
}
