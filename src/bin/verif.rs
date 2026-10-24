//! Verification fonctionnelle des actions de l'interface, sans clic humain.
//!
//! Construit l'interface hors ecran, puis envoie lui-meme les messages
//! Objective-C des actions (clic sur une vignette, changement d'ecran,
//! recherche) et controle l'effet sur l'unsafe { &*etat_brut }. Ecrit tout dans le journal.
//!
//! Regle : n'appelle JAMAIS `appliquer:` — cela changerait le fond d'ecran
//! reel de l'utilisateur. Le chemin d'application est verifie a la compilation
//! et par la presence de la cible sur le bouton.
//!
//! Usage : verif [dossier]

use std::path::PathBuf;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DowncastTarget, Message, msg_send};
use objc2_app_kit::{
    NSApplication, NSButton, NSScrollView, NSSearchField, NSSegmentedControl, NSView,
};
use objc2_foundation::NSString;

use mural::journal;

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let dossier = arguments.get(1).map(PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join("Pictures")
    });

    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("verif doit tourner sur le fil principal");
        std::process::exit(1);
    };
    let _application = NSApplication::sharedApplication(mtm);

    let (fenetre, etat_brut) =
        mural::app::construire(mtm, dossier, arguments.iter().any(|a| a == "--progressif"));
    let Some(contenu) = fenetre.contentView() else {
        journal::erreur("verif : pas de vue de contenu");
        std::process::exit(2);
    };

    if arguments.iter().any(|a| a == "--progressif") {
        let limite = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let charges = unsafe { &*etat_brut }
                .vignettes
                .iter()
                .filter(|v| v.is_some())
                .count();
            if charges == unsafe { &*etat_brut }.images.len() {
                break;
            }
            assert!(
                std::time::Instant::now() < limite,
                "chargement progressif incomplet : {charges}"
            );
            objc2_foundation::NSRunLoop::currentRunLoop().runUntilDate(
                &objc2_foundation::NSDate::dateWithTimeIntervalSinceNow(0.02),
            );
        }
        journal::info("verif OK chargement progressif : toutes les vignettes recues via AppKit");
    }

    let mut echecs = 0usize;
    let mut resultat = |nom: &str, ok: bool, detail: String| {
        let verdict = if ok { "OK  " } else { "ECHEC" };
        if !ok {
            echecs += 1;
        }
        journal::info(&format!("verif {verdict} {nom} : {detail}"));
    };

    // Redimensionner doit recalculer la grille et conserver la selection.
    let selection_avant = unsafe { &*etat_brut }.selection;
    for largeur in [920.0, 1440.0] {
        fenetre.setContentSize(objc2_foundation::NSSize {
            width: largeur,
            height: 800.0,
        });
        let reel = unsafe { &*etat_brut };
        resultat(
            "redimensionnement",
            (reel.largeur - largeur).abs() < 30.0 && reel.selection == selection_avant,
            format!("largeur demandee {largeur}, grille {}", reel.largeur),
        );
    }

    // --- 1. La grille a bien ete construite -------------------------------
    let boutons = boutons_grille(&contenu);
    resultat(
        "grille construite",
        !boutons.is_empty(),
        format!(
            "{} vignette(s), {} attendue(s)",
            boutons.len(),
            unsafe { &*etat_brut }.images.len()
        ),
    );

    // --- 2. La cible est retenue par les controles ------------------------
    let cible = boutons.first().and_then(|b| b.target());
    resultat(
        "cible retenue",
        cible.is_some(),
        if cible.is_some() {
            "les actions sont branchees".to_string()
        } else {
            "target nil : aucune action ne se declencherait".to_string()
        },
    );

    // --- 3. Clic sur une vignette ----------------------------------------
    if let Some(cible) = &cible {
        let index_vise = boutons.len().min(3) - 1;
        let bouton = &boutons[index_vise];
        let _: () = unsafe { msg_send![&**cible, celluleCliquee: &**bouton] };
        let attendu = unsafe { &*etat_brut }
            .cellules
            .get(&(&**bouton as *const NSButton as usize))
            .copied();
        resultat(
            "clic sur une vignette",
            attendu.is_some() && unsafe { &*etat_brut }.selection == attendu,
            format!(
                "selection = {:?}, attendu = {:?}",
                unsafe { &*etat_brut }
                    .selection
                    .map(|i| unsafe { &*etat_brut }.images[i].nom.clone()),
                attendu.map(|i| unsafe { &*etat_brut }.images[i].nom.clone())
            ),
        );
    }

    if let Some(cible) = &cible {
        let id = unsafe { &*etat_brut }
            .ecrans
            .get(unsafe { &*etat_brut }.ecran_courant)
            .map(|e| e.identifiant);
        unsafe { &mut *etat_brut }.ecrans.reverse();
        unsafe { &mut *etat_brut }.ecran_courant = unsafe { &*etat_brut }
            .ecrans
            .iter()
            .position(|e| Some(e.identifiant) == id)
            .unwrap_or(0);
        unsafe {
            objc2_foundation::NSNotificationCenter::defaultCenter().postNotificationName_object(
                unsafe { objc2_app_kit::NSApplicationDidChangeScreenParametersNotification },
                None,
            );
        }
        let nouveau = unsafe { &*etat_brut };
        resultat(
            "ecran conserve apres reenumeration",
            nouveau
                .ecrans
                .get(nouveau.ecran_courant)
                .map(|e| e.identifiant)
                == id,
            format!("identifiant {:?}", id),
        );
        if let Some(id) = id {
            assert!(mural::ecrans::retrouver_identifiant(id).is_some());
        }
        let _ = cible;
    }

    // --- 4. Changement d'ecran -------------------------------------------
    if let (Some(cible), Some(segments)) = (&cible, controle::<NSSegmentedControl>(&contenu)) {
        let vise = if unsafe { &*etat_brut }.ecrans.len() > 1 {
            1
        } else {
            0
        };
        unsafe { segments.setSelectedSegment(vise as isize) };
        let _: () = unsafe { msg_send![&**cible, ecranChange: &*segments] };
        resultat(
            "changement d'ecran",
            unsafe { &mut *etat_brut }.ecran_courant == vise as usize,
            format!(
                "ecran courant = {}",
                unsafe { &*etat_brut }.ecran_courant + 1
            ),
        );
    }

    // --- 5. Recherche -----------------------------------------------------
    if let (Some(cible), Some(champ)) = (&cible, controle::<NSSearchField>(&contenu)) {
        let avant = unsafe { &*etat_brut }.cellules.len();
        let motif = unsafe { &*etat_brut }
            .images
            .first()
            .map(|i| i.nom.chars().take(3).collect::<String>())
            .unwrap_or_else(|| "zzz".to_string());
        unsafe { champ.setStringValue(&NSString::from_str(&motif)) };
        let _: () = unsafe { msg_send![&**cible, rechercher: &*champ] };
        let apres = unsafe { &*etat_brut }.cellules.len();
        let attendus = unsafe { &*etat_brut }
            .images
            .iter()
            .filter(|i| i.nom.to_lowercase().contains(&motif.to_lowercase()))
            .count();
        resultat(
            "recherche",
            apres == attendus && apres != avant,
            format!("« {motif} » : {avant} -> {apres} vignette(s), {attendus} attendue(s)"),
        );
    }

    // --- 6. Journal -------------------------------------------------------
    let chemin_journal = journal::chemin()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(indisponible)".to_string());
    println!("journal : {chemin_journal}");
    println!(
        "verifications : {} echec(s) sur {}",
        echecs,
        if echecs == 0 {
            "toutes OK"
        } else {
            "voir le journal"
        }
    );
    if echecs > 0 {
        std::process::exit(3);
    }
}

