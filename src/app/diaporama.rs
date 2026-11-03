//! Diaporama et suivi des demandes de fond (minuteries, relecture, propagation).
use super::*;

/// Le diaporama continue tant qu'au moins un ecran a accepte la demande ;
/// il ne se met en pause que si tous les ecrans du tick ont echoue.
fn diaporama_continue(appliques: usize) -> bool {
    appliques > 0
}

#[cfg(test)]
mod tests_continuite {
    use super::diaporama_continue;

    #[test]
    fn un_ecran_en_echec_n_arrete_pas_les_autres() {
        assert!(diaporama_continue(1));
        assert!(diaporama_continue(2));
        assert!(!diaporama_continue(0), "tous en echec : pause");
    }
}

impl Cible {
    /// Applique un intervalle (menu ou panneau) sans dupliquer la logique.
    pub(super) fn appliquer_intervalle(&self, secondes: u64) {
        {
            let mut etat = self.etat_mut();
            etat.reglages.diaporama_intervalle = secondes.clamp(60, 86400);
            etat.enregistrer();
        }
        if self.etat().diaporama_actif {
            self.programmer_diaporama();
        }
        self.rafraichir();
    }

    /// Coche/decoche un ecran participant au diaporama (cle UUID).
    pub(super) fn basculer_participation(&self, cle: &str) {
        {
            let mut etat = self.etat_mut();
            let etat_brut: &mut Etat = &mut etat;
            let cles = etat_brut
                .reglages
                .diaporama_ecrans
                .get_or_insert_with(|| etat_brut.ecrans.iter().map(|e| e.cle.clone()).collect());
            if cles.iter().any(|c| c == cle) {
                cles.retain(|c| c != cle);
            } else {
                cles.push(cle.to_owned());
            }
            etat.enregistrer();
        }
        self.selection_diaporama_modifiee();
    }

    pub(super) fn arreter_diaporama(&self) {
        if let Some(minuterie) = self.ivars().minuterie.borrow_mut().take() {
            minuterie.invalidate();
        }
        self.etat_mut().diaporama_actif = false;
    }

    /// Enregistre l'intention **avant** l'appel Apple et rend son numero de
    /// sequence. Une nouvelle intention pour cet ecran annule le resultat
    /// precedent dans le bilan du cycle courant : elle remplace l'ancienne,
    /// qu'elle soit acceptee ou non.
    pub(super) fn enregistrer_intention(&self, cle: &str, chemin: &std::path::Path) -> u64 {
        self.ivars().bilan.borrow_mut().oublier(cle);
        self.ivars()
            .suivi
            .borrow_mut()
            .demander(cle, chemin, maintenant_ms())
    }

    /// Consomme le bilan du cycle courant (vide apres lecture).
    pub(super) fn prendre_bilan(&self) -> Option<String> {
        let mut bilan = self.ivars().bilan.borrow_mut();
        let resume = bilan.resume();
        bilan.vider();
        resume
    }

