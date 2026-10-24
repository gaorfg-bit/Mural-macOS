//! Reglages persistants — equivalent de `mural/config.py` et de
//! l'`AppSettings.swift` du premier prototype.
//!
//! Stockage : un fichier JSON dans `~/Library/Application Support/Mural/`.
//!
//! Deux schemas ont coexiste dans ce fichier : l'ancien (Swift/Linux :
//! `folder`, `per_monitor`, `slideshow_*`, …) et le schema Rust actuel
//! (`dossier`, `image_par_ecran`, …). La lecture est volontairement explicite
//! (`From<serde_json::Value>`) plutot qu'un alias serde : un fichier mixte qui
//! contient les deux cles doit charger sans erreur de doublon.
//!
//! Regles :
//! - une cle Rust presente a toujours priorite ; l'ancienne cle n'est qu'un
//!   repli ;
//! - toutes les cles non reconnues (dont les anciennes cles et les champs
//!   Swift non exploites) sont conservees dans `champs_inconnus` et reecrites
//!   telles quelles a la sauvegarde ;
//! - charger ne modifie jamais le fichier.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Reglages de l'application.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "Value", into = "Value")]
pub struct Reglages {
    /// Dossier de la bibliotheque d'images.
    pub dossier: String,
    /// « nom » ou « date ».
    pub tri: String,
    /// Appliquer la meme image sur tous les ecrans.
    pub meme_image_partout: bool,
    /// Image choisie par ecran (cle : UUID d'ecran, valeur : chemin).
    pub image_par_ecran: BTreeMap<String, String>,
    /// Images ajoutees aux favoris du diaporama (liste globale historique).
    pub favoris: Vec<String>,
    /// Selection independante par ecran (desactivee par defaut : les
    /// configurations existantes gardent leur liste globale).
    pub selection_independante: bool,
    /// Listes dediees par ecran, indexees par UUID ColorSync. Conservees quel
    /// que soit le mode courant : changer de mode ne supprime aucune liste.
    pub favoris_par_ecran: BTreeMap<String, Vec<String>>,
    /// Dossiers marques dans la bibliotheque (distincts des images du diaporama).
    pub dossiers_favoris: Vec<String>,
    /// UUID de l’ecran selectionne au dernier lancement.
    pub ecran_selectionne: Option<String>,
    /// Aucun demarrage automatique : seuls ces parametres sont conserves.
    pub diaporama_intervalle: u64,
    pub diaporama_aleatoire: bool,
    /// None : tous les ecrans connectes ; Some([]) : aucun ecran choisi.
    pub diaporama_ecrans: Option<Vec<String>>,
    /// Dossier par defaut associe a un ecran (equivalent `monitor_folders`).
    pub dossier_par_ecran: BTreeMap<String, String>,
    /// Largeur de fenetre memorisee.
    pub largeur_fenetre: f64,
    /// Hauteur de fenetre memorisee.
    pub hauteur_fenetre: f64,
    /// Cles non reconnues, conservees pour ne rien perdre (ancien schema
    /// Swift/Linux, champs inutilises, futur). Jamais interpretees ici.
    pub champs_inconnus: Map<String, Value>,
}

impl Default for Reglages {
    fn default() -> Self {
        Reglages {
            dossier: String::new(),
            tri: "nom".to_string(),
            meme_image_partout: false,
            image_par_ecran: BTreeMap::new(),
            favoris: Vec::new(),
            selection_independante: false,
            favoris_par_ecran: BTreeMap::new(),
            dossiers_favoris: Vec::new(),
            ecran_selectionne: None,
            diaporama_intervalle: 300,
            diaporama_aleatoire: false,
            diaporama_ecrans: None,
            dossier_par_ecran: BTreeMap::new(),
            largeur_fenetre: 1280.0,
            hauteur_fenetre: 800.0,
            champs_inconnus: Map::new(),
        }
    }
}

/// Toutes les cles du schema Rust actuel. Elles ne doivent jamais etre
/// dupliquees dans `champs_inconnus`, sinon la sauvegarde ecrirait deux fois la
/// meme cle.
const CLES_RUST: [&str; 15] = [
    "dossier",
    "tri",
    "meme_image_partout",
    "image_par_ecran",
    "favoris",
    "selection_independante",
    "favoris_par_ecran",
    "dossiers_favoris",
    "ecran_selectionne",
    "diaporama_intervalle",
    "diaporama_aleatoire",
    "diaporama_ecrans",
    "dossier_par_ecran",
    "largeur_fenetre",
    "hauteur_fenetre",
];

fn texte(brut: &Map<String, Value>, cle: &str) -> Option<String> {
    brut.get(cle).and_then(Value::as_str).map(str::to_owned)
}

fn booleen(brut: &Map<String, Value>, cle: &str) -> Option<bool> {
    brut.get(cle).and_then(Value::as_bool)
}

