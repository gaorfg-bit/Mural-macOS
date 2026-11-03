//! Etat partage de l'interface, ses gardes d'emprunt et ses calculs de statut.
use super::*;

/// Etat partage entre Rust et les actions Objective-C.
pub struct Etat {
    /// Les outils hors ecran attendent toutes les images sans boucle AppKit.
    pub(super) chargement_integral: bool,
    pub(super) zone_vignettes: Vec<usize>,
    pub(super) reception: Option<std::sync::mpsc::Receiver<(usize, Option<bibliotheque::Vignette>)>>,
    /// Leve des qu'une nouvelle demande remplace le worker courant : il s'arrete
    /// avant le prochain decodage au lieu de finir toute sa liste.
    pub(super) annulation_worker: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub reglages: crate::preferences::Reglages,
    pub erreur_sauvegarde: Option<String>,
    pub diaporama_actif: bool,
    pub filtre_diaporama: bool,
    pub dossier: PathBuf,
    pub images: Vec<Image>,
    /// Images du dossier hors masquees, avant la limite d'affichage.
    pub total_images: usize,
    pub vignettes: Vec<Option<Retained<NSImage>>>,
    pub(super) vignettes_en_attente: Vec<bool>,
    pub selection: Option<usize>,
    pub ecran_courant: usize,
    pub ecrans: Vec<ecrans::Ecran>,
    pub fonds_actifs: Vec<String>,
    pub tri: Tri,
    /// Texte de recherche (minuscules), chaine vide = pas de filtre.
    pub filtre: String,
    /// Pointeur du bouton de vignette -> index dans `images`.
    pub cellules: HashMap<usize, usize>,
    /// Retient chaque bouton de `cellules` : une adresse de la carte reste donc
    /// valide tant qu'elle y figure, meme si la vue a quitte la hierarchie.
    pub(super) boutons: Vec<Retained<NSButton>>,
    pub largeur: f64,
    pub hauteur: f64,
}

/// Donnees possedees du panneau : aucune reference vers l'etat vivant ne
/// traverse la construction ou l'actualisation des controles AppKit.
#[derive(Clone)]
pub(crate) struct InstantaneReglages {
    pub reglages: crate::preferences::Reglages,
    pub ecrans: Vec<ecrans::Ecran>,
    pub ecran_courant: usize,
    pub dossier: PathBuf,
    pub tri: Tri,
    pub diaporama_actif: bool,
    pub filtre_diaporama: bool,
    pub plan_vide: bool,
}
impl InstantaneReglages {
    pub fn cle_ecran_courant(&self) -> Option<&str> {
        self.ecrans.get(self.ecran_courant).map(|e| e.cle.as_str())
    }
}

