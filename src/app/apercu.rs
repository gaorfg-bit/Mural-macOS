//! Apercu principal : identite du rendu, memoire d'un apercu, decodage hors fil principal.
use super::*;

/// Identite du fichier et du rendu, sans lecture des pixels.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct CleApercu {
    pub(super) chemin: PathBuf,
    pub(super) modification: std::time::SystemTime,
    pub(super) taille: u64,
    pub(super) identite_unix: (u64, u64, i64, i64),
    pub(super) cible: (u32, u32),
}

impl CleApercu {
    pub(super) fn lire(chemin: &std::path::Path, largeur: u32, hauteur: u32) -> Option<Self> {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(chemin).ok()?;
        if !meta.is_file() { return None; }
        Some(Self {
            chemin: std::fs::canonicalize(chemin).ok()?, modification: meta.modified().ok()?,
            taille: meta.len(), identite_unix: (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec()),
            cible: (largeur, hauteur),
        })
    }
}

pub(super) struct ApercuMemoire {
    pub(super) cle: CleApercu,
    pub(super) image: Retained<NSImage>,
    pub(super) dimensions: String,
}

/// Resultat d'un decodage d'apercu fait hors du fil principal.
pub(super) struct ResultatApercu {
    pub(super) cle: CleApercu,
    pub(super) vignette: Option<bibliotheque::Vignette>,
    pub(super) dimensions: String,
    pub(super) duree_ms: u128,
}

/// Decode l'apercu et compose la ligne « dimensions · taille » (sans AppKit).
pub(super) fn decoder_apercu(chemin: &std::path::Path, largeur: u32, hauteur: u32) -> (Option<bibliotheque::Vignette>, String) {
    let vignette = bibliotheque::apercu(chemin, largeur, hauteur);
    let dimensions = bibliotheque::dimensions(chemin)
        .map(|(l, h)| format!("{l} x {h} px")).unwrap_or_default();
    let taille = bibliotheque::taille_lisible(std::fs::metadata(chemin).ok().map(|m| m.len()));
    let dimensions = if dimensions.is_empty() { taille } else { format!("{dimensions} · {taille}") };
    (vignette, dimensions)
}

impl Cible {
    /// Un seul apercu conserve : les options ne relancent pas ImageIO.
    ///
    /// Le decodage se fait hors du fil principal (un disque lent ou une tres
    /// grande image bloquait l'interface) ; les outils hors ecran, sans boucle
    /// AppKit, gardent le decodage synchrone.
    pub(super) fn charger_apercu(&self, image: &Image, largeur: u32, hauteur: u32) -> (Option<Retained<NSImage>>, String) {
        let cle = CleApercu::lire(&image.chemin, largeur, hauteur);
        let reutilisable = self.ivars().apercu_memoire.borrow().as_ref()
            .filter(|ancien| Some(&ancien.cle) == cle.as_ref())
            .map(|ancien| (ancien.image.clone(), ancien.dimensions.clone()));
        if let Some((image, dimensions)) = reutilisable { return (Some(image), dimensions); }
        let ancien = self.ivars().apercu_memoire.borrow_mut().take();
        drop(ancien);
        let hors_ecran = self.etat().chargement_integral;
        if let (Some(cle), false) = (cle.clone(), hors_ecran) {
            if self.ivars().apercu_echec.borrow().as_ref() == Some(&cle) {
                return (None, "Preview unavailable".into());
            }
            if self.ivars().apercu_demande.borrow().as_ref() != Some(&cle) {
                let (envoi, reception) = std::sync::mpsc::channel();
                *self.ivars().apercu_reception.borrow_mut() = Some(reception);
                *self.ivars().apercu_demande.borrow_mut() = Some(cle.clone());
                let cible = self as *const Self as usize;
                std::thread::spawn(move || {
                    let debut = std::time::Instant::now();
                    let (vignette, dimensions) = objc2::rc::autoreleasepool(|_| {
                        decoder_apercu(&cle.chemin, cle.cible.0, cle.cible.1)
                    });
                    let resultat = ResultatApercu { cle, vignette, dimensions, duree_ms: debut.elapsed().as_millis() };
                    if envoi.send(resultat).is_ok() {
                        // La cible vit jusqu'a la fin du processus (`mem::forget`).
                        unsafe {
                            let objet = &*(cible as *const NSObject);
                            let _: () = msg_send![objet, performSelectorOnMainThread: sel!(apercuPret:), withObject: std::ptr::null::<NSObject>(), waitUntilDone: false];
                        }
                    }
                });
            }
            return (None, "Loading preview…".into());
        }
        // Aucun cadre de cache possible (fichier illisible) ou outil hors ecran : synchrone.
        let debut = std::time::Instant::now();
        let (vignette, dimensions) = decoder_apercu(&image.chemin, largeur, hauteur);
        let apercu = vignette.map(|v| {
            NSImage::initWithCGImage_size(NSImage::alloc(), &v.image,
                NSSize { width: v.largeur as f64, height: v.hauteur as f64 })
        });
        if apercu.is_none() { journal::erreur(&format!("apercu impossible : {}", image.nom)); }
        journal::info(&format!("apercu de « {} » en {} ms", image.nom, debut.elapsed().as_millis()));
        // Un fichier remplace pendant le decodage ne doit pas alimenter le cache.
        if let (Some(cle), Some(apercu)) = (cle, apercu.as_ref()) {
            if Some(&cle) == CleApercu::lire(&image.chemin, largeur, hauteur).as_ref() {
                *self.ivars().apercu_memoire.borrow_mut() = Some(ApercuMemoire {
                    cle, image: apercu.clone(), dimensions: dimensions.clone(),
                });
            }
        }
        (apercu, dimensions)
    }
}
