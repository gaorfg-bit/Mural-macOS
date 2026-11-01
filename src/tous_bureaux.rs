//! Fond sur tous les bureaux (Spaces) — **experimental, hors API publique**.
//!
//! `NSWorkspace` ne change que le bureau actif de chaque ecran. Pour atteindre les
//! autres, on edite le magasin prive de macOS
//! (`~/Library/Application Support/com.apple.wallpaper/Store/Index.plist`) puis on
//! relance `WallpaperAgent`, qui relit le fichier.
//!
//! Garde-fous : structure verifiee avant toute ecriture (sinon rien n'est touche),
//! sauvegarde du plist d'origine et du dernier etat, remplacement atomique, seules
//! les entrees **deja presentes** pour les ecrans vises sont modifiees (rien n'est
//! cree). Les images de l'utilisateur ne sont jamais ecrites.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::msg_send;
use objc2_foundation::{
    NSArray, NSData, NSDictionary, NSMutableArray, NSMutableDictionary,
    NSPropertyListFormat, NSPropertyListMutabilityOptions, NSPropertyListSerialization, NSString,
    NSURL,
};

type Dict = NSMutableDictionary;

const FOURNISSEUR_IMAGE: &str = "com.apple.wallpaper.choice.image";

/// Erreurs de propagation. Aucune n'a laisse le magasin a moitie ecrit.
#[derive(Debug)]
pub enum ErreurBureaux {
    MagasinIntrouvable(PathBuf),
    Illisible(String),
    /// Le plist n'a pas la forme attendue (version de macOS differente ?).
    StructureInconnue(String),
    Sauvegarde(String),
    Ecriture(String),
}

impl fmt::Display for ErreurBureaux {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MagasinIntrouvable(p) => write!(f, "Wallpaper store not found: {}", p.display()),
            Self::Illisible(m) => write!(f, "Wallpaper store unreadable: {m}"),
            Self::StructureInconnue(m) => {
                write!(f, "Unrecognized wallpaper store (left untouched): {m}")
            }
            Self::Sauvegarde(m) => write!(f, "Backup failed (store left untouched): {m}"),
            Self::Ecriture(m) => write!(f, "Could not write wallpaper store: {m}"),
        }
    }
}

impl std::error::Error for ErreurBureaux {}

/// Ce qui a reellement change.
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct Rapport {
    /// Entrees de `Displays` modifiees (fond par defaut d'un ecran).
    pub ecrans: usize,
    /// Entrees `Spaces/<id>/Displays/<ecran>` modifiees.
    pub bureaux: usize,
}

impl Rapport {
    pub fn total(&self) -> usize {
        self.ecrans + self.bureaux
    }
}

/// Emplacement du magasin. `MURAL_MAGASIN_FONDS` le remplace (recettes, tests) ;
/// dans ce cas `WallpaperAgent` n'est jamais relance.
pub fn chemin_magasin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MURAL_MAGASIN_FONDS").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library/Application Support/com.apple.wallpaper/Store/Index.plist"),
    )
}

/// Dossier des sauvegardes, a cote des reglages de Mural.
pub fn dossier_sauvegardes() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MURAL_REGLAGES").filter(|p| !p.is_empty()) {
        return Path::new(&p).parent().map(|d| d.join("sauvegardes-fonds"));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/Mural/sauvegardes-fonds"))
}

/// Propage chaque `(image, ecrans)` sur tous les bureaux (ecrans = UUID ColorSync),
/// puis relance `WallpaperAgent` **une seule fois** si quelque chose a change et
/// que le magasin est reel.
pub fn appliquer_partout(groupes: &[(PathBuf, Vec<String>)]) -> Result<Rapport, ErreurBureaux> {
    let magasin = chemin_magasin().ok_or_else(|| ErreurBureaux::Illisible("HOME absent".into()))?;
    let sauvegardes = dossier_sauvegardes()
        .ok_or_else(|| ErreurBureaux::Sauvegarde("dossier introuvable".into()))?;
    let mut total = Rapport::default();
    for (image, cles) in groupes {
        let r = propager(&magasin, &sauvegardes, image, cles)?;
        total.ecrans += r.ecrans;
        total.bureaux += r.bureaux;
    }
    if total.total() > 0 && std::env::var_os("MURAL_MAGASIN_FONDS").is_none() {
        relancer_agent();
    }
    Ok(total)
}

/// Relance `WallpaperAgent` (launchd le redemarre seul).
pub fn relancer_agent() {
    let _ = std::process::Command::new("/usr/bin/killall")
        .arg("WallpaperAgent")
        .status();
}