impl Etat {
    pub(super) fn instantane_reglages(&self) -> InstantaneReglages {
        InstantaneReglages {
            reglages: self.reglages.clone(), ecrans: self.ecrans.clone(),
            ecran_courant: self.ecran_courant, dossier: self.dossier.clone(), tri: self.tri,
            diaporama_actif: self.diaporama_actif, filtre_diaporama: self.filtre_diaporama,
            plan_vide: self.plan_diaporama().is_empty(),
        }
    }
    pub(super) fn nouveau(dossier: PathBuf, chargement_integral: bool) -> Self {
        // Les anciens identifiants Swift (`nom@x,y`) ne sont remappes que vers
        // les ecrans reellement connectes, avec une correspondance exacte.
        let ecrans = ecrans::lister();
        let (reglages, erreur_chargement) = match crate::preferences::essayer_charger_avec_ecrans(&ecrans) {
            Ok(reglages) => (reglages, None),
            Err(erreur) => {
                let message = format!("Settings could not be read: {erreur}. Original file preserved.");
                journal::erreur(&message);
                (crate::preferences::Reglages::default(), Some(message))
            }
        };
        let mut etat = Etat {
            tri: if reglages.tri == "date" {
                Tri::Date
            } else {
                Tri::Nom
            },
            largeur: taille_valide(reglages.largeur_fenetre, 880.0, 2400.0, 1180.0),
            hauteur: taille_valide(reglages.hauteur_fenetre, 520.0, 1600.0, 760.0),
            reglages,
            erreur_sauvegarde: erreur_chargement,
            diaporama_actif: false,
            filtre_diaporama: false,
            chargement_integral,
            zone_vignettes: Vec::new(),
            reception: None,
            annulation_worker: Default::default(),
            dossier,
            images: Vec::new(),
            total_images: 0,
            vignettes: Vec::new(),
            vignettes_en_attente: Vec::new(),
            selection: None,
            ecran_courant: 0,
            ecrans,
            fonds_actifs: Vec::new(),
            filtre: String::new(),
            cellules: HashMap::new(),
            boutons: Vec::new(),
        };
        etat.ecran_courant = etat
            .ecrans
            .iter()
            .position(|e| Some(&e.cle) == etat.reglages.ecran_selectionne.as_ref())
            .unwrap_or(0);
        for e in &etat.ecrans {
            journal::info(&format!(
                "ecran {} : {} {}x{} a ({}, {}){}",
                e.index + 1,
                e.nom,
                e.largeur,
                e.hauteur,
                e.x as i64,
                e.y as i64,
                if e.principal { " principal" } else { "" }
            ));
        }
        etat.recharger();
        etat.relever_fonds_actifs();
        etat
    }

    pub fn cibles_application(&self) -> Vec<&ecrans::Ecran> {
        self.ecrans
            .iter()
            .filter(|e| self.reglages.meme_image_partout || e.index == self.ecran_courant)
            .collect()
    }

    pub fn cibles_diaporama(&self) -> Vec<&ecrans::Ecran> {
        self.ecrans
            .iter()
            .filter(|e| {
                self.reglages
                    .diaporama_ecrans
                    .as_ref()
                    .map(|cles| cles.contains(&e.cle))
                    .unwrap_or(true)
            })
            .collect()
    }

    /// Cle UUID de l'ecran selectionne dans l'interface.
    pub fn cle_ecran_courant(&self) -> Option<&str> {
        self.ecrans.get(self.ecran_courant).map(|e| e.cle.as_str())
    }

    /// Plan du prochain tick.
    ///
    /// - mode global : liste `favoris` et planificateur historique inchanges ;
    /// - mode independant : chaque ecran ne pioche que dans sa liste dediee ;
    /// - mode « image commune » : une seule liste de reference est choisie
    ///   (ecran selectionne s'il participe et a une liste non vide, sinon
    ///   premier participant eligible), puis la meme image partout.
    pub fn plan_diaporama(&self) -> Vec<(String, String)> {
        let ecrans: Vec<_> = self
            .cibles_diaporama()
            .iter()
            .map(|e| e.cle.clone())
            .collect();
        let graine = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let images_existantes = |liste: &[String]| -> Vec<String> {
            liste
                .iter()
                .filter(|p| std::path::Path::new(p).is_file())
                .cloned()
                .collect()
        };
        if !self.reglages.selection_independante {
            let images = images_existantes(&self.reglages.favoris);
            return crate::diaporama::planifier(
                &images,
                &ecrans,
                &self.reglages.image_par_ecran,
                self.reglages.meme_image_partout,
                self.reglages.diaporama_aleatoire,
                graine,
            );
        }
        let listes: BTreeMap<String, Vec<String>> = ecrans
            .iter()
            .map(|cle| {
                let images = self
                    .reglages
                    .favoris_par_ecran
                    .get(cle)
                    .map(|liste| images_existantes(liste))
                    .unwrap_or_default();
                (cle.clone(), images)
            })
            .collect();
        if self.reglages.meme_image_partout {
            let Some((cle_reference, reference)) = crate::diaporama::liste_reference(
                &listes,
                &ecrans,
                self.cle_ecran_courant(),
            ) else {
                return Vec::new();
            };
            // Le planificateur commun avance depuis sa première cible : placer
            // l'écran de référence en tête, pas seulement récupérer sa liste.
            let mut ordre = ecrans.clone();
            if let Some(position) = ordre.iter().position(|cle| cle.as_str() == cle_reference) {
                ordre.swap(0, position);
            }
            return crate::diaporama::planifier(
                reference,
                &ordre,
                &self.reglages.image_par_ecran,
                true,
                self.reglages.diaporama_aleatoire,
                graine,
            );
        }
        crate::diaporama::planifier_par_ecran(
            &listes,
            &ecrans,
            &self.reglages.image_par_ecran,
            self.reglages.diaporama_aleatoire,
            graine,
        )
    }

