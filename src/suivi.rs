//! Suivi des demandes de changement de fond : machine a etats pure, sans AppKit.
//!
//! `NSWorkspace` peut accepter une demande alors que la relecture immediate
//! renvoie encore l'ancien fond : le retour de l'API ne prouve donc rien. Ce
//! module distingue trois etats observables — demande acceptee en attente,
//! confirmation par relecture, echec ou absence de confirmation dans un delai
//! borne — et garantit qu'une confirmation tardive n'ecrase jamais une demande
//! plus recente (numeros de sequence).
//!
//! Limite assumee : la confirmation porte sur le **chemin** relu, jamais sur le
//! rendu visuel.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Delai maximal entre l'acceptation d'une demande et sa confirmation.
pub const DELAI_CONFIRMATION_MS: u64 = 3_000;
/// Periode de relecture cote application (non bloquante, via NSTimer).
pub const PERIODE_RELECTURE_MS: u64 = 250;

/// Une demande acceptee par macOS, en attente de confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Demande {
    /// Cle d'ecran (UUID ColorSync), comme dans `image_par_ecran`.
    pub cle: String,
    /// Chemin demande.
    pub chemin: PathBuf,
    /// Numero de sequence : toute reponse portant un autre numero est perimee.
    pub sequence: u64,
    /// Instant logique (ms) de l'acceptation.
    pub depuis_ms: u64,
}

/// Resultat d'une relecture ou d'un evenement sur une demande.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
    /// L'ecran n'existe plus (deconnecte entre la demande et la relecture).
    EcranDisparu,
    /// Reponse ignoree : la demande a ete remplacee entre-temps.
    Perimee,
    /// Relecture encore sur l'ancien fond, delai non atteint.
    EnAttente,
    /// Le chemin demande est celui relu : confirmation du chemin, pas du rendu.
    Confirmee,
    /// Aucune confirmation dans le delai borne.
    DelaiDepasse,
    /// L'appel Apple a explicitement echoue.
    EchecApi(String),
}

/// Resultat retenu pour un ecran dans le bilan d'un cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResultatEcran {
    Confirmee(PathBuf),
    Echec(String),
}

/// Bilan d'un cycle de demandes, accumule entre les ticks de relecture.
///
/// Un seul resultat par ecran : une nouvelle intention pour cet ecran remplace
/// le resultat precedent (`oublier`), et le dernier resultat connu l'emporte.
/// Le bilan se consomme (`resume` puis `vider`) a la fin du cycle.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Bilan {
    resultats: Vec<(String, ResultatEcran)>,
}

impl Bilan {
    pub fn new() -> Self {
        Self::default()
    }

    /// Consigne une confirmation pour cet ecran (remplace tout resultat anterieur).
    pub fn confirmer(&mut self, cle: &str, chemin: impl Into<PathBuf>) {
        self.poser(cle, ResultatEcran::Confirmee(chemin.into()));
    }

    /// Consigne un echec pour cet ecran (remplace tout resultat anterieur).
    pub fn echouer(&mut self, cle: &str, message: impl Into<String>) {
        self.poser(cle, ResultatEcran::Echec(message.into()));
    }

    /// Oublie le resultat de cet ecran : une nouvelle intention le remplace.
    pub fn oublier(&mut self, cle: &str) {
        self.resultats.retain(|(c, _)| c != cle);
    }

    fn poser(&mut self, cle: &str, resultat: ResultatEcran) {
        match self.resultats.iter_mut().find(|(c, _)| c == cle) {
            Some((_, existant)) => *existant = resultat,
            None => self.resultats.push((cle.to_string(), resultat)),
        }
    }

    /// Nombre d'ecrans confirmes dans ce cycle.
    pub fn confirmees(&self) -> usize {
        self.resultats
            .iter()
            .filter(|(_, r)| matches!(r, ResultatEcran::Confirmee(_)))
            .count()
    }

    /// Messages d'echec, dans l'ordre d'arrivee des ecrans.
    pub fn echecs(&self) -> impl Iterator<Item = &str> {
        self.resultats.iter().filter_map(|(_, r)| match r {
            ResultatEcran::Echec(message) => Some(message.as_str()),
            ResultatEcran::Confirmee(_) => None,
        })
    }

    pub fn est_vide(&self) -> bool {
        self.resultats.is_empty()
    }