/// Coeur testable : edite `magasin` en place (apres sauvegarde), sans relancer l'agent.
pub fn propager(
    magasin: &Path,
    sauvegardes: &Path,
    image: &Path,
    cles: &[String],
) -> Result<Rapport, ErreurBureaux> {
    if !magasin.is_file() {
        return Err(ErreurBureaux::MagasinIntrouvable(magasin.to_path_buf()));
    }
    let octets = fs::read(magasin).map_err(|e| ErreurBureaux::Illisible(e.to_string()))?;
    let donnees = NSData::with_bytes(&octets);
    let racine = lire_modifiable(&donnees)?;
    let racine_dict = dictionnaire(&racine)
        .ok_or_else(|| ErreurBureaux::StructureInconnue("racine non dictionnaire".into()))?;

    let ecrans = enfant_dict(racine_dict, "Displays")
        .ok_or_else(|| ErreurBureaux::StructureInconnue("« Displays » absent".into()))?;
    let bureaux = enfant_dict(racine_dict, "Spaces")
        .ok_or_else(|| ErreurBureaux::StructureInconnue("« Spaces » absent".into()))?;

    let configuration = configuration_image(image)?;
    let mut rapport = Rapport::default();
    let mut reconnus = 0usize;

    for cle in cles {
        if let Some(entree) = entree_insensible(&ecrans, cle) {
            reconnus += 1;
            if maj_bureau(&entree, &configuration)? {
                rapport.ecrans += 1;
            }
        }
        for (_, espace) in cles_et_valeurs(&bureaux) {
            let Some(espace) = dictionnaire(&espace) else { continue };
            let Some(ecrans_espace) = enfant_dict(espace, "Displays") else { continue };
            if let Some(entree) = entree_insensible(&ecrans_espace, cle) {
                reconnus += 1;
                if maj_bureau(&entree, &configuration)? {
                    rapport.bureaux += 1;
                }
            }
        }
    }
    if reconnus == 0 {
        return Err(ErreurBureaux::StructureInconnue(
            "aucun des ecrans vises n'apparait dans le magasin".into(),
        ));
    }
    if rapport.total() == 0 {
        return Ok(rapport); // deja a jour : on n'ecrit rien
    }

    // Verification de bout en bout avant d'ecrire : le plist doit se reserialiser.
    let sortie = unsafe {
        NSPropertyListSerialization::dataWithPropertyList_format_options_error(
            &racine,
            NSPropertyListFormat::BinaryFormat_v1_0,
            0,
        )
    }
    .map_err(|e| ErreurBureaux::Ecriture(e.localizedDescription().to_string()))?;
    let sortie = sortie.to_vec();
    let relu = NSData::with_bytes(&sortie);
    lire_modifiable(&relu).map_err(|_| ErreurBureaux::Ecriture("relecture impossible".into()))?;

    sauvegarder(magasin, sauvegardes)?;
    ecrire_atomique(magasin, &sortie)?;
    Ok(rapport)
}

/// Garde l'original (jamais ecrase) et le dernier etat avant modification.
fn sauvegarder(magasin: &Path, dossier: &Path) -> Result<(), ErreurBureaux> {
    let err = |e: std::io::Error| ErreurBureaux::Sauvegarde(e.to_string());
    fs::create_dir_all(dossier).map_err(err)?;
    let original = dossier.join("Index.plist.original");
    if !original.exists() {
        fs::copy(magasin, &original).map_err(err)?;
    }
    fs::copy(magasin, dossier.join("Index.plist.derniere")).map_err(err)?;
    Ok(())
}

fn ecrire_atomique(cible: &Path, octets: &[u8]) -> Result<(), ErreurBureaux> {
    let err = |e: std::io::Error| ErreurBureaux::Ecriture(e.to_string());
    let tmp = cible.with_extension("plist.mural-tmp");
    fs::write(&tmp, octets).map_err(err)?;
    if let Ok(meta) = fs::metadata(cible) {
        let _ = fs::set_permissions(&tmp, meta.permissions());
    }
    if let Err(e) = fs::rename(&tmp, cible) {
        let _ = fs::remove_file(&tmp);
        return Err(err(e));
    }
    Ok(())
}