    /// Statut sans lecture d’image : partage par la selection et les notifications du worker.
    pub(super) fn statut_bibliotheque(&self) -> String {
        if let Some(erreur) = &self.erreur_sauvegarde { return erreur.clone(); }
        let selection = self.selection.map(|index| format!("Selected: {}", self.images[index].nom))
            .unwrap_or_else(|| "Select an image".into());
        let base = format!("{} · {} slideshow image(s){}{}", selection,
            self.reglages.favoris_selection(self.cle_ecran_courant()).len(),
            if self.diaporama_actif { " · playing" } else { "" },
            texte_limite(self.images.len(), self.total_images));
        let en_attente: Vec<_> = self.zone_vignettes.iter()
            .map(|&i| self.vignettes_en_attente[i]).collect();
        statut_vignettes(&base, &en_attente)
    }

    pub(super) fn enregistrer(&mut self) {
        self.reglages.tri = if self.tri == Tri::Date { "date" } else { "nom" }.into();
        self.reglages.largeur_fenetre = self.largeur;
        self.reglages.hauteur_fenetre = self.hauteur;
        self.reglages.ecran_selectionne =
            self.ecrans.get(self.ecran_courant).map(|e| e.cle.clone());
        self.erreur_sauvegarde = crate::preferences::sauvegarder(&self.reglages)
            .err()
            .map(|e| format!("Settings could not be saved: {e}"));
        if let Some(erreur) = &self.erreur_sauvegarde {
            journal::erreur(erreur);
        }
    }

