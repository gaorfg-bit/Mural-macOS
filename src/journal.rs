//! Journal d'execution de Mural — pour lire ce qui deconne sans terminal.
//!
//! Ecrit dans `~/Library/Logs/Mural/mural.log`, une ligne par evenement,
//! horodatee et videe immediatement sur le disque (pas de tampon). Le meme
//! texte part sur stderr quand un terminal est present.
//!
//! L'horodatage vient de Foundation (heure locale reelle) ; au-dela de 2 Mo le
//! journal est archive en `mural.log.1` (une seule archive conservee).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static VERROU: Mutex<()> = Mutex::new(());

/// Emplacement du journal.
pub fn chemin() -> Option<PathBuf> {
    if let Some(chemin) = std::env::var_os("MURAL_JOURNAL").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(chemin));
    }
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
        faire_tourner(&fichier);
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

/// Horodatage local (fuseau et heure d'ete du systeme) `AAAA-MM-JJ HH:MM:SS`.
fn horodatage() -> String {
    use objc2_foundation::{NSDate, NSDateFormatter, NSLocale, NSString};
    let formatteur = NSDateFormatter::new();
    // Locale fixe : le format ne depend ni de la langue ni du reglage 12/24 h.
    formatteur.setLocale(Some(&NSLocale::localeWithLocaleIdentifier(&NSString::from_str(
        "en_US_POSIX",
    ))));
    formatteur.setDateFormat(Some(&NSString::from_str("yyyy-MM-dd HH:mm:ss")));
    formatteur.stringFromDate(&NSDate::now()).to_string()
}

/// Taille au-dela de laquelle le journal passe en `.1` (une seule archive).
const TAILLE_MAX: u64 = 2 * 1024 * 1024;

fn faire_tourner(fichier: &std::path::Path) {
    if fs::metadata(fichier).is_ok_and(|m| m.len() > TAILLE_MAX) {
        let _ = fs::rename(fichier, fichier.with_extension("log.1"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