/// Plist `{type: imageFile, url: {relative: file://...}}`, en binaire.
fn configuration_image(image: &Path) -> Result<Retained<NSData>, ErreurBureaux> {
    let chemin = NSString::from_str(&image.to_string_lossy());
    let url = NSURL::fileURLWithPath(&chemin);
    let texte = url
        .absoluteString()
        .ok_or_else(|| ErreurBureaux::Ecriture("URL de l'image invalide".into()))?;
    let (k_relative, k_type, k_url) = (
        NSString::from_str("relative"),
        NSString::from_str("type"),
        NSString::from_str("url"),
    );
    let relatif: Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_retained_objects(&[&*k_relative], &[objet(texte)]);
    let racine: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_retained_objects(
        &[&*k_type, &*k_url],
        &[objet(NSString::from_str("imageFile")), objet(relatif)],
    );
    unsafe {
        NSPropertyListSerialization::dataWithPropertyList_format_options_error(
            &racine,
            NSPropertyListFormat::BinaryFormat_v1_0,
            0,
        )
    }
    .map_err(|e| ErreurBureaux::Ecriture(e.localizedDescription().to_string()))
}

/// Remplace le fond « Desktop » d'une entree d'ecran. `Ok(false)` si deja identique
/// ; `Err` si la forme est inattendue (rien n'est alors ecrit nulle part).
fn maj_bureau(entree: &AnyObject, configuration: &NSData) -> Result<bool, ErreurBureaux> {
    let inconnu = |m: &str| ErreurBureaux::StructureInconnue(m.into());
    let entree = dictionnaire(entree).ok_or_else(|| inconnu("entree d'ecran non dictionnaire"))?;
    let bureau = enfant_dict(entree, "Desktop").ok_or_else(|| inconnu("« Desktop » absent"))?;
    let contenu = enfant_dict(&bureau, "Content").ok_or_else(|| inconnu("« Content » absent"))?;
    let choix = obtenir(&contenu, "Choices").ok_or_else(|| inconnu("« Choices » absent"))?;
    let tableau = choix
        .downcast_ref::<NSMutableArray>()
        .ok_or_else(|| inconnu("« Choices » non tableau"))?;
    if tableau.count() != 1 {
        return Err(inconnu("« Choices » ne contient pas exactement un choix"));
    }
    let premier = tableau.objectAtIndex(0);
    let premier = dictionnaire(&premier).ok_or_else(|| inconnu("choix non dictionnaire"))?;

    let fournisseur = NSString::from_str(FOURNISSEUR_IMAGE);
    let deja_fournisseur = obtenir(premier, "Provider")
        .and_then(|p| p.downcast_ref::<NSString>().map(|s| s.to_string()))
        .is_some_and(|p| p == FOURNISSEUR_IMAGE);
    let deja_config = obtenir(premier, "Configuration")
        .and_then(|c| c.downcast_ref::<NSData>().map(|d| d.to_vec()))
        .is_some_and(|d| d == configuration.to_vec());
    if deja_fournisseur && deja_config {
        return Ok(false);
    }
    poser(premier, "Provider", &*fournisseur);
    poser(premier, "Configuration", configuration);
    poser(premier, "Files", &*NSArray::<AnyObject>::new());
    Ok(true)
}

// --- Outils Foundation (dictionnaires heterogenes) ---------------------------

/// Remonte n'importe quel objet Foundation au type dynamique `AnyObject`.
fn objet<T: objc2::Message>(o: Retained<T>) -> Retained<AnyObject> {
    // Surete : tout objet Objective-C est un `AnyObject` ; seul le type statique change.
    unsafe { Retained::cast_unchecked(o) }
}

fn lire_modifiable(donnees: &NSData) -> Result<Retained<AnyObject>, ErreurBureaux> {
    let mut format = NSPropertyListFormat(0);
    unsafe {
        NSPropertyListSerialization::propertyListWithData_options_format_error(
            donnees,
            NSPropertyListMutabilityOptions::MutableContainers,
            &mut format,
        )
    }
    .map_err(|e| ErreurBureaux::Illisible(e.localizedDescription().to_string()))
}

fn dictionnaire(objet: &AnyObject) -> Option<&Dict> {
    objet.downcast_ref::<Dict>()
}

fn obtenir(d: &Dict, cle: &str) -> Option<Retained<AnyObject>> {
    d.objectForKey(&NSString::from_str(cle))
}

fn enfant_dict(
    d: &Dict,
    cle: &str,
) -> Option<Retained<Dict>> {
    let objet = obtenir(d, cle)?;
    objet
        .downcast_ref::<Dict>()?;
    // Surete : le downcast vient de reussir, le type dynamique est confirme.
    Some(unsafe { Retained::cast_unchecked(objet) })
}

fn cles_et_valeurs(
    d: &Dict,
) -> Vec<(String, Retained<AnyObject>)> {
    let cles = d.allKeys();
    cles.iter()
        .filter_map(|k| {
            let nom = k.downcast_ref::<NSString>()?.to_string();
            Some((nom, d.objectForKey(&k)?))
        })
        .collect()
}