    /// Relit le dossier courant, regenere les vignettes, selectionne la premiere.
    pub fn recharger(&mut self) {
        let reglages = &self.reglages;
        match bibliotheque::lister_sauf(&self.dossier, self.tri, |chemin| {
            reglages.image_masquee(&chemin.to_string_lossy())
        }) {
            Ok(liste) => {
                journal::info(&format!(
                    "dossier « {} » : {} image(s) affichee(s) sur {}",
                    self.dossier.display(),
                    liste.images.len(),
                    liste.total
                ));
                self.total_images = liste.total;
                self.images = liste.images;
            }
            Err(erreur) => {
                journal::erreur(&format!(
                    "lecture du dossier « {} » impossible : {erreur}",
                    self.dossier.display()
                ));
                self.total_images = 0;
                self.images = Vec::new();
            }
        }
        self.charger_vignettes();
        self.selection = if self.images.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    pub(super) fn charger_vignettes(&mut self) {
        // Detruire la reception annule le worker precedent, y compris un envoi bloque.
        self.reception = None;
        self.annulation_worker.store(true, std::sync::atomic::Ordering::Release);
        self.zone_vignettes.clear();
        self.vignettes = vec![None; self.images.len()];
        self.vignettes_en_attente = vec![false; self.images.len()];
        if self.chargement_integral {
            // Outils hors ecran : pas de boucle AppKit a notifier, la reception est lue directement.
            self.demander_vignettes((0..self.images.len()).collect(), None);
        }
    }

    /// `cible` : adresse de la `Cible` a notifier sur le fil principal (vit jusqu'a la fin du processus).
    pub(super) fn demander_vignettes(&mut self, indices: Vec<usize>, cible: Option<usize>) {
        if indices == self.zone_vignettes { return; }
        self.reception = None;
        self.annulation_worker.store(true, std::sync::atomic::Ordering::Release);
        let arret = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.annulation_worker = std::sync::Arc::clone(&arret);
        self.zone_vignettes = indices;
        for index in 0..self.images.len() {
            if !self.zone_vignettes.contains(&index) {
                self.vignettes[index] = None;
            }
            self.vignettes_en_attente[index] = false;
        }
        let chemins: Vec<_> = self.zone_vignettes.iter().copied()
            .filter(|&i| self.vignettes[i].is_none())
            .map(|i| { self.vignettes_en_attente[i] = true; (i, self.images[i].chemin.clone()) })
            .collect();
        // Pas de file illimitee de pixels si le fil principal est occupe.
        let (envoi, reception) = std::sync::mpsc::sync_channel(2);
        self.reception = Some(reception);
        std::thread::spawn(move || {
            for (index, chemin) in chemins {
                if arret.load(std::sync::atomic::Ordering::Acquire) { break; }
                let vignette = objc2::rc::autoreleasepool(|_| {
                    bibliotheque::vignette(&chemin, LARGEUR_VIGNETTE as usize)
                });
                if envoi.send((index, vignette)).is_err() { break; }
                if let Some(cible) = cible {
                    unsafe {
                        let objet = &*(cible as *const NSObject);
                        let _: () = msg_send![objet, performSelectorOnMainThread: sel!(vignettesPretes:), withObject: std::ptr::null::<NSObject>(), waitUntilDone: false];
                    }
                }
            }
        });
    }

    /// Images qui sont actuellement le fond d'un ecran (lecture seule).
    pub(super) fn relever_fonds_actifs(&mut self) {
        let mut actifs = Vec::new();
        for e in &self.ecrans {
            if let Some(ecran) = ecrans::retrouver_identifiant(e.identifiant)
                && let Some(chemin) = fond_ecran::image_courante(&ecran)
            {
                journal::info(&format!(
                    "fond actuel ecran {} : {}",
                    e.index + 1,
                    chemin.display()
                ));
                actifs.push(chemin.to_string_lossy().to_string());
            }
        }
        self.fonds_actifs = actifs;
    }

    pub fn est_fond_actif(&self, index: usize) -> bool {
        self.images
            .get(index)
            .map(|i| {
                self.fonds_actifs
                    .contains(&i.chemin.to_string_lossy().to_string())
            })
            .unwrap_or(false)
    }

    /// Indices des images qui passent le filtre de recherche.
    pub fn indices_affiches(&self) -> Vec<usize> {
        let selection = self
            .reglages
            .favoris_selection(self.cle_ecran_courant());
        (0..self.images.len())
            .filter(|i| {
                (self.filtre.is_empty()
                    || self.images[*i].nom.to_lowercase().contains(&self.filtre))
                    && (!self.filtre_diaporama
                        || selection.contains(&self.images[*i].chemin.to_string_lossy().into_owned()))
            })
            .collect()
    }
}

/// Lecture de l'etat partage, suivie : un chevauchement avec une ecriture est
/// signale (panic en debug, journal en release) au lieu d'etre un UB silencieux.
pub(super) struct EtatRef<'a> {
    etat: &'a Etat,
    emprunts: &'a Cell<isize>,
    suivi: bool,
}
impl std::ops::Deref for EtatRef<'_> {
    type Target = Etat;
    fn deref(&self) -> &Etat {
        self.etat
    }
}
impl Drop for EtatRef<'_> {
    fn drop(&mut self) {
        if self.suivi {
            self.emprunts.set(self.emprunts.get() - 1);
        }
    }
}

