//! Les sauvegardes ne doivent jamais effacer une configuration illisible.
use mural::preferences::{self, Reglages};
use std::{fs, path::PathBuf};
fn chemin(nom: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dossier = PathBuf::from(format!(
        "/private/tmp/mural-preferences-{nom}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dossier).unwrap();
    dossier.join("reglages.json")
}
#[test]
fn json_corrompu_ne_peut_pas_etre_remplace_par_les_valeurs_par_defaut() {
    let p = chemin("corrompu");
    let original = b"{\"favoris\": [\"photo.png\"],";
    fs::write(&p, original).unwrap();
    assert_eq!(
        preferences::essayer_charger_depuis(&p).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    let r = preferences::charger_depuis(&p);
    assert!(
        preferences::sauvegarder_vers(&p, &r).is_err(),
        "JSON corrompu ecrase"
    );
    assert_eq!(fs::read(&p).unwrap(), original);
}
#[test]
fn fichier_non_utf8_reste_intact() {
    let p = chemin("octets");
    fs::write(&p, [0xff, 0x00, 0xfe]).unwrap();
    assert!(preferences::sauvegarder_vers(&p, &Reglages::default()).is_err());
    assert_eq!(fs::read(&p).unwrap(), [0xff, 0x00, 0xfe]);
}
#[test]
fn json_sans_objet_reste_intact() {
    for original in ["null", "[]", "42"] {
        let p = chemin("schema");
        fs::write(&p, original).unwrap();
        assert!(preferences::sauvegarder_vers(&p, &Reglages::default()).is_err());
        assert_eq!(fs::read_to_string(&p).unwrap(), original);
    }
}
#[test]
fn lecture_impossible_n_est_pas_traitee_comme_une_absence() {
    let p = chemin("illisible");
    fs::create_dir(&p).unwrap();
    assert!(preferences::sauvegarder_vers(&p, &Reglages::default()).is_err());
    assert!(p.is_dir());
    assert!(preferences::essayer_charger_depuis(&p).is_err());
}
#[test]
fn absence_et_fichier_valide_autorisent_la_sauvegarde_atomique() {
    let p = chemin("normal");
    let mut r =
        preferences::essayer_charger_depuis(&p).expect("absence distincte d une erreur de lecture");
    r.diaporama_intervalle = 900;
    preferences::sauvegarder_vers(&p, &r).unwrap();
    assert_eq!(preferences::charger_depuis(&p).diaporama_intervalle, 900);
    r.diaporama_intervalle = 1800;
    preferences::sauvegarder_vers(&p, &r).unwrap();
    assert_eq!(preferences::charger_depuis(&p).diaporama_intervalle, 1800);
}
