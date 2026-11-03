//! Verifie en LECTURE SEULE que le vrai magasin de fonds macOS a la forme attendue.
//!
//! Le fichier reel n'est jamais ecrit : on en travaille une copie dans `/private/tmp`
//! avec `tous_bureaux::propager`, et on controle que l'original est inchange (empreinte).
use std::fs;
use std::path::PathBuf;

fn main() {
    let reel = mural::tous_bureaux::chemin_magasin().expect("HOME absent");
    let avant = fs::read(&reel).expect("magasin illisible");
    let base = PathBuf::from(format!("/private/tmp/mural-verif-magasin-{}", std::process::id()));
    fs::create_dir_all(&base).unwrap();
    let copie = base.join("Index.plist");
    fs::write(&copie, &avant).unwrap();
    let cles: Vec<String> = mural::ecrans::lister().into_iter().map(|e| e.cle).collect();
    println!("ecrans connectes : {}", cles.len());
    let image = base.join("sonde.png");
    match mural::tous_bureaux::propager(&copie, &base.join("sauvegardes"), &image, &cles) {
        Ok(r) => println!(
            "OK : structure reconnue sur la copie ; ecrans modifies {}, bureaux modifies {} (rien d'ecrit dans le vrai magasin)",
            r.ecrans, r.bureaux
        ),
        Err(e) => println!("ECHEC : {e}"),
    }
    let apres = fs::read(&reel).unwrap();
    assert_eq!(avant, apres, "le vrai magasin a change pendant la verification");
    println!("OK : vrai magasin inchange ({} octets)", apres.len());
    let _ = fs::remove_dir_all(&base);
}
