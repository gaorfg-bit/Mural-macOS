//! Suppression definitive d'une image, hors interface (testable sans AppKit).
//!
//! Seule action destructrice de Mural : elle n'est appelee qu'apres la
//! confirmation du menu image. Les caches sont effaces AVANT le fichier, car
//! leurs cles dependent des metadonnees de la source, encore presente ; si la
//! suppression echoue ensuite, les caches se regenerent au besoin.

use std::path::Path;

use crate::preferences::Reglages;

/// Supprime `chemin` du disque, efface ses copies de cache et le retire des
/// reglages (diaporamas, fonds enregistres). Renvoie le nombre de fichiers de
/// cache retires. En cas d'echec, les reglages ne changent pas.
pub fn supprimer_image(
    chemin: &Path,
    reglages: &mut Reglages,
    cache_vignettes: Option<&Path>,
    cache_avif: Option<&Path>,
) -> std::io::Result<usize> {
    let copies = cache_vignettes.map_or(0, |d| crate::cache::effacer_pour_source(d, chemin))
        + cache_avif.map_or(0, |d| crate::avif::effacer_cache_dans(d, chemin));
    std::fs::remove_file(chemin)?;
    let texte = chemin.to_string_lossy();
    reglages.oublier_image(&texte);
    reglages.oublier_fond_supprime(&texte);
    Ok(copies)
}