fn nombre(brut: &Map<String, Value>, cle: &str) -> Option<f64> {
    match brut.get(cle)? {
        Value::Number(nombre) => nombre.as_f64(),
        Value::String(texte) => texte.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn entier(brut: &Map<String, Value>, cle: &str) -> Option<i64> {
    match brut.get(cle)? {
        Value::Number(nombre) => nombre
            .as_i64()
            .or_else(|| nombre.as_f64().map(|v| v as i64)),
        Value::String(texte) => texte.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// Une ancienne valeur peut etre une chaine seule (réparée en liste d'un
/// element), une liste, ou un type inconnu (réparé en liste vide), comme
/// `WallpaperSettings.from_dict` cote Linux.
fn liste_valeur(valeur: &Value) -> Vec<String> {
    match valeur {
        Value::Array(elements) => elements
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Value::String(texte) => vec![texte.clone()],
        _ => Vec::new(),
    }
}

fn liste(brut: &Map<String, Value>, cle: &str) -> Option<Vec<String>> {
    brut.get(cle).map(liste_valeur)
}

fn dictionnaire_chaines(
    brut: &Map<String, Value>,
    cle: &str,
) -> Option<BTreeMap<String, String>> {
    match brut.get(cle)? {
        Value::Object(entrees) => Some(
            entrees
                .iter()
                .filter_map(|(cle, valeur)| {
                    valeur
                        .as_str()
                        .map(|valeur| (cle.clone(), valeur.to_owned()))
                })
                .collect(),
        ),
        _ => Some(BTreeMap::new()),
    }
}

fn dictionnaire_listes(
    brut: &Map<String, Value>,
    cle: &str,
) -> Option<BTreeMap<String, Vec<String>>> {
    match brut.get(cle)? {
        Value::Object(entrees) => Some(
            entrees
                .iter()
                .map(|(cle, valeur)| (cle.clone(), liste_valeur(valeur)))
                .collect(),
        ),
        _ => Some(BTreeMap::new()),
    }
}

/// Ancien tri Swift/Linux : `name` devient `nom`, `date` reste `date`.
fn normaliser_ancien_tri(valeur: &str) -> String {
    match valeur.trim().to_ascii_lowercase().as_str() {
        "date" => "date",
        _ => "nom",
    }
    .to_string()
}

/// Ancien intervalle en MINUTES -> secondes Rust, borne a 60..=86400.
///
/// Aucun debordement : `saturating_mul` plafonne avant le `clamp`.
pub fn minutes_vers_secondes(minutes: i64) -> u64 {
    let minutes = minutes.max(0) as u64;
    minutes.saturating_mul(60).clamp(60, 86_400)
}

impl From<Value> for Reglages {
    fn from(valeur: Value) -> Self {
        let mut brut = match valeur {
            Value::Object(entrees) => entrees,
            _ => Map::new(),
        };
        let mut reglages = Reglages::default();

        reglages.dossier = texte(&brut, "dossier")
            .or_else(|| texte(&brut, "folder"))
            .unwrap_or_default();

        if let Some(valeur) = texte(&brut, "tri") {
            reglages.tri = valeur;
        } else if let Some(valeur) = texte(&brut, "sort") {
            reglages.tri = normaliser_ancien_tri(&valeur);
        }

        reglages.meme_image_partout =
            booleen(&brut, "meme_image_partout")
                .or_else(|| booleen(&brut, "same_image_on_all"))
                .unwrap_or(false);

        reglages.image_par_ecran = if brut.contains_key("image_par_ecran") {
            dictionnaire_chaines(&brut, "image_par_ecran").unwrap_or_default()
        } else {
            dictionnaire_chaines(&brut, "per_monitor").unwrap_or_default()
        };

        reglages.favoris = if brut.contains_key("favoris") {
            liste(&brut, "favoris").unwrap_or_default()
        } else {
            liste(&brut, "slideshow_images").unwrap_or_default()
        };

        reglages.selection_independante = if let Some(valeur) =
            booleen(&brut, "selection_independante")
        {
            valeur
        } else {
            // Une configuration Swift ecrit toujours ses listes dediees : leur
            // presence signale le mode independant.
            brut.contains_key("slideshow_images_per_monitor")
        };

        reglages.favoris_par_ecran = if brut.contains_key("favoris_par_ecran") {
            dictionnaire_listes(&brut, "favoris_par_ecran").unwrap_or_default()
        } else {
            dictionnaire_listes(&brut, "slideshow_images_per_monitor").unwrap_or_default()
        };

        reglages.dossiers_favoris = if brut.contains_key("dossiers_favoris") {
            liste(&brut, "dossiers_favoris").unwrap_or_default()
        } else {
            liste(&brut, "folder_bookmarks").unwrap_or_default()
        };

        reglages.ecran_selectionne = texte(&brut, "ecran_selectionne");

        reglages.diaporama_intervalle = if let Some(secondes) = entier(&brut, "diaporama_intervalle")
        {
            secondes.max(0) as u64
        } else if let Some(minutes) = entier(&brut, "slideshow_interval") {
            minutes_vers_secondes(minutes)
        } else {
            Reglages::default().diaporama_intervalle
        };

        reglages.diaporama_aleatoire = booleen(&brut, "diaporama_aleatoire")
            .or_else(|| booleen(&brut, "slideshow_random"))
            .unwrap_or(false);

        reglages.diaporama_ecrans = if brut.contains_key("diaporama_ecrans") {
            match brut.get("diaporama_ecrans") {
                Some(Value::Null) => None,
                Some(Value::Array(_)) | Some(Value::String(_)) => {
                    Some(liste_valeur(brut.get("diaporama_ecrans").unwrap()))
                }
                _ => None,
            }
        } else if brut.contains_key("slideshow_monitors") {
            // Ancien schema : liste vide = tous les ecrans (Rust `None`),
            // jamais « aucun ecran » (`Some([])`).
            let participants = liste_valeur(brut.get("slideshow_monitors").unwrap());
            if participants.is_empty() {
                None
            } else {
                Some(participants)
            }
        } else {
            None
        };

        reglages.dossier_par_ecran = if brut.contains_key("dossier_par_ecran") {
            dictionnaire_chaines(&brut, "dossier_par_ecran").unwrap_or_default()
        } else {
            dictionnaire_chaines(&brut, "monitor_folders").unwrap_or_default()
        };

        reglages.largeur_fenetre = nombre(&brut, "largeur_fenetre")
            .or_else(|| nombre(&brut, "window_width"))
            .unwrap_or(1280.0);
        reglages.hauteur_fenetre = nombre(&brut, "hauteur_fenetre")
            .or_else(|| nombre(&brut, "window_height"))
            .unwrap_or(800.0);

        for cle in CLES_RUST {
            brut.remove(cle);
        }
        reglages.champs_inconnus = brut;
        reglages
    }
}

impl From<Reglages> for Value {
    fn from(reglages: Reglages) -> Self {
        let mut sortie = Map::new();
        sortie.insert("dossier".into(), json!(reglages.dossier));
        sortie.insert("tri".into(), json!(reglages.tri));
        sortie.insert(
            "meme_image_partout".into(),
            json!(reglages.meme_image_partout),
        );
        sortie.insert("image_par_ecran".into(), json!(reglages.image_par_ecran));
        sortie.insert("favoris".into(), json!(reglages.favoris));
        sortie.insert(
            "selection_independante".into(),
            json!(reglages.selection_independante),
        );
        sortie.insert("favoris_par_ecran".into(), json!(reglages.favoris_par_ecran));
        sortie.insert("dossiers_favoris".into(), json!(reglages.dossiers_favoris));
        sortie.insert(
            "ecran_selectionne".into(),
            match reglages.ecran_selectionne {
                Some(cle) => json!(cle),
                None => Value::Null,
            },
        );
        sortie.insert(
            "diaporama_intervalle".into(),
            json!(reglages.diaporama_intervalle),
        );
        sortie.insert(
            "diaporama_aleatoire".into(),
            json!(reglages.diaporama_aleatoire),
        );
        sortie.insert(
            "diaporama_ecrans".into(),
            match reglages.diaporama_ecrans {
                Some(ecrans) => json!(ecrans),
                None => Value::Null,
            },
        );
        sortie.insert("dossier_par_ecran".into(), json!(reglages.dossier_par_ecran));
        sortie.insert("largeur_fenetre".into(), json!(reglages.largeur_fenetre));
        sortie.insert("hauteur_fenetre".into(), json!(reglages.hauteur_fenetre));

        for (cle, valeur) in reglages.champs_inconnus {
            if !CLES_RUST.contains(&cle.as_str()) {
                sortie.insert(cle, valeur);
            }
        }
        Value::Object(sortie)
    }
}

/// Emplacement du fichier de reglages.
pub fn chemin_fichier() -> Option<PathBuf> {
    // Les outils de verification isolent explicitement leurs preferences.
    if let Some(chemin) = std::env::var_os("MURAL_REGLAGES") {
        return Some(PathBuf::from(chemin));
    }
    let home = std::env::var("HOME").ok()?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Mural")
            .join("reglages.json"),
    )
}

/// Charge les reglages. En cas de probleme, renvoie les valeurs par defaut
/// (jamais d'erreur remontee : l'application doit demarrer dans tous les cas).
///
/// Ne modifie jamais le fichier : la migration est purement en memoire.
pub fn charger() -> Reglages {
    match chemin_fichier() {
        Some(chemin) => charger_depuis(&chemin),
        None => Reglages::default(),
    }
}

/// Lit un fichier de reglages sans le modifier. Une absence seule autorise
/// les valeurs par defaut ; les autres erreurs sont journalisees.
pub fn charger_depuis(chemin: &Path) -> Reglages {
    match essayer_charger_depuis(chemin) {
        Ok(reglages) => reglages,
        Err(erreur) => {
            crate::journal::erreur(&format!("Settings could not be read: {erreur}. Original file preserved: {}", chemin.display()));
            Reglages::default()
        }
    }
}

/// Lecture stricte : distingue absence, erreur d'acces et JSON inutilisable.
pub fn essayer_charger_depuis(chemin: &Path) -> std::io::Result<Reglages> {
    let texte = match fs::read_to_string(chemin) {
        Ok(texte) => texte,
        Err(erreur) if erreur.kind() == std::io::ErrorKind::NotFound => return Ok(Reglages::default()),
        Err(erreur) => return Err(erreur),
    };
    let valeur: Value = serde_json::from_str(&texte)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if !valeur.is_object() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Settings must contain a JSON object"));
    }
    serde_json::from_value(valeur)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Lit le JSON de reglages, y compris l'ancien schema Swift/Linux. Un JSON
/// invalide retombe sur les valeurs par defaut.
pub fn lire_texte(texte: &str) -> Reglages {
    serde_json::from_str(texte).unwrap_or_default()
}

/// Enregistre les reglages, en creant le dossier si besoin.
pub fn sauvegarder(reglages: &Reglages) -> std::io::Result<()> {
    match chemin_fichier() {
        Some(chemin) => sauvegarder_vers(&chemin, reglages),
        None => Ok(()),
    }
}

/// Ecrit les reglages a l'emplacement donne (utilisé par `sauvegarder` et par
/// les tests sous `/private/tmp`).
pub fn sauvegarder_vers(chemin: &Path, reglages: &Reglages) -> std::io::Result<()> {
    // Ne jamais remplacer par des valeurs par defaut un fichier que l'on ne
    // sait pas lire, y compris s'il a ete corrompu depuis le demarrage.
    essayer_charger_depuis(chemin)?;
    if let Some(parent) = chemin.parent() {
        fs::create_dir_all(parent)?;
    }
    let texte = serde_json::to_string_pretty(reglages)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let temporaire = chemin.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temporaire, texte)?;
    fs::rename(temporaire, chemin)
}

// ---------------------------------------------------------------------------
// Remappage des anciens identifiants d'ecran (Swift `nom@x,y` -> UUID ColorSync)
// ---------------------------------------------------------------------------

/// Bilan d'un remappage. `conservees` compte les cles laissees intactes
/// (UUID Rust deja valides, ecrans inconnus ou deconnectes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RapportRemappage {
    pub remappees: usize,
    pub collisions: usize,
    pub conservees: usize,
}

/// Ancien identifiant Swift d'un ecran : `nom@x,y` avec la position arrondie,
/// exactement comme `EcransService.swift` (`Int(cadre.origin.x.rounded())`).
pub fn cle_ancienne(ecran: &crate::ecrans::Ecran) -> String {
    format!(
        "{}@{},{}",
        ecran.nom,
        ecran.x.round() as i64,
        ecran.y.round() as i64
    )
}

/// Table explicite ancien identifiant -> UUID, construite uniquement a partir
/// des ecrans connectes. Aucune correspondance par index n'est inventee.
pub fn table_correspondances(
    ecrans: &[crate::ecrans::Ecran],
) -> BTreeMap<String, String> {
    ecrans
        .iter()
        .map(|ecran| (cle_ancienne(ecran), ecran.cle.clone()))
        .collect()
}

/// Charge les reglages puis remappe les anciens identifiants d'ecran avec la
/// table des ecrans connectes. N'ecrit jamais le fichier.
pub fn charger_avec_ecrans(ecrans: &[crate::ecrans::Ecran]) -> Reglages {
    essayer_charger_avec_ecrans(ecrans).unwrap_or_default()
}

/// Meme remappage, en conservant l'erreur pour l'interface.
pub fn essayer_charger_avec_ecrans(ecrans: &[crate::ecrans::Ecran]) -> std::io::Result<Reglages> {
    let mut reglages = match chemin_fichier() {
        Some(chemin) => essayer_charger_depuis(&chemin)?,
        None => Reglages::default(),
    };
    remapper_ecrans(&mut reglages, &table_correspondances(ecrans));
    Ok(reglages)
}

/// Variante de `charger_avec_ecrans` a partir d'un chemin explicite.
pub fn charger_fichier_avec_ecrans(
    chemin: &Path,
    ecrans: &[crate::ecrans::Ecran],
) -> Reglages {
    let mut reglages = charger_depuis(chemin);
    let table = table_correspondances(ecrans);
    if !table.is_empty() {
        remapper_ecrans(&mut reglages, &table);
    }
    reglages
}

/// Remappe en place les anciens identifiants presents dans les trois tables
/// par ecran et dans la liste des participants au diaporama.
///
/// - correspondance exacte et cible libre : la valeur migre vers l'UUID ;
/// - collision (l'UUID existe deja) : la valeur Rust est conservee et la
///   valeur source reste en place, rien n'est perdu ;
/// - aucune correspondance : la donnee est laissee intacte, jamais affectee a
///   un autre ecran.
pub fn remapper_ecrans(
    reglages: &mut Reglages,
    table: &BTreeMap<String, String>,
) -> RapportRemappage {
    let mut rapport = RapportRemappage::default();
    remapper_valeurs(&mut reglages.image_par_ecran, table, &mut rapport);
    remapper_listes(&mut reglages.favoris_par_ecran, table, &mut rapport);
    remapper_valeurs(&mut reglages.dossier_par_ecran, table, &mut rapport);
    if let Some(participants) = &mut reglages.diaporama_ecrans {
        let mut resultat = Vec::with_capacity(participants.len());
        for participant in participants.iter() {
            match table.get(participant) {
                Some(cible) if cible != participant => {
                    if resultat.contains(cible) {
                        // Deja present sous l'UUID : garder la source aussi.
                        rapport.collisions += 1;
                        resultat.push(participant.clone());
                    } else {
                        rapport.remappees += 1;
                        resultat.push(cible.clone());
                    }
                }
                _ => {
                    rapport.conservees += 1;
                    resultat.push(participant.clone());
                }
            }
        }
        *participants = resultat;
    }
    rapport
}

fn remapper_valeurs(
    map: &mut BTreeMap<String, String>,
    table: &BTreeMap<String, String>,
    rapport: &mut RapportRemappage,
) {
    let sources: Vec<String> = map.keys().cloned().collect();
    for source in sources {
        let Some(cible) = table.get(&source) else {
            rapport.conservees += 1;
            continue;
        };
        if cible == &source {
            rapport.conservees += 1;
        } else if map.contains_key(cible) {
            // Collision : valeur Rust (cible) conservee, source gardee.
            rapport.collisions += 1;
        } else if let Some(valeur) = map.remove(&source) {
            map.insert(cible.clone(), valeur);
            rapport.remappees += 1;
        }
    }
}

fn remapper_listes(
    map: &mut BTreeMap<String, Vec<String>>,
    table: &BTreeMap<String, String>,
    rapport: &mut RapportRemappage,
) {
    let sources: Vec<String> = map.keys().cloned().collect();
    for source in sources {
        let Some(cible) = table.get(&source) else {
            rapport.conservees += 1;
            continue;
        };
        if cible == &source {
            rapport.conservees += 1;
        } else if map.contains_key(cible) {
            rapport.collisions += 1;
        } else if let Some(valeur) = map.remove(&source) {
            map.insert(cible.clone(), valeur);
            rapport.remappees += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dossiers_favoris_et_ecran_survivent_a_la_serialisation() {
        let texte = r#"{"dossiers_favoris":["/Images/Paysages"],"ecran_selectionne":"uuid-ecran"}"#;
        let reglages: Reglages = serde_json::from_str(texte).unwrap();
        let valeur = serde_json::to_value(reglages).unwrap();
        assert_eq!(
            valeur["dossiers_favoris"],
            serde_json::json!(["/Images/Paysages"])
        );
        assert_eq!(valeur["ecran_selectionne"], "uuid-ecran");
    }

    #[test]
    fn ancien_json_reste_en_mode_global_et_garde_ses_favoris() {
        // Aucun champ de selection par ecran : le fichier existant doit garder
        // exactement son comportement (liste globale).
        let texte = r#"{"favoris":["/Images/a.png","/Images/b.png"],"meme_image_partout":true}"#;
        let reglages: Reglages = serde_json::from_str(texte).unwrap();
        assert!(!reglages.selection_independante);
        assert!(reglages.favoris_par_ecran.is_empty());
        assert_eq!(reglages.favoris.len(), 2);
        assert_eq!(
            reglages.favoris_selection(Some("uuid-ecran")),
            reglages.favoris.as_slice()
        );
        let valeur = serde_json::to_value(&reglages).unwrap();
        assert_eq!(
            valeur["favoris"],
            serde_json::json!(["/Images/a.png", "/Images/b.png"])
        );
        assert_eq!(valeur["selection_independante"], false);
    }

    #[test]
    fn roundtrip_des_listes_par_ecran() {
        let mut reglages = Reglages::default();
        reglages.selection_independante = true;
        reglages.favoris = vec!["/global/g.png".into()];
        reglages
            .favoris_par_ecran
            .insert("uuid-a".into(), vec!["/a/1.png".into(), "/a/2.png".into()]);
        reglages
            .favoris_par_ecran
            .insert("uuid-b".into(), Vec::new());
        let texte = serde_json::to_string_pretty(&reglages).unwrap();
        let relu: Reglages = serde_json::from_str(&texte).unwrap();
        assert!(relu.selection_independante);
        assert_eq!(relu.favoris, reglages.favoris);
        assert_eq!(relu.favoris_par_ecran, reglages.favoris_par_ecran);
    }

    #[test]
    fn deux_listes_independantes_sans_doublons() {
        let mut reglages = Reglages::default();
        reglages.selection_independante = true;
        reglages.ajouter_favori("/a/1.png", Some("uuid-a"));
        reglages.ajouter_favori("/a/1.png", Some("uuid-a"));
        reglages.ajouter_favori("/b/1.png", Some("uuid-b"));
        assert_eq!(reglages.favoris_selection(Some("uuid-a")), ["/a/1.png"]);
        assert_eq!(reglages.favoris_selection(Some("uuid-b")), ["/b/1.png"]);
        assert!(
            reglages.favoris.is_empty(),
            "le mode par ecran ne touche pas la liste globale"
        );
        reglages.retirer_favori("/a/1.png", Some("uuid-a"));
        assert!(reglages.favoris_selection(Some("uuid-a")).is_empty());
        assert_eq!(reglages.favoris_selection(Some("uuid-b")), ["/b/1.png"]);
    }

    #[test]
    fn ecran_sans_liste_dediee_ne_replie_pas_sur_la_liste_globale() {
        let mut reglages = Reglages::default();
        reglages.selection_independante = true;
        reglages.favoris = vec!["/global/g.png".into()];
        assert!(reglages.favoris_selection(Some("uuid-b")).is_empty());
        reglages.ajouter_favori("/a/1.png", Some("uuid-a"));
        assert!(reglages.favoris_selection(Some("uuid-b")).is_empty());
        assert_eq!(reglages.favoris, ["/global/g.png"]);
    }

    #[test]
    fn changer_de_mode_ne_supprime_aucune_liste() {
        let mut reglages = Reglages::default();
        reglages.favoris = vec!["/global/g.png".into()];
        reglages
            .favoris_par_ecran
            .insert("uuid-a".into(), vec!["/a/1.png".into()]);
        reglages.selection_independante = true;
        assert_eq!(reglages.favoris_selection(Some("uuid-a")), ["/a/1.png"]);
        reglages.selection_independante = false;
        assert_eq!(reglages.favoris, ["/global/g.png"]);
        assert_eq!(
            reglages.favoris_par_ecran.get("uuid-a").map(Vec::as_slice).unwrap(),
            ["/a/1.png"]
        );
        reglages.selection_independante = true;
        assert_eq!(reglages.favoris_selection(Some("uuid-a")), ["/a/1.png"]);
        assert_eq!(reglages.favoris, ["/global/g.png"]);
    }

    #[test]
    fn retirer_un_dossier_vise_la_liste_pertinente() {
        let mut reglages = Reglages::default();
        reglages.selection_independante = true;
        reglages.ajouter_favori("/a/1.png", Some("uuid-a"));
        reglages.ajouter_favori("/a/2.png", Some("uuid-a"));
        reglages.ajouter_favori("/b/1.png", Some("uuid-b"));
        reglages.retirer_dossier_favori(Path::new("/a"), Some("uuid-a"));
        assert!(reglages.favoris_selection(Some("uuid-a")).is_empty());
        assert_eq!(reglages.favoris_selection(Some("uuid-b")), ["/b/1.png"]);
        // Sans ecran (mode global), le meme geste retire de la liste globale.
        reglages.selection_independante = false;
        reglages.favoris = vec!["/a/1.png".into()];
        reglages.retirer_dossier_favori(Path::new("/a"), None);
        assert!(reglages.favoris.is_empty());
        assert_eq!(reglages.favoris_par_ecran["uuid-b"], ["/b/1.png"]);
    }
}

impl Reglages {
    /// Liste d'images pertinente pour l'ecran donne.
    ///
    /// - mode global (historique) : la liste `favoris`, quel que soit l'ecran ;
    /// - mode independant : uniquement la liste dediee de cet ecran. Un ecran
    ///   sans liste dediee renvoie une liste vide (aucun repli global).
    pub fn favoris_selection(&self, cle: Option<&str>) -> &[String] {
        if self.selection_independante {
            cle.and_then(|cle| self.favoris_par_ecran.get(cle))
                .map(Vec::as_slice)
                .unwrap_or(&[])
        } else {
            &self.favoris
        }
    }

    pub fn favori_inclus(&self, chemin: &str, cle: Option<&str>) -> bool {
        self.favoris_selection(cle).iter().any(|p| p == chemin)
    }

    /// Liste mutable visee par une action d'ajout/retrait. Sans ecran courant
    /// en mode independant, l'action ne cree aucune entree artificielle.
    fn liste_mutable(&mut self, cle: Option<&str>) -> Option<&mut Vec<String>> {
        if self.selection_independante {
            cle.map(|cle| self.favoris_par_ecran.entry(cle.to_owned()).or_default())
        } else {
            Some(&mut self.favoris)
        }
    }

    pub fn basculer_favori(&mut self, chemin: &str, cle: Option<&str>) {
        let Some(liste) = self.liste_mutable(cle) else {
            return;
        };
        if let Some(position) = liste.iter().position(|p| p == chemin) {
            liste.remove(position);
        } else {
            liste.push(chemin.to_owned());
        }
    }

    pub fn ajouter_favori(&mut self, chemin: &str, cle: Option<&str>) {
        let Some(liste) = self.liste_mutable(cle) else {
            return;
        };
        if !liste.iter().any(|p| p == chemin) {
            liste.push(chemin.to_owned());
        }
    }

    pub fn retirer_favori(&mut self, chemin: &str, cle: Option<&str>) {
        let Some(liste) = self.liste_mutable(cle) else {
            return;
        };
        liste.retain(|p| p != chemin);
    }

    pub fn retirer_dossier_favori(&mut self, dossier: &Path, cle: Option<&str>) {
        let Some(liste) = self.liste_mutable(cle) else {
            return;
        };
        liste.retain(|p| Path::new(p).parent() != Some(dossier));
    }

    pub fn memoriser_dossier(&mut self, dossier: &std::path::Path, ecran: Option<&str>) {
        self.dossier = dossier.to_string_lossy().into_owned();
        if let Some(cle) = ecran {
            self.dossier_par_ecran
                .insert(cle.to_owned(), self.dossier.clone());
        }
    }

    pub fn dossier_ecran(&self, cle: &str) -> Option<PathBuf> {
        self.dossier_par_ecran
            .get(cle)
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
    }

    pub fn basculer_dossier_favori(&mut self, dossier: &std::path::Path) {
        let texte = dossier.to_string_lossy().into_owned();
        if self.dossiers_favoris.contains(&texte) {
            self.dossiers_favoris.retain(|p| p != &texte);
        } else {
            self.dossiers_favoris.push(texte);
        }
    }
}

#[cfg(test)]
mod tests_compatibilite {
    use super::*;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    fn json_swift_complet() -> Value {
        json!({
            "folder": "/tmp/photos",
            "mode": "zoom",
            "lock_screen": true,
            "same_image_on_all": true,
            "per_monitor": {"DELL U2720Q@0,0": "/tmp/a.png"},
            "window_width": 1440,
            "window_height": 900,
            "window_maximized": true,
            "slideshow_enabled": true,
            "slideshow_interval": 10,
            "slideshow_random": true,
            "slideshow_images": ["/tmp/s1.png", "/tmp/s2.png"],
            "slideshow_images_per_monitor": {"DELL U2720Q@0,0": ["/tmp/s1.png"]},
            "slideshow_monitors": [],
            "folder_bookmarks": ["/tmp/photos"],
            "monitor_folders": {"DELL U2720Q@0,0": "/tmp/photos"},
            "avif_use_for_gnome": true,
            "last_seen_version": "1.2.3",
            "cle_inconnue": {"x": 1}
        })
    }

    fn chemin_temporaire(nom: &str) -> std::path::PathBuf {
        let chemin = std::path::PathBuf::from(format!(
            "/private/tmp/mural-preferences-{}-{nom}.json",
            std::process::id()
        ));
        let _ = fs::remove_file(&chemin);
        chemin
    }

    fn ecran_factice(nom: &str, cle: &str, x: f64, y: f64) -> crate::ecrans::Ecran {
        crate::ecrans::Ecran {
            index: 0,
            identifiant: 1,
            cle: cle.to_string(),
            nom: nom.to_string(),
            largeur: 1920,
            hauteur: 1080,
            x,
            y,
            principal: true,
        }
    }

    #[test]
    fn ancien_json_swift_complet_est_importe() {
        let reglages = lire_texte(&json_swift_complet().to_string());

        assert_eq!(reglages.dossier, "/tmp/photos");
        assert_eq!(reglages.tri, "nom");
        assert!(reglages.meme_image_partout);
        assert_eq!(
            reglages.image_par_ecran["DELL U2720Q@0,0"],
            "/tmp/a.png"
        );
        assert_eq!(reglages.favoris, ["/tmp/s1.png", "/tmp/s2.png"]);
        assert_eq!(
            reglages.favoris_par_ecran["DELL U2720Q@0,0"],
            ["/tmp/s1.png"]
        );
        assert!(reglages.selection_independante);
        assert_eq!(reglages.diaporama_intervalle, 600, "10 minutes = 600 s");
        assert!(reglages.diaporama_aleatoire);
        assert_eq!(reglages.diaporama_ecrans, None, "liste vide = tous");
        assert_eq!(reglages.dossiers_favoris, ["/tmp/photos"]);
        assert_eq!(
            reglages.dossier_par_ecran["DELL U2720Q@0,0"],
            "/tmp/photos"
        );
        assert_eq!(reglages.largeur_fenetre, 1440.0);
        assert_eq!(reglages.hauteur_fenetre, 900.0);

        for cle in [
            "mode",
            "lock_screen",
            "window_maximized",
            "slideshow_enabled",
            "avif_use_for_gnome",
            "last_seen_version",
            "cle_inconnue",
        ] {
            assert!(
                reglages.champs_inconnus.contains_key(cle),
                "champ Swift inutilise perdu : {cle}"
            );
        }
    }

    #[test]
    fn ancien_json_rust_reste_identique() {
        let mut avant = Reglages::default();
        avant.dossier = "/tmp/rust".into();
        avant.tri = "date".into();
        avant.meme_image_partout = false;
        avant.favoris = vec!["/tmp/r1.png".into()];
        avant.selection_independante = false;
        avant
            .favoris_par_ecran
            .insert("UUID-A".into(), vec!["/tmp/r2.png".into()]);
        avant
            .image_par_ecran
            .insert("UUID-A".into(), "/tmp/r1.png".into());
        avant.diaporama_intervalle = 300;
        avant.diaporama_aleatoire = false;
        avant.diaporama_ecrans = Some(Vec::new());
        avant.largeur_fenetre = 1024.0;
        avant.hauteur_fenetre = 700.0;
        avant
            .dossier_par_ecran
            .insert("UUID-A".into(), "/tmp/rust".into());

        let attendu = serde_json::to_value(&avant).unwrap();
        let texte = serde_json::to_string(&attendu).unwrap();
        let relu = lire_texte(&texte);

        assert_eq!(serde_json::to_value(&relu).unwrap(), attendu);
        assert!(relu.champs_inconnus.is_empty());
        assert_eq!(relu.diaporama_ecrans, Some(Vec::new()));
        assert!(!relu.selection_independante);
    }

    #[test]
    fn json_mixte_donne_priorite_aux_cles_rust() {
        let texte = json!({
            "dossier": "/rust",
            "folder": "/swift",
            "tri": "date",
            "sort": "name",
            "meme_image_partout": false,
            "same_image_on_all": true,
            "image_par_ecran": {"UUID-A": "/rust.png"},
            "per_monitor": {"vieux@0,0": "/swift.png"},
            "favoris": ["/rust-fav.png"],
            "slideshow_images": ["/swift-fav.png"],
            "selection_independante": false,
            "slideshow_images_per_monitor": {"vieux@0,0": ["/swift-fav.png"]},
            "diaporama_intervalle": 120,
            "slideshow_interval": 900,
            "diaporama_ecrans": [],
            "slideshow_monitors": ["vieux@0,0"],
            "dossier_par_ecran": {"UUID-A": "/rust-dossier"},
            "monitor_folders": {"vieux@0,0": "/swift-dossier"},
            "largeur_fenetre": 1000.0,
            "window_width": 2000
        })
        .to_string();
        let reglages = lire_texte(&texte);

        assert_eq!(reglages.dossier, "/rust");
        assert_eq!(reglages.tri, "date");
        assert!(!reglages.meme_image_partout);
        assert_eq!(reglages.image_par_ecran.len(), 1);
        assert_eq!(reglages.image_par_ecran["UUID-A"], "/rust.png");
        assert_eq!(reglages.favoris, ["/rust-fav.png"]);
        assert!(!reglages.selection_independante, "valeur Rust explicite");
        assert_eq!(reglages.diaporama_intervalle, 120, "secondes Rust");
        assert_eq!(
            reglages.diaporama_ecrans,
            Some(Vec::new()),
            "Some([]) = aucun ecran, distinct de None"
        );
        assert_eq!(reglages.dossier_par_ecran.len(), 1);
        assert_eq!(reglages.largeur_fenetre, 1000.0);
        // Pas de cle Rust `favoris_par_ecran` : l'ancienne liste dediee sert
        // de repli (mais `selection_independante` reste faux).
        assert_eq!(
            reglages.favoris_par_ecran["vieux@0,0"],
            ["/swift-fav.png"]
        );
        // Les anciennes cles restent preservees, sans etre utilisees.
        assert!(reglages.champs_inconnus.contains_key("folder"));
        assert!(reglages.champs_inconnus.contains_key("per_monitor"));
    }

    #[test]
    fn intervalle_minutes_converti_et_borne() {
        assert_eq!(minutes_vers_secondes(10), 600);
        assert_eq!(minutes_vers_secondes(1), 60);
        assert_eq!(minutes_vers_secondes(0), 60);
        assert_eq!(minutes_vers_secondes(-5), 60);
        assert_eq!(minutes_vers_secondes(1440), 86_400);
        assert_eq!(minutes_vers_secondes(100_000), 86_400);
        assert_eq!(minutes_vers_secondes(i64::MAX), 86_400);

        assert_eq!(
            lire_texte(r#"{"slideshow_interval": 90}"#).diaporama_intervalle,
            5_400
        );
        assert_eq!(
            lire_texte(r#"{"slideshow_interval": 2000}"#).diaporama_intervalle,
            86_400
        );
        assert_eq!(
            lire_texte(r#"{"slideshow_interval": 2}"#).diaporama_intervalle,
            120
        );
    }

    #[test]
    fn participants_vide_absent_ou_aucun() {
        assert_eq!(
            lire_texte(r#"{"slideshow_monitors": []}"#).diaporama_ecrans,
            None
        );
        assert_eq!(
            lire_texte(r#"{"slideshow_monitors": ["vieux@0,0"]}"#).diaporama_ecrans,
            Some(vec!["vieux@0,0".to_string()])
        );
        assert_eq!(lire_texte("{}").diaporama_ecrans, None);
        assert_eq!(
            lire_texte(r#"{"diaporama_ecrans": []}"#).diaporama_ecrans,
            Some(Vec::new())
        );
        assert_eq!(
            lire_texte(r#"{"diaporama_ecrans": null}"#).diaporama_ecrans,
            None
        );
    }

    #[test]
    fn sauvegarde_conserve_les_champs_inconnus() {
        let chemin = chemin_temporaire("inconnus");
        let reglages = lire_texte(&json_swift_complet().to_string());
        sauvegarder_vers(&chemin, &reglages).unwrap();

        let texte = fs::read_to_string(&chemin).unwrap();
        let brut: Value = serde_json::from_str(&texte).unwrap();
        assert_eq!(brut["mode"], json!("zoom"));
        assert_eq!(brut["lock_screen"], json!(true));
        assert_eq!(brut["window_maximized"], json!(true));
        assert_eq!(brut["slideshow_enabled"], json!(true));
        assert_eq!(brut["avif_use_for_gnome"], json!(true));
        assert_eq!(brut["last_seen_version"], json!("1.2.3"));
        assert_eq!(brut["cle_inconnue"], json!({"x": 1}));
        // Les anciennes cles restent telles quelles : rien n'est perdu.
        assert_eq!(brut["per_monitor"]["DELL U2720Q@0,0"], json!("/tmp/a.png"));
        assert_eq!(
            brut["slideshow_images_per_monitor"]["DELL U2720Q@0,0"],
            json!(["/tmp/s1.png"])
        );
        // Les valeurs importees sont ecrites sous les cles Rust.
        assert_eq!(brut["dossier"], json!("/tmp/photos"));
        assert_eq!(brut["diaporama_intervalle"], json!(600));

        let relu = charger_depuis(&chemin);
        assert_eq!(relu.dossier, "/tmp/photos");
        assert_eq!(relu.champs_inconnus["last_seen_version"], json!("1.2.3"));
        assert_eq!(relu.image_par_ecran["DELL U2720Q@0,0"], "/tmp/a.png");

        let _ = fs::remove_file(&chemin);
    }

    #[test]
    fn remappage_exact_collision_et_ecran_inconnu() {
        let mut reglages = Reglages::default();
        reglages
            .image_par_ecran
            .insert("DELL U2720Q@0,0".into(), "/swift.png".into());
        reglages
            .image_par_ecran
            .insert("UUID-A".into(), "/rust.png".into());
        reglages
            .image_par_ecran
            .insert("INCONNU@9,9".into(), "/inconnu.png".into());
        reglages
            .favoris_par_ecran
            .insert("DELL U2720Q@0,0".into(), vec!["/swift-fav.png".into()]);
        reglages
            .dossier_par_ecran
            .insert("DELL U2720Q@0,0".into(), "/swift-dossier".into());
        reglages.diaporama_ecrans = Some(vec![
            "UUID-A".into(),
            "DELL U2720Q@0,0".into(),
            "INCONNU@9,9".into(),
        ]);

        let table = BTreeMap::from([
            ("DELL U2720Q@0,0".to_string(), "UUID-A".to_string()),
            ("AUTRE@1,1".to_string(), "UUID-B".to_string()),
        ]);
        let rapport = remapper_ecrans(&mut reglages, &table);

        // Collision : la valeur Rust (UUID-A) gagne, la source reste presente.
        assert_eq!(reglages.image_par_ecran["UUID-A"], "/rust.png");
        assert_eq!(
            reglages.image_par_ecran["DELL U2720Q@0,0"], "/swift.png",
            "valeur source conservee en cas de collision"
        );
        assert_eq!(rapport.collisions, 2, "image et participant en collision");
        assert_eq!(
            reglages.favoris_par_ecran["UUID-A"],
            ["/swift-fav.png"],
            "liste dediee remappee"
        );
        assert!(!reglages.favoris_par_ecran.contains_key("DELL U2720Q@0,0"));
        assert_eq!(reglages.dossier_par_ecran["UUID-A"], "/swift-dossier");
        assert_eq!(
            reglages.image_par_ecran["INCONNU@9,9"], "/inconnu.png",
            "aucune correspondance = donnee conservee"
        );
        assert_eq!(
            reglages.diaporama_ecrans,
            Some(vec![
                "UUID-A".into(),
                "DELL U2720Q@0,0".into(),
                "INCONNU@9,9".into()
            ]),
            "la valeur source en collision n'est pas perdue"
        );
        assert_eq!(rapport.remappees, 2, "liste de favoris et dossier");
        assert_eq!(rapport.conservees, 4, "UUID-A, 2 x INCONNU, UUID-A participant");
    }

    #[test]
    fn table_correspondances_utilise_nom_et_position_exacts() {
        let ecrans = vec![
            ecran_factice("DELL U2720Q", "UUID-A", 0.0, 0.0),
            ecran_factice("LG", "UUID-B", -1920.4, 12.6),
        ];
        let table = table_correspondances(&ecrans);

        assert_eq!(table.len(), 2);
        assert_eq!(table["DELL U2720Q@0,0"], "UUID-A");
        assert_eq!(table["LG@-1920,13"], "UUID-B");
        assert_eq!(cle_ancienne(&ecrans[1]), "LG@-1920,13");
    }

    #[test]
    fn chargement_avec_ecrans_remappe_sans_ecrire_le_fichier() {
        let chemin = chemin_temporaire("chargement");
        let texte = json!({
            "folder": "/tmp/photos",
            "per_monitor": {
                "DELL U2720Q@0,0": "/tmp/a.png",
                "INCONNU@9,9": "/tmp/inconnu.png"
            },
            "slideshow_images_per_monitor": {
                "DELL U2720Q@0,0": ["/tmp/s1.png"]
            },
            "monitor_folders": {"DELL U2720Q@0,0": "/tmp/photos"},
            "slideshow_monitors": ["DELL U2720Q@0,0"]
        })
        .to_string();
        fs::write(&chemin, &texte).unwrap();

        let ecrans = vec![ecran_factice("DELL U2720Q", "UUID-A", 0.0, 0.0)];
        let reglages = charger_fichier_avec_ecrans(&chemin, &ecrans);

        assert_eq!(reglages.image_par_ecran["UUID-A"], "/tmp/a.png");
        assert!(!reglages.image_par_ecran.contains_key("DELL U2720Q@0,0"));
        assert_eq!(reglages.image_par_ecran["INCONNU@9,9"], "/tmp/inconnu.png");
        assert_eq!(reglages.favoris_par_ecran["UUID-A"], ["/tmp/s1.png"]);
        assert_eq!(reglages.dossier_par_ecran["UUID-A"], "/tmp/photos");
        assert_eq!(reglages.diaporama_ecrans, Some(vec!["UUID-A".to_string()]));

        // Le simple chargement ne doit rien ecrire sur le disque.
        assert_eq!(fs::read_to_string(&chemin).unwrap(), texte);

        let _ = fs::remove_file(&chemin);
    }
}
