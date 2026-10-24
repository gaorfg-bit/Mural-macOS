//! Mesure du cache disque des vignettes : froid puis chaud, meme image, meme
//! largeur. Sortie brute en millisecondes, sans interpretation.
//!
//! Usage : mesure_cache <image> [largeur] [dossier_cache]
//!
//! Sans dossier explicite, un dossier isole est cree sous le repertoire
//! temporaire du systeme. Le dossier fourni doit etre vide : l'outil refuse de
//! mesurer un « froid » sur un cache deja rempli, et ne supprime jamais rien.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use mural::bibliotheque;

fn dossier_isole() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duree| duree.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("mural-mesure-cache-{}-{nanos}", std::process::id()))
}

fn entrees(dossier: &Path) -> usize {
    fs::read_dir(dossier)
        .map(|lecture| lecture.filter_map(|e| e.ok()).count())
        .unwrap_or(0)
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let Some(image) = arguments.next().map(PathBuf::from) else {
        eprintln!("usage : mesure_cache <image> [largeur] [dossier_cache]");
        std::process::exit(2);
    };
    let largeur: usize = arguments
        .next()
        .map(|valeur| valeur.parse().expect("largeur entiere"))
        .unwrap_or(180);
    let cache = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(dossier_isole);

    if !image.is_file() {
        eprintln!("image introuvable : {}", image.display());
        std::process::exit(2);
    }
    if entrees(&cache) > 0 {
        eprintln!(
            "dossier de cache non vide ({}) : mesure froide impossible, rien n'a ete supprime",
            cache.display()
        );
        std::process::exit(2);
    }
    fs::create_dir_all(&cache).expect("creation du dossier de cache");

    println!("IMAGE {}", image.display());
    println!("LARGEUR {largeur}");
    println!("CACHE {}", cache.display());

    let debut = Instant::now();
    let froid =
        bibliotheque::vignette_avec_cache(&image, largeur, Some(&cache)).expect("vignette froide");
    let duree_froide = debut.elapsed();
    println!(
        "FROID {:.3} ms ({}x{})",
        duree_froide.as_secs_f64() * 1000.0,
        froid.largeur,
        froid.hauteur
    );

    for tour in 1..=3 {
        let debut = Instant::now();
        let chaud = bibliotheque::vignette_avec_cache(&image, largeur, Some(&cache))
            .expect("vignette chaude");
        let duree = debut.elapsed();
        assert_eq!(
            (chaud.largeur, chaud.hauteur),
            (froid.largeur, froid.hauteur),
            "froid et chaud doivent produire la meme vignette"
        );
        println!(
            "CHAUD {tour} {:.3} ms ({}x{})",
            duree.as_secs_f64() * 1000.0,
            chaud.largeur,
            chaud.hauteur
        );
    }
    println!("FICHIERS CACHE {}", entrees(&cache));
}
