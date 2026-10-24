//! Sonde — preuve executable du backend Mural (aucune interface graphique).
//!
//! Mesure, dans l'ordre : les ecrans connectes, l'image de fond actuelle de
//! chaque ecran (lecture seule), l'analyse d'un dossier, une vignette de 180
//! points et les dimensions de la premiere image. Sortie ASCII sur la sortie
//! standard. Le binaire rend la main tout seul.
//!
//! Usage : sonde [dossier]

use std::path::PathBuf;
use std::time::Instant;

use mural::bibliotheque::{self, Tri};
use mural::{ecrans, fond_ecran};

fn dossier_images_utilisateur() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("Pictures")
}

fn main() {
    // --- 1. Capacites ImageIO (lecture seule, aucune conversion) ---
    println!(
        "AVIF LECTURE : {}",
        if mural::formats::avif_lisible() {
            "OUI"
        } else {
            "NON"
        }
    );
    println!(
        "AVIF ENCODAGE : {}",
        if mural::formats::avif_encodable() {
            "OUI"
        } else {
            "NON"
        }
    );

    // --- 2. Ecrans ---
    let liste = ecrans::lister();
    println!("ECRANS : {}", liste.len());
    for e in &liste {
        println!(
            "  Ecran {}: {} {}x{} a ({}, {}){}",
            e.index + 1,
            e.nom,
            e.largeur,
            e.hauteur,
            e.x as i64,
            e.y as i64,
            if e.principal { " (principal)" } else { "" }
        );
    }

    // --- 3. Fond d'ecran actuel (lecture seule) ---
    for e in &liste {
        match ecrans::retrouver(e.index) {
            Some(ecran) => match fond_ecran::image_courante(&ecran) {
                Some(chemin) => println!("FOND ecran {}: {}", e.index + 1, chemin.display()),
                None => println!("FOND ecran {}: (aucun)", e.index + 1),
            },
            None => println!("FOND ecran {}: (ecran disparu)", e.index + 1),
        }
    }

    // --- 4. Analyse du dossier ---
    let dossier = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(dossier_images_utilisateur);

    let debut = Instant::now();
    let images = match bibliotheque::lister(&dossier, Tri::Date) {
        Ok(v) => v,
        Err(e) => {
            println!("DOSSIER : {} (erreur : {})", dossier.display(), e);
            println!("RESUME ecrans={} images=0 vignette=ECHEC", liste.len());
            return;
        }
    };
    let duree_analyse = debut.elapsed().as_millis();
    println!("DOSSIER : {}", dossier.display());
    println!("IMAGES : {}", images.len());
    println!("TEMPS ANALYSE : {} ms", duree_analyse);

    // --- 5. Vignette de la premiere image ---
    let mut vignette_ok = false;
    if let Some(premiere) = images.first() {
        let debut = Instant::now();
        match bibliotheque::vignette(&premiere.chemin, 180) {
            Some(vignette) => {
                let duree = debut.elapsed().as_millis();
                println!(
                    "VIGNETTE 180 : {}x{} pixels en {} ms ({})",
                    vignette.largeur, vignette.hauteur, duree, premiere.nom
                );
                vignette_ok = true;
            }
            None => println!("VIGNETTE 180 : ECHEC sur {}", premiere.nom),
        }

        // --- 6. Dimensions ---
        let debut = Instant::now();
        match bibliotheque::dimensions(&premiere.chemin) {
            Some((l, h)) => println!(
                "DIMENSIONS : {}x{} en {} ms",
                l,
                h,
                debut.elapsed().as_millis()
            ),
            None => println!("DIMENSIONS : ECHEC sur {}", premiere.nom),
        }

        // --- 7. Apercu panoramique ---
        let debut = Instant::now();
        match bibliotheque::apercu(&premiere.chemin, 1600, 780) {
            Some(a) => println!(
                "APERCU 1600x780 : {}x{} pixels en {} ms",
                a.largeur,
                a.hauteur,
                debut.elapsed().as_millis()
            ),
            None => println!("APERCU 1600x780 : ECHEC"),
        }

        // --- 8. Lot de vignettes : ce que coute le remplissage de la grille ---
        let lot: Vec<_> = images.iter().take(30).collect();
        let debut = Instant::now();
        let mut faites = 0usize;
        for img in &lot {
            if bibliotheque::vignette(&img.chemin, 180).is_some() {
                faites += 1;
            }
        }
        let total_lot = debut.elapsed().as_millis();
        if faites > 0 {
            println!(
                "LOT {} vignettes : {} ms au total, {} ms de moyenne",
                faites,
                total_lot,
                total_lot / faites as u128
            );
        }
    }

    println!(
        "RESUME ecrans={} images={} vignette={}",
        liste.len(),
        images.len(),
        if vignette_ok { "OK" } else { "ECHEC" }
    );
}