    /// Demarre le relevé periodique (250 ms) s'il ne tourne pas deja.
    ///
    /// Aucune attente ni sommeil : le fil principal rend la main entre deux
    /// relectures, et la minuterie s'arrete des qu'il n'y a plus rien a suivre.
    pub(super) fn assurer_relecture(&self) {
        if self.ivars().relecture.borrow().is_some() {
            return;
        }
        let periode = suivi::PERIODE_RELECTURE_MS as f64 / 1000.0;
        let minuterie = unsafe {
            objc2_foundation::NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                periode,
                self,
                sel!(relireDemandes:),
                None,
                true,
            )
        };
        unsafe {
            objc2_foundation::NSRunLoop::mainRunLoop()
                .addTimer_forMode(&minuterie, objc2_foundation::NSRunLoopCommonModes);
        }
        *self.ivars().relecture.borrow_mut() = Some(minuterie);
    }

    pub(super) fn arreter_relecture(&self) {
        if let Some(minuterie) = self.ivars().relecture.borrow_mut().take() {
            minuterie.invalidate();
        }
    }

    /// Relit les fonds sans bloquer : confirme, expire ou abandonne chaque demande.
    pub(super) fn relire_demandes(&self) {
        let maintenant = maintenant_ms();
        let demandes: Vec<(String, u64, PathBuf)> = self
            .ivars()
            .suivi
            .borrow()
            .demandes()
            .map(|d| (d.cle.clone(), d.sequence, d.chemin.clone()))
            .collect();
        let mut confirmees_tick = 0usize;
        for (cle, sequence, chemin) in demandes {
            let cible = self.etat().ecrans.iter().find(|e| e.cle == cle).cloned();
            let (libelle, ecran) = match cible {
                Some(e) => (format!("Display {}", e.index + 1), ecrans::retrouver_identifiant(e.identifiant)),
                None => (format!("Display {cle}"), None),
            };
            let constat = ecran.as_ref().and_then(|e| fond_ecran::image_courante(e));
            let issue = {
                let mut suivi = self.ivars().suivi.borrow_mut();
                if ecran.is_some() { suivi.relire(&cle, sequence, constat.as_deref(), maintenant) }
                else { suivi.ecran_disparu(&cle, sequence) }
            };
            match issue {
                Issue::Confirmee => {
                    confirmees_tick += 1;
                    self.etat_mut()
                        .reglages
                        .image_par_ecran
                        .insert(cle.clone(), chemin.to_string_lossy().into_owned());
                    self.ivars().bilan.borrow_mut().confirmer(&cle, chemin.clone());
                    journal::info(&format!(
                        "{libelle} : chemin confirme par relecture (rendu non verifie)"
                    ));
                    let mut attente = self.ivars().propagation_attente.borrow_mut();
                    if let Some(i) = attente.iter().position(|(c, p)| *c == cle && *p == chemin) {
                        let prete = attente.remove(i);
                        self.ivars().propagation_pretes.borrow_mut().push(prete);
                    }
                }
                Issue::DelaiDepasse => {
                    self.ivars().propagation_attente.borrow_mut().retain(|(c, _)| *c != cle);
                    journal::erreur(&format!(
                        "{libelle} : changement non confirme apres {} ms",
                        suivi::DELAI_CONFIRMATION_MS
                    ));
                    self.ivars().bilan.borrow_mut().echouer(
                        &cle,
                        format!(
                            "{libelle} not confirmed after {} s",
                            suivi::DELAI_CONFIRMATION_MS / 1000
                        ),
                    );
                }
                Issue::EcranDisparu => {
                    self.ivars().propagation_attente.borrow_mut().retain(|(c, _)| *c != cle);
                    journal::erreur(&format!("{libelle} : deconnecte avant confirmation"));
                    self.ivars()
                        .bilan
                        .borrow_mut()
                        .echouer(&cle, format!("{libelle} disconnected"));
                }
                Issue::EchecApi(message) => {
                    self.ivars().propagation_attente.borrow_mut().retain(|(c, _)| *c != cle);
                    journal::erreur(&format!("{libelle} : {message}"));
                    self.ivars()
                        .bilan
                        .borrow_mut()
                        .echouer(&cle, format!("{libelle} : {message}"));
                }
                _ => {}
            }
        }
        if confirmees_tick > 0 {
            self.etat_mut().enregistrer();
        }
        if self.ivars().suivi.borrow().est_vide() {
            self.arreter_relecture();
            let propagation = self.propager_sur_tous_les_bureaux();
            self.relever_fonds_actifs();
            self.rafraichir();
            let texte = match (self.prendre_bilan(), propagation) {
                (Some(bilan), Some(p)) => Some(format!("{bilan} — {p}")),
                (bilan, p) => bilan.or(p),
            };
            if let Some(texte) = texte {
                self.message(&texte);
            }
        }
    }

    /// Etend aux autres bureaux les fonds **deja confirmes** par macOS (jamais
    /// avant : l'agent doit avoir ecrit le sien dans le magasin). Un seul passage,
    /// une seule relance de `WallpaperAgent`.
    pub(super) fn propager_sur_tous_les_bureaux(&self) -> Option<String> {
        let pretes: Vec<(String, PathBuf)> =
            std::mem::take(&mut *self.ivars().propagation_pretes.borrow_mut());
        self.ivars().propagation_attente.borrow_mut().clear();
        if pretes.is_empty() {
            return None;
        }
        let mut groupes: Vec<(PathBuf, Vec<String>)> = Vec::new();
        for (cle, chemin) in pretes {
            match groupes.iter_mut().find(|(p, _)| *p == chemin) {
                Some((_, cles)) => cles.push(cle),
                None => groupes.push((chemin, vec![cle])),
            }
        }
        match tous_bureaux::appliquer_partout(&groupes) {
            Ok(r) => {
                journal::info(&format!(
                    "tous les bureaux : {} ecran(s) et {} bureau(x) mis a jour",
                    r.ecrans, r.bureaux
                ));
                Some(format!("applied to all desktops ({} updated)", r.total()))
            }
            Err(e) => {
                journal::erreur(&format!("tous les bureaux : {e}"));
                Some(format!("all desktops not updated: {e}"))
            }
        }
    }

    pub(super) fn programmer_diaporama(&self) {
        self.arreter_diaporama();
        let intervalle = self.etat()
            .reglages
            .diaporama_intervalle
            .clamp(60, 86400) as f64;
        let minuterie = unsafe {
            objc2_foundation::NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                intervalle,
                self,
                sel!(suivanteDiaporama:),
                None,
                true,
            )
        };
        minuterie.setTolerance((intervalle * 0.05).min(5.0));
        unsafe {
            objc2_foundation::NSRunLoop::mainRunLoop()
                .addTimer_forMode(&minuterie, objc2_foundation::NSRunLoopCommonModes);
        }
        *self.ivars().minuterie.borrow_mut() = Some(minuterie);
        self.etat_mut().diaporama_actif = true;
    }

    pub(super) fn selection_diaporama_modifiee(&self) {
        let vide = self.etat().plan_diaporama().is_empty();
        if vide && self.etat().diaporama_actif {
            self.arreter_diaporama();
        }
        self.rafraichir_grille();
        self.rafraichir();
    }

    pub(super) fn avancer_diaporama(&self) {
        // Relever les fonds actuels avant de calculer la suite, y compris les
        // changements faits par l’utilisateur dans les reglages macOS.
        let ecrans = self.etat().ecrans.clone();
        let courantes: Vec<_> = ecrans.iter().filter_map(|e| {
            let vivant = ecrans::retrouver_identifiant(e.identifiant)?;
            let chemin = fond_ecran::image_courante(&vivant)?;
            Some((e.cle.clone(), chemin.to_string_lossy().into_owned()))
        }).collect();
        let plan = {
            let mut etat = self.etat_mut();
            etat.reglages.image_par_ecran.extend(courantes);
            etat.plan_diaporama()
        };
        if plan.is_empty() {
            self.arreter_diaporama();
            self.rafraichir();
            self.message("Slideshow stopped: no accessible images or participating displays");
            return;
        }
        let mut appliques = 0;
        let mut erreurs = Vec::new();
        for (cle, chemin) in plan {
            let chemin = PathBuf::from(chemin);
            // L'intention est enregistree avant l'appel : meme si macOS refuse,
            // l'ancienne demande de cet ecran ne peut plus confirmer.
            let sequence = self.enregistrer_intention(&cle, &chemin);
            let cible = ecrans.iter().find(|e| e.cle == cle);
            let libelle = cible
                .map(|e| format!("Display {}", e.index + 1))
                .unwrap_or_else(|| format!("Display {cle}"));
            let Some(ecran) = cible.and_then(|e| ecrans::retrouver_identifiant(e.identifiant)) else {
                self.ivars().suivi.borrow_mut().ecran_disparu(&cle, sequence);
                self.ivars()
                    .bilan
                    .borrow_mut()
                    .echouer(&cle, format!("{libelle} disconnected"));
                erreurs.push(format!("{libelle} disconnected"));
                continue;
            };
            match fond_ecran::appliquer(&chemin, &ecran) {
                Ok(()) => {
                    self.assurer_relecture();
                    appliques += 1;
                }
                Err(erreur) => {
                    self.ivars()
                        .suivi
                        .borrow_mut()
                        .echouer(&cle, sequence, &erreur.to_string());
                    self.ivars()
                        .bilan
                        .borrow_mut()
                        .echouer(&cle, format!("{libelle} : {erreur}"));
                    erreurs.push(format!("{libelle} : {erreur}"));
                }
            }
        }
        self.etat_mut().enregistrer();
        self.relever_fonds_actifs();
        self.rafraichir();
        if erreurs.is_empty() {
            self.message(&format!(
                "Slideshow: request accepted on {appliques} display(s), checking"
            ));
        } else if diaporama_continue(appliques) {
            // Un ecran en echec ne doit pas arreter les autres : le diaporama
            // continue, l'echec est signale et journalise a chaque tick.
            self.message(&format!(
                "Slideshow: request accepted on {appliques} display(s), checking; failed: {}",
                erreurs.join(", ")
            ));
        } else {
            self.arreter_diaporama();
            self.rafraichir();
            let bilan = if self.ivars().suivi.borrow().est_vide() {
                self.arreter_relecture();
                self.prendre_bilan()
            } else {
                None
            };
            let texte = match bilan {
                Some(bilan) => format!(
                    "Slideshow paused: {} — {bilan}",
                    erreurs.join(", ")
                ),
                None => format!("Slideshow paused: {}", erreurs.join(", ")),
            };
            self.message(&texte);
        }
    }
}
