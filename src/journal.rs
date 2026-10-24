//! Journal d'execution de Mural — pour lire ce qui deconne sans terminal.
//!
//! Ecrit dans `~/Library/Logs/Mural/mural.log`, une ligne par evenement,
//! horodatee et videe immediatement sur le disque (pas de tampon). Le meme
//! texte part sur stderr quand un terminal est present.
//!
//! Aucune dependance externe : l'horodatage est calcule a la main.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static VERROU: Mutex<()> = Mutex::new(());

/// Emplacement du journal.
pub fn chemin() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Logs")
            .join("Mural")
            .join("mural.log"),
    )
}

/// Ecrit une ligne dans le journal (et sur stderr).
pub fn ecrire(niveau: &str, message: &str) {
    let ligne = format!("{} [{}] {}\n", horodatage(), niveau, message);
    let _garde = VERROU.lock();
    if let Some(fichier) = chemin() {
        if let Some(parent) = fichier.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&fichier) {
            let _ = f.write_all(ligne.as_bytes());
            let _ = f.flush();
        }
    }
    eprint!("{ligne}");
}

/// Evenement normal.
pub fn info(message: &str) {
    ecrire("info", message);
}

/// Probleme a lire dans le journal pour comprendre.
pub fn erreur(message: &str) {
    ecrire("ERREUR", message);
}

/// Ligne de separation, en tete de chaque demarrage.
pub fn demarrage() {
    ecrire(
        "info",
        "================ demarrage de Mural ================",
    );
}

/// Horodatage local au format `AAAA-MM-JJ HH:MM:SS`.
///
/// Sans dependance : jours depuis l'epoque convertis en date civile
/// (algorithme de Howard Hinnant), puis heure locale approchee par le
/// decalage du fuseau s'il est lisible dans l'environnement.
fn horodatage() -> String {
    let Some(maintenant) = SystemTime::now().duration_since(UNIX_EPOCH).ok() else {
        return "date inconnue".to_string();
    };
    let secondes = maintenant.as_secs() as i64 + decalage_local();
    let jours = secondes.div_euclid(86_400);
    let reste = secondes.rem_euclid(86_400);
    let (annee, mois, jour) = date_civile(jours);
    format!(
        "{annee:04}-{mois:02}-{jour:02} {:02}:{:02}:{:02}",
        reste / 3600,
        (reste % 3600) / 60,
        reste % 60
    )
}

/// Decalage horaire local en secondes, s'il est lisible dans l'environnement.
fn decalage_local() -> i64 {
    let Ok(valeur) = std::env::var("MURAL_FUSEAU") else {
        return 0;
    };
    valeur.parse::<i64>().unwrap_or(0) * 3600
}

/// Convertit un nombre de jours depuis l'epoque en (annee, mois, jour).
fn date_civile(jours: i64) -> (i64, u32, u32) {
    let z = jours + 719_468;
    let ere = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let annee_ere = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let jour_annee = doe - (365 * annee_ere + annee_ere / 4 - annee_ere / 100);
    let mp = (5 * jour_annee + 2) / 153;
    let jour = (jour_annee - (153 * mp + 2) / 5 + 1) as u32;
    let mois = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let annee = annee_ere + ere * 400 + if mois <= 2 { 1 } else { 0 };
    (annee, mois, jour)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_civile_connue() {
        // 2026-10-22 = 20748 jours depuis le 1er janvier 1970.
        assert_eq!(date_civile(20_748), (2026, 10, 22));
        assert_eq!(date_civile(0), (1970, 1, 1));
    }

    #[test]
    fn horodatage_a_la_bonne_forme() {
        let texte = horodatage();
        assert_eq!(
            texte.len(),
            19,
            "format attendu AAAA-MM-JJ HH:MM:SS : {texte}"
        );
    }
}