/// Boutons de vignettes : les sous-vues du document du `NSScrollView`.
fn boutons_grille(contenu: &NSView) -> Vec<Retained<NSButton>> {
    let Some(defilement) = controle::<NSScrollView>(contenu) else {
        return Vec::new();
    };
    let Some(document) = (unsafe { defilement.documentView() }) else {
        return Vec::new();
    };
    let sous_vues = unsafe { document.subviews() };
    let mut boutons = Vec::new();
    for index in 0..sous_vues.count() {
        let vue = unsafe { sous_vues.objectAtIndex(index) };
        if let Some(bouton) = vue.downcast_ref::<NSButton>().map(|b| b.retain()) {
            boutons.push(bouton);
        }
    }
    boutons
}

/// Premier controle d'un type donne, trouve en descendant la hierarchie sur
/// deux niveaux (contenu puis barre d'outils).
fn controle<T: Message + DowncastTarget>(contenu: &NSView) -> Option<Retained<T>> {
    let premier_niveau = unsafe { contenu.subviews() };
    for index in 0..premier_niveau.count() {
        let vue = unsafe { premier_niveau.objectAtIndex(index) };
        if let Some(trouve) = vue.downcast_ref::<T>() {
            return Some(trouve.retain());
        }
        let enfants = unsafe { vue.subviews() };
        for sous_index in 0..enfants.count() {
            let enfant = unsafe { enfants.objectAtIndex(sous_index) };
            if let Some(trouve) = enfant.downcast_ref::<T>() {
                return Some(trouve.retain());
            }
        }
    }
    None
}

#[allow(dead_code)]
fn _types(_objet: &AnyObject) {}