fn entree_insensible(
    d: &Dict,
    cle: &str,
) -> Option<Retained<AnyObject>> {
    cles_et_valeurs(d)
        .into_iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(cle))
        .map(|(_, v)| v)
}

fn poser<T: objc2::Message>(d: &Dict, cle: &str, valeur: &T) {
    let cle = NSString::from_str(cle);
    // Surete : `setObject:forKey:` accepte tout objet et une cle NSString (NSCopying).
    unsafe {
        let _: () = msg_send![d, setObject: valeur, forKey: &*cle];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fabrique un magasin synthetique : 1 ecran, 2 bureaux dont 1 sans l'ecran.
    fn magasin_synthetique(dossier: &Path, ecran: &str, autre: &str) -> PathBuf {
        let chemin = dossier.join("Index.plist");
        let entree = |source: &str| {
            let cfg = configuration_image(Path::new(source)).unwrap();
            format!("<data>{}</data>", base64(&cfg.to_vec()))
        };
        let choix = |source: &str| {
            format!(
                "<dict><key>Desktop</key><dict><key>Content</key><dict><key>Choices</key><array><dict><key>Configuration</key>{}<key>Files</key><array/><key>Provider</key><string>{}</string></dict></array><key>Shuffle</key><string>$null</string></dict><key>LastSet</key><integer>1</integer></dict><key>Type</key><string>individual</string></dict>",
                entree(source),
                FOURNISSEUR_IMAGE
            )
        };
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict>\
<key>AllSpacesAndDisplays</key><dict/>\
<key>Displays</key><dict><key>{ecran}</key>{}</dict>\
<key>Spaces</key><dict>\
<key>S1</key><dict><key>Default</key><dict/><key>Displays</key><dict><key>{ecran}</key>{}</dict></dict>\
<key>S2</key><dict><key>Default</key><dict/><key>Displays</key><dict><key>{autre}</key>{}</dict></dict>\
</dict></dict></plist>",
            choix("/tmp/a.png"),
            choix("/tmp/b.png"),
            choix("/tmp/c.png"),
        );
        fs::write(&chemin, xml).unwrap();
        chemin
    }

    fn base64(octets: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut s = String::new();
        for bloc in octets.chunks(3) {
            let n = bloc.iter().enumerate().fold(0u32, |a, (i, b)| a | (*b as u32) << (16 - 8 * i));
            for i in 0..4 {
                if i <= bloc.len() {
                    s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    s.push('=');
                }
            }
        }
        s
    }

    fn temporaire(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mural-bureaux-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn lire_cfg(magasin: &Path, chemin: &[&str]) -> String {
        let donnees = NSData::with_bytes(&fs::read(magasin).unwrap());
        let racine = lire_modifiable(&donnees).unwrap();
        let mut courant: Retained<AnyObject> = racine;
        for etape in chemin {
            courant = if let Ok(i) = etape.parse::<usize>() {
                let t = courant.downcast_ref::<NSMutableArray>().unwrap();
                t.objectAtIndex(i)
            } else {
                obtenir(dictionnaire(&courant).unwrap(), etape).unwrap()
            };
        }
        let d = courant.downcast_ref::<NSData>().unwrap();
        let plist = lire_modifiable(&NSData::with_bytes(&d.to_vec())).unwrap();
        let url = enfant_dict(dictionnaire(&plist).unwrap(), "url").unwrap();
        obtenir(&url, "relative")
            .unwrap()
            .downcast_ref::<NSString>()
            .unwrap()
            .to_string()
    }

    #[test]
    fn modifie_ecran_et_bureaux_vises_seulement() {
        let d = temporaire("ok");
        let magasin = magasin_synthetique(&d, "AAAA-1111", "BBBB-2222");
        let r = propager(&magasin, &d.join("sauv"), Path::new("/tmp/Mon Image.png"), &["aaaa-1111".into()]).unwrap();
        assert_eq!(r, Rapport { ecrans: 1, bureaux: 1 });
        let chemin = ["Displays", "AAAA-1111", "Desktop", "Content", "Choices", "0", "Configuration"];
        assert_eq!(lire_cfg(&magasin, &chemin), "file:///tmp/Mon%20Image.png");
        let s1 = ["Spaces", "S1", "Displays", "AAAA-1111", "Desktop", "Content", "Choices", "0", "Configuration"];
        assert_eq!(lire_cfg(&magasin, &s1), "file:///tmp/Mon%20Image.png");
        // L'autre ecran, dans S2, n'est pas touche.
        let s2 = ["Spaces", "S2", "Displays", "BBBB-2222", "Desktop", "Content", "Choices", "0", "Configuration"];
        assert_eq!(lire_cfg(&magasin, &s2), "file:///tmp/c.png");
    }

    #[test]
    fn sauvegardes_creees_et_original_conserve() {
        let d = temporaire("sauv");
        let magasin = magasin_synthetique(&d, "AAAA-1111", "BBBB-2222");
        let avant = fs::read(&magasin).unwrap();
        let sauv = d.join("sauv");
        propager(&magasin, &sauv, Path::new("/tmp/x.png"), &["AAAA-1111".into()]).unwrap();
        propager(&magasin, &sauv, Path::new("/tmp/y.png"), &["AAAA-1111".into()]).unwrap();
        assert_eq!(fs::read(sauv.join("Index.plist.original")).unwrap(), avant);
        assert_ne!(fs::read(sauv.join("Index.plist.derniere")).unwrap(), avant);
    }

    #[test]
    fn deja_a_jour_n_ecrit_rien() {
        let d = temporaire("idem");
        let magasin = magasin_synthetique(&d, "AAAA-1111", "BBBB-2222");
        let sauv = d.join("sauv");
        propager(&magasin, &sauv, Path::new("/tmp/x.png"), &["AAAA-1111".into()]).unwrap();
        let apres = fs::read(&magasin).unwrap();
        let r = propager(&magasin, &sauv, Path::new("/tmp/x.png"), &["AAAA-1111".into()]).unwrap();
        assert_eq!(r.total(), 0);
        assert_eq!(fs::read(&magasin).unwrap(), apres);
    }

    #[test]
    fn ecran_inconnu_ou_structure_etrangere_ne_touche_a_rien() {
        let d = temporaire("refus");
        let magasin = magasin_synthetique(&d, "AAAA-1111", "BBBB-2222");
        let avant = fs::read(&magasin).unwrap();
        let inconnu = propager(&magasin, &d.join("sauv"), Path::new("/tmp/x.png"), &["ZZZZ".into()]);
        assert!(matches!(inconnu, Err(ErreurBureaux::StructureInconnue(_))));
        assert_eq!(fs::read(&magasin).unwrap(), avant);

        let etranger = d.join("autre.plist");
        fs::write(&etranger, "<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>x</key><integer>1</integer></dict></plist>").unwrap();
        let r = propager(&etranger, &d.join("sauv"), Path::new("/tmp/x.png"), &["AAAA-1111".into()]);
        assert!(matches!(r, Err(ErreurBureaux::StructureInconnue(_))));
        assert!(!d.join("sauv").exists(), "aucune sauvegarde sans modification");

        let absent = propager(&d.join("nope.plist"), &d.join("sauv"), Path::new("/tmp/x.png"), &["A".into()]);
        assert!(matches!(absent, Err(ErreurBureaux::MagasinIntrouvable(_))));
    }

    /// Lecture seule du vrai magasin : travaille sur une copie temporaire.
    /// `cargo test --offline --locked --lib copie_du_magasin_reel -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn copie_du_magasin_reel() {
        let reel = chemin_magasin().unwrap();
        let d = temporaire("reel");
        let copie = d.join("Index.plist");
        fs::copy(&reel, &copie).unwrap();
        let cles: Vec<String> = crate::ecrans::lister().into_iter().map(|e| e.cle).collect();
        let cles = if cles.is_empty() {
            vec!["5ABFF110-DD7E-4E0B-B29E-1F0D4C196F2B".into(), "7EB7921B-BEFC-4306-B6BE-341FCE29DC31".into()]
        } else {
            cles
        };
        let r = propager(&copie, &d.join("sauv"), Path::new("/tmp/essai.png"), &cles).unwrap();
        println!("rapport sur copie : {r:?}");
        assert!(r.total() > 0);
        assert!(fs::metadata(&reel).unwrap().len() > 0);
    }

    /// ECRIT dans le vrai magasin (sauvegarde + relance de l'agent). Manuel uniquement :
    /// `ESSAI_IMAGE=<fichier> ESSAI_ECRANS=<uuid,uuid> cargo test ... essai_reel -- --ignored`
    #[test]
    #[ignore]
    fn essai_reel_propagation() {
        let image = PathBuf::from(std::env::var("ESSAI_IMAGE").unwrap());
        let cles: Vec<String> = std::env::var("ESSAI_ECRANS").unwrap().split(',').map(String::from).collect();
        let r = appliquer_partout(&[(image, cles)]).unwrap();
        println!("rapport reel : {r:?}");
    }
}
