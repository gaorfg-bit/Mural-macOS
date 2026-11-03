//! Suppression d'image : fichiers synthetiques dans un dossier temporaire,
//! aucun fond applique, aucun reglage reel touche.

use std::fs;
use std::path::PathBuf;

use mural::preferences::Reglages;
use mural::suppression::supprimer_image;

fn dossier(nom: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("mural-suppression-{nom}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(base.join("cache")).unwrap();
    base
}

/// Fausse entree de cache (`.ident` + `.png`) rattachee a `source`.
fn faux_cache(cache: &std::path::Path, nom: &str, source: &std::path::Path) {
    let canonique = fs::canonicalize(source).unwrap();
    let texte = format!("mural-vignette\0{}\0{nom}", canonique.display());
    fs::write(cache.join(format!("{nom}.ident")), texte).unwrap();
    fs::write(cache.join(format!("{nom}.png")), b"png").unwrap();
}

#[test]
fn supprime_le_fichier_son_cache_et_ses_traces_dans_les_reglages() {
    let base = dossier("complet");
    let cache = base.join("cache");
    let a = base.join("a.png");
    let b = base.join("b.png");
    fs::write(&a, b"x").unwrap();
    fs::write(&b, b"x").unwrap();
    faux_cache(&cache, "1", &a);
    faux_cache(&cache, "2", &b);

    let (ta, tb) = (a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned());
    let mut reglages = Reglages::default();
    reglages.favoris = vec![ta.clone(), tb.clone()];
    reglages.favoris_par_ecran.insert("uuid".into(), vec![ta.clone()]);
    reglages.image_par_ecran.insert("uuid".into(), ta.clone());
    reglages.image_par_ecran.insert("autre".into(), tb.clone());

    let copies = supprimer_image(&a, &mut reglages, Some(&cache), None).unwrap();

    assert_eq!(copies, 2, "un .ident et un .png retires");
    assert!(!a.exists(), "le fichier est supprime");
    assert!(b.exists(), "l'autre image n'est pas touchee");
    assert!(cache.join("2.ident").exists() && cache.join("2.png").exists());
    assert_eq!(reglages.favoris, vec![tb.clone()]);
    assert!(reglages.favoris_par_ecran["uuid"].is_empty());
    assert!(!reglages.image_par_ecran.contains_key("uuid"));
    assert_eq!(reglages.image_par_ecran["autre"], tb);
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn echec_de_suppression_ne_change_pas_les_reglages() {
    let base = dossier("echec");
    let absente = base.join("absente.png");
    let texte = absente.to_string_lossy().into_owned();
    let mut reglages = Reglages::default();
    reglages.favoris = vec![texte.clone()];
    reglages.image_par_ecran.insert("uuid".into(), texte.clone());

    assert!(supprimer_image(&absente, &mut reglages, Some(&base.join("cache")), None).is_err());

    assert_eq!(reglages.favoris, vec![texte.clone()]);
    assert_eq!(reglages.image_par_ecran["uuid"], texte);
    let _ = fs::remove_dir_all(&base);
}