    /// Texte du bilan final, ou `None` si aucun resultat n'est consigne.
    pub fn resume(&self) -> Option<String> {
        let confirmees = self.confirmees();
        let echecs: Vec<&str> = self.echecs().collect();
        match (confirmees, echecs.is_empty()) {
            (0, true) => None,
            (0, false) => Some(format!("Change not confirmed: {}", echecs.join(", "))),
            (n, true) => Some(format!(
                "Wallpaper path confirmed on {n} display(s) — visual result not verified"
            )),
            (n, false) => Some(format!(
                "Change not confirmed: {} — wallpaper path confirmed on {n} display(s)",
                echecs.join(", ")
            )),
        }
    }

    /// Vide le bilan apres emission : le cycle suivant repart de zero.
    pub fn vider(&mut self) {
        self.resultats.clear();
    }
}

/// Etat du suivi : au plus une demande en attente par ecran.
#[derive(Debug, Default)]
pub struct Suivi {
    sequence: u64,
    en_attente: BTreeMap<String, Demande>,
    confirmees: BTreeMap<String, PathBuf>,
}

impl Suivi {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enregistre une demande acceptee. Une demande anterieure pour le meme
    /// ecran est remplacee ; sa confirmation eventuelle sera ignoree.
    pub fn demander(&mut self, cle: &str, chemin: impl Into<PathBuf>, maintenant_ms: u64) -> u64 {
        self.sequence += 1;
        let sequence = self.sequence;
        self.en_attente.insert(
            cle.to_string(),
            Demande {
                cle: cle.to_string(),
                chemin: chemin.into(),
                sequence,
                depuis_ms: maintenant_ms,
            },
        );
        sequence
    }

    /// Confronte une relecture a la demande en attente pour cet ecran.
    pub fn relire(
        &mut self,
        cle: &str,
        sequence: u64,
        constat: Option<&Path>,
        maintenant_ms: u64,
    ) -> Issue {
        let Some(demande) = self.en_attente.get(cle) else {
            return Issue::Perimee;
        };
        if demande.sequence != sequence {
            return Issue::Perimee;
        }
        if constat.is_some_and(|c| meme_chemin(c, &demande.chemin)) {
            let demande = self
                .en_attente
                .remove(cle)
                .expect("une demande vient d'etre lue");
            self.confirmees.insert(cle.to_string(), demande.chemin);
            return Issue::Confirmee;
        }
        if maintenant_ms.saturating_sub(demande.depuis_ms) >= DELAI_CONFIRMATION_MS {
            self.en_attente.remove(cle);
            return Issue::DelaiDepasse;
        }
        Issue::EnAttente
    }

    /// L'ecran a disparu : la demande est abandonnee.
    pub fn ecran_disparu(&mut self, cle: &str, sequence: u64) -> Issue {
        match self.en_attente.get(cle) {
            Some(demande) if demande.sequence == sequence => {
                self.en_attente.remove(cle);
                Issue::EcranDisparu
            }
            _ => Issue::Perimee,
        }
    }

    /// L'appel Apple a echoue : aucune confirmation n'est attendue.
    pub fn echouer(&mut self, cle: &str, sequence: u64, message: &str) -> Issue {
        match self.en_attente.get(cle) {
            Some(demande) if demande.sequence == sequence => {
                self.en_attente.remove(cle);
                Issue::EchecApi(message.to_string())
            }
            _ => Issue::Perimee,
        }
    }

    /// Demande encore en attente pour cet ecran.
    pub fn en_attente(&self, cle: &str) -> Option<&Demande> {
        self.en_attente.get(cle)
    }

    /// Toutes les demandes en attente.
    pub fn demandes(&self) -> impl Iterator<Item = &Demande> {
        self.en_attente.values()
    }

    pub fn est_vide(&self) -> bool {
        self.en_attente.is_empty()
    }

    /// Dernier chemin confirme par relecture pour cet ecran.
    pub fn confirmee(&self, cle: &str) -> Option<&Path> {
        self.confirmees.get(cle).map(PathBuf::as_path)
    }
}