/// Ecriture exclusive de l'etat partage (voir `EtatRef`).
pub(super) struct EtatMut<'a> {
    etat: &'a mut Etat,
    emprunts: &'a Cell<isize>,
    suivi: bool,
}
impl std::ops::Deref for EtatMut<'_> {
    type Target = Etat;
    fn deref(&self) -> &Etat {
        self.etat
    }
}
impl std::ops::DerefMut for EtatMut<'_> {
    fn deref_mut(&mut self) -> &mut Etat {
        self.etat
    }
}
impl Drop for EtatMut<'_> {
    fn drop(&mut self) {
        if self.suivi {
            self.emprunts.set(0);
        }
    }
}

/// Chevauchement d'emprunts : un bug de reentrance, jamais une situation normale.
fn conflit_emprunt(nature: &str) -> bool {
    journal::erreur(&format!("ERREUR DE REENTRANCE : {nature} de l'etat partage"));
    debug_assert!(false, "reentrance : {nature} de l'etat partage");
    false
}

pub(super) fn statut_vignettes(base: &str, en_attente: &[bool]) -> String {
    let restantes = en_attente.iter().filter(|&&attente| attente).count();
    if restantes == 0 { base.to_owned() } else {
        format!("Loading thumbnails {}/{}", en_attente.len() - restantes, en_attente.len())
    }
}

#[cfg(test)]
mod tests_progression {
    use super::statut_vignettes;

    #[test]
    fn progression_compte_les_resultats_meme_illisibles() {
        assert_eq!(statut_vignettes("Selected: photo · 2 slideshow image(s)", &[true, true, true]), "Loading thumbnails 0/3");
        assert_eq!(statut_vignettes("Selected: photo · 2 slideshow image(s)", &[false, true, false]), "Loading thumbnails 2/3");
    }

    #[test]
    fn termine_ou_vide_retablit_le_statut() {
        assert_eq!(statut_vignettes("Selected: photo · 2 slideshow image(s) · playing", &[false, false]), "Selected: photo · 2 slideshow image(s) · playing");
        assert_eq!(statut_vignettes("Select an image · 0 slideshow image(s)", &[]), "Select an image · 0 slideshow image(s)");
    }
}

/// Suffixe de statut quand le dossier depasse la limite d'affichage.
fn texte_limite(affichees: usize, total: usize) -> String {
    if total > affichees {
        format!(" · showing {affichees} of {total} images")
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests_limite {
    use super::texte_limite;

    #[test]
    fn limite_signalee_seulement_si_des_images_sont_cachees() {
        assert_eq!(texte_limite(500, 500), "");
        assert_eq!(texte_limite(12, 12), "");
        assert_eq!(texte_limite(500, 731), " · showing 500 of 731 images");
    }
}

impl Cible {
    /// Etat partage en lecture. L'etat est alloue par `construire` et vit jusqu'a
    /// la fin du processus ; les emprunts sont comptes pour reperer les chevauchements.
    pub(super) fn etat(&self) -> EtatRef<'_> {
        let ivars = self.ivars();
        let pointeur = ivars.etat.get();
        debug_assert!(!pointeur.is_null(), "etat non branche");
        let suivi = if ivars.emprunts.get() < 0 {
            conflit_emprunt("lecture pendant une ecriture")
        } else {
            ivars.emprunts.set(ivars.emprunts.get() + 1);
            true
        };
        EtatRef { etat: unsafe { &*pointeur }, emprunts: &ivars.emprunts, suivi }
    }

    /// Etat partage en ecriture exclusive (voir `etat`).
    pub(super) fn etat_mut(&self) -> EtatMut<'_> {
        let ivars = self.ivars();
        let pointeur = ivars.etat.get();
        debug_assert!(!pointeur.is_null(), "etat non branche");
        let suivi = if ivars.emprunts.get() != 0 {
            conflit_emprunt("ecriture pendant un autre emprunt")
        } else {
            ivars.emprunts.set(-1);
            true
        };
        EtatMut { etat: unsafe { &mut *pointeur }, emprunts: &ivars.emprunts, suivi }
    }
}