/// Compare deux chemins, en tolerant les variantes de forme du meme fichier.
fn meme_chemin(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(chemin: &str) -> PathBuf {
        PathBuf::from(chemin)
    }

    #[test]
    fn confirmation_retardee() {
        let mut suivi = Suivi::new();
        let sequence = suivi.demander("a", p("/img/nouvelle.jpg"), 0);
        assert_eq!(
            suivi.relire("a", sequence, Some(&p("/img/ancienne.jpg")), 250),
            Issue::EnAttente
        );
        assert_eq!(
            suivi.relire("a", sequence, Some(&p("/img/ancienne.jpg")), 500),
            Issue::EnAttente
        );
        assert_eq!(
            suivi.relire("a", sequence, Some(&p("/img/nouvelle.jpg")), 750),
            Issue::Confirmee
        );
        assert!(suivi.est_vide());
        assert_eq!(suivi.confirmee("a"), Some(p("/img/nouvelle.jpg").as_path()));
    }

    #[test]
    fn absence_de_confirmation_bornee() {
        let mut suivi = Suivi::new();
        let sequence = suivi.demander("a", p("/img/nouvelle.jpg"), 1_000);
        let issue = suivi.relire(
            "a",
            sequence,
            Some(&p("/img/ancienne.jpg")),
            1_000 + DELAI_CONFIRMATION_MS - 1,
        );
        assert_eq!(issue, Issue::EnAttente);
        assert_eq!(
            suivi.relire(
                "a",
                sequence,
                Some(&p("/img/ancienne.jpg")),
                1_000 + DELAI_CONFIRMATION_MS
            ),
            Issue::DelaiDepasse
        );
        assert!(suivi.est_vide());
        assert_eq!(suivi.confirmee("a"), None);
        // Une relecture tardive ne peut plus confirmer : la demande est close.
        assert_eq!(
            suivi.relire("a", sequence, Some(&p("/img/nouvelle.jpg")), 2_000),
            Issue::Perimee
        );
        assert_eq!(suivi.confirmee("a"), None);
    }

    #[test]
    fn erreur_api() {
        let mut suivi = Suivi::new();
        let sequence = suivi.demander("a", p("/img/nouvelle.jpg"), 0);
        assert_eq!(
            suivi.echouer("a", sequence, "NSCocoaErrorDomain (code 260)"),
            Issue::EchecApi("NSCocoaErrorDomain (code 260)".to_string())
        );
        assert!(suivi.est_vide());
        assert_eq!(
            suivi.relire("a", sequence, Some(&p("/img/nouvelle.jpg")), 100),
            Issue::Perimee
        );
        assert_eq!(suivi.confirmee("a"), None);
    }

    #[test]
    fn ecran_disparu() {
        let mut suivi = Suivi::new();
        let sequence = suivi.demander("a", p("/img/nouvelle.jpg"), 0);
        assert_eq!(suivi.ecran_disparu("a", sequence), Issue::EcranDisparu);
        assert!(suivi.est_vide());
        assert_eq!(
            suivi.relire("a", sequence, Some(&p("/img/nouvelle.jpg")), 100),
            Issue::Perimee
        );
    }

    #[test]
    fn demande_remplacee_ignore_l_ancienne_confirmation() {
        let mut suivi = Suivi::new();
        let premiere = suivi.demander("a", p("/img/une.jpg"), 0);
        let seconde = suivi.demander("a", p("/img/deux.jpg"), 100);
        assert_ne!(premiere, seconde);
        assert_eq!(
            suivi.relire("a", premiere, Some(&p("/img/deux.jpg")), 200),
            Issue::Perimee
        );
        assert_eq!(
            suivi.relire("a", seconde, Some(&p("/img/une.jpg")), 300),
            Issue::EnAttente
        );
        assert_eq!(
            suivi.relire("a", seconde, Some(&p("/img/deux.jpg")), 400),
            Issue::Confirmee
        );
        assert_eq!(suivi.confirmee("a"), Some(p("/img/deux.jpg").as_path()));
    }

    #[test]
    fn plusieurs_ecrans_et_demandes_rapprochees() {
        let mut suivi = Suivi::new();
        let a = suivi.demander("a", p("/img/1.jpg"), 0);
        let b = suivi.demander("b", p("/img/2.jpg"), 10);
        assert_eq!(suivi.demandes().count(), 2);
        assert_eq!(
            suivi.relire("a", a, Some(&p("/img/1.jpg")), 250),
            Issue::Confirmee
        );
        assert_eq!(
            suivi.relire("b", b, Some(&p("/img/ancienne.jpg")), 250),
            Issue::EnAttente
        );
        assert_eq!(suivi.demandes().count(), 1);
        assert_eq!(
            suivi.relire(
                "b",
                b,
                Some(&p("/img/ancienne.jpg")),
                10 + DELAI_CONFIRMATION_MS
            ),
            Issue::DelaiDepasse
        );
        assert!(suivi.est_vide());
        assert_eq!(suivi.confirmee("a"), Some(p("/img/1.jpg").as_path()));
        assert_eq!(suivi.confirmee("b"), None);
    }

    #[test]
    fn demande_sans_ecran_ni_constat_reste_en_attente_puis_expire() {
        let mut suivi = Suivi::new();
        let sequence = suivi.demander("a", p("/img/1.jpg"), 0);
        assert_eq!(suivi.relire("a", sequence, None, 100), Issue::EnAttente);
        assert_eq!(
            suivi.relire("a", sequence, None, DELAI_CONFIRMATION_MS),
            Issue::DelaiDepasse
        );
    }

    #[test]
    fn reponse_inconnue_est_perimee() {
        let mut suivi = Suivi::new();
        assert_eq!(
            suivi.relire("a", 1, Some(&p("/img/1.jpg")), 0),
            Issue::Perimee
        );
        assert_eq!(suivi.ecran_disparu("a", 1), Issue::Perimee);
        assert_eq!(suivi.echouer("a", 1, "x"), Issue::Perimee);
    }

    #[test]
    fn bilan_erreur_puis_confirmation_autre_ecran() {
        let mut suivi = Suivi::new();
        let mut bilan = Bilan::new();
        let a = suivi.demander("a", p("/img/A.jpg"), 0);
        // B est demandee plus tard : son delai court a partir de 2 000 ms.
        let b = suivi.demander("b", p("/img/B.jpg"), 2_000);

        // Tick 1 : A expire, B reste en attente.
        let issue_a = suivi.relire("a", a, Some(&p("/img/ancienne.jpg")), DELAI_CONFIRMATION_MS);
        assert_eq!(issue_a, Issue::DelaiDepasse);
        bilan.echouer("a", "écran 1 non confirmé après 3 s");
        assert!(!suivi.est_vide());

        // Tick 2 : B confirme, le cycle se termine.
        let issue_b = suivi.relire("b", b, Some(&p("/img/B.jpg")), DELAI_CONFIRMATION_MS + 250);
        assert_eq!(issue_b, Issue::Confirmee);
        bilan.confirmer("b", p("/img/B.jpg"));
        assert!(suivi.est_vide());

        let resume = bilan.resume().expect("bilan non vide");
        assert!(resume.contains("écran 1 non confirmé"), "{resume}");
        assert!(resume.contains("confirmed on 1 display"), "{resume}");
    }

    #[test]
    fn confirmations_sur_deux_ticks_total_deux() {
        let mut suivi = Suivi::new();
        let mut bilan = Bilan::new();
        let a = suivi.demander("a", p("/img/A.jpg"), 0);
        let b = suivi.demander("b", p("/img/B.jpg"), 0);

        assert_eq!(
            suivi.relire("a", a, Some(&p("/img/A.jpg")), 250),
            Issue::Confirmee
        );
        bilan.confirmer("a", p("/img/A.jpg"));
        assert!(!suivi.est_vide());
        assert_eq!(bilan.confirmees(), 1);

        assert_eq!(
            suivi.relire("b", b, Some(&p("/img/B.jpg")), 500),
            Issue::Confirmee
        );
        bilan.confirmer("b", p("/img/B.jpg"));
        assert!(suivi.est_vide());
        assert_eq!(bilan.confirmees(), 2);
        assert_eq!(
            bilan.resume().as_deref(),
            Some("Wallpaper path confirmed on 2 display(s) — visual result not verified")
        );
    }

    #[test]
    fn demande_remplacee_par_un_echec_ne_confirme_plus() {
        let mut suivi = Suivi::new();
        let mut bilan = Bilan::new();

        // A acceptee, en attente.
        let a = suivi.demander("a", p("/img/A.jpg"), 0);
        assert!(suivi.en_attente("a").is_some());

        // B remplace A, puis l'appel Apple echoue : l'echec est enregistre.
        bilan.oublier("a");
        let b = suivi.demander("a", p("/img/B.jpg"), 100);
        assert_eq!(
            suivi.echouer("a", b, "NSCocoaErrorDomain (code 260)"),
            Issue::EchecApi("NSCocoaErrorDomain (code 260)".to_string())
        );
        bilan.echouer("a", "écran 1 : NSCocoaErrorDomain (code 260)");

        // La reponse de A ne peut plus confirmer ni effacer l'echec de B.
        assert_eq!(
            suivi.relire("a", a, Some(&p("/img/A.jpg")), 200),
            Issue::Perimee
        );
        assert_eq!(bilan.confirmees(), 0);
        let resume = bilan.resume().expect("bilan non vide");
        assert!(resume.contains("NSCocoaErrorDomain"), "{resume}");
        assert_eq!(suivi.confirmee("a"), None);
    }

    #[test]
    fn echec_api_immediat_puis_autre_ecran_confirme() {
        let mut suivi = Suivi::new();
        let mut bilan = Bilan::new();
        let b = suivi.demander("b", p("/img/B.jpg"), 0);
        let a = suivi.demander("a", p("/img/A.jpg"), 0);
        bilan.echouer("a", "écran 1 : disque illisible");
        assert_eq!(suivi.echouer("a", a, "disque illisible"), Issue::EchecApi("disque illisible".to_string()));
        assert!(!suivi.est_vide());

        assert_eq!(
            suivi.relire("b", b, Some(&p("/img/B.jpg")), 250),
            Issue::Confirmee
        );
        bilan.confirmer("b", p("/img/B.jpg"));
        assert!(suivi.est_vide());

        let resume = bilan.resume().expect("bilan non vide");
        assert!(resume.contains("disque illisible"), "{resume}");
        assert!(resume.contains("1 display(s)"), "{resume}");
        assert_eq!(bilan.confirmees(), 1);
    }

    #[test]
    fn bilan_vide_apres_emission_cycle_suivant_independant() {
        let mut bilan = Bilan::new();
        bilan.confirmer("a", p("/img/A.jpg"));
        let premier = bilan.resume().expect("bilan du premier cycle");
        assert!(premier.contains("1 display"));
        bilan.vider();
        assert!(bilan.est_vide());
        assert_eq!(bilan.resume(), None);

        bilan.echouer("a", "écran 1 : échec du cycle suivant");
        let second = bilan.resume().expect("bilan du second cycle");
        assert!(second.starts_with("Change not confirmed"));
        assert_eq!(bilan.confirmees(), 0);
    }

    #[test]
    fn remplacement_pendant_un_cycle_pas_de_resultat_obsolete() {
        let mut suivi = Suivi::new();
        let mut bilan = Bilan::new();
        let a = suivi.demander("a", p("/img/A.jpg"), 0);
        assert_eq!(
            suivi.relire("a", a, Some(&p("/img/A.jpg")), 100),
            Issue::Confirmee
        );
        bilan.confirmer("a", p("/img/A.jpg"));
        assert_eq!(bilan.confirmees(), 1);

        // Nouvelle intention sur le meme ecran pendant le cycle : le resultat
        // de A est remplace.
        bilan.oublier("a");
        assert_eq!(bilan.confirmees(), 0);
        let b = suivi.demander("a", p("/img/B.jpg"), 200);
        assert_eq!(
            suivi.relire("a", a, Some(&p("/img/A.jpg")), 300),
            Issue::Perimee
        );
        assert_eq!(
            suivi.relire("a", b, Some(&p("/img/B.jpg")), 400),
            Issue::Confirmee
        );
        bilan.confirmer("a", p("/img/B.jpg"));
        assert_eq!(bilan.confirmees(), 1);
        assert_eq!(
            bilan.resume().as_deref(),
            Some("Wallpaper path confirmed on 1 display(s) — visual result not verified")
        );

        // Si la nouvelle intention echoue, seul l'echec de B reste.
        bilan.oublier("a");
        bilan.echouer("a", "écran 1 : nouvelle demande refusee");
        let resume = bilan.resume().expect("bilan non vide");
        assert!(resume.contains("nouvelle demande refusee"), "{resume}");
        assert!(!resume.contains("Wallpaper path confirmed"), "{resume}");
    }
}
