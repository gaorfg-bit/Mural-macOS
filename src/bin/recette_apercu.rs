//! Apercu reel : reutilisation, changement de source et disparition.
//! Toutes les images sont generees dans un dossier temporaire neuf.
use objc2::{MainThreadMarker, msg_send, rc::Retained};
use objc2_app_kit::{NSApplication, NSButton, NSImage, NSImageView, NSTextField, NSView};
use objc2_foundation::{NSDate, NSObject};
use std::{path::Path, time::Instant};

fn trouver<T: objc2::Message + objc2::DowncastTarget>(vue: &NSView) -> Option<Retained<T>> {
    if let Some(v) = vue.downcast_ref::<T>() {
        return Some(v.retain());
    }
    vue.subviews().iter().find_map(|v| trouver::<T>(&v))
}
fn contient_texte(vue: &NSView, texte: &str) -> bool {
    vue.downcast_ref::<NSTextField>()
        .is_some_and(|v| v.stringValue().to_string().contains(texte))
        || vue.subviews().iter().any(|v| contient_texte(&v, texte))
}
fn ecrire(p: &Path, l: u32, h: u32) {
    image::RgbImage::from_fn(l, h, |x, y| image::Rgb([x as u8, y as u8, (x ^ y) as u8]))
        .save(p)
        .unwrap();
}
fn option(cible: &NSObject) -> f64 {
    let debut = Instant::now();
    objc2::rc::autoreleasepool(|_| {
        let _: () = unsafe { msg_send![cible, memeImagePartout: cible] };
    });
    debut.elapsed().as_secs_f64() * 1000.0
}
fn meme(a: &NSImage, b: &NSImage) -> bool {
    std::ptr::eq(a, b)
}
fn main() {
    if std::env::args().any(|a| a == "--journal") {
        let chemin = mural::journal::chemin().expect("journal isole obligatoire");
        assert!(chemin.starts_with("/private/tmp/"));
        let mut durees = Vec::new();
        for _ in 0..100 {
            let debut = Instant::now();
            mural::journal::info("mesure synchrone du journal de recette");
            durees.push(debut.elapsed().as_secs_f64() * 1_000_000.0);
        }
        durees.sort_by(f64::total_cmp);
        println!(
            "JOURNAL_US_MEDIANE {:.3} MIN {:.3} MAX {:.3} OCTETS {}",
            (durees[49] + durees[50]) / 2.0,
            durees[0],
            durees[99],
            std::fs::metadata(chemin).unwrap().len()
        );
        return;
    }
    let origine = Instant::now();
    let avif = std::env::args().any(|a| a == "--avif");
    if avif && !mural::formats::avif_encodable() {
        println!("NON VERIFIE : encodeur AVIF absent, profil grande image non execute");
        return;
    }
    let reglages = std::path::PathBuf::from(
        std::env::var_os("MURAL_REGLAGES").expect("reglages isoles obligatoires"),
    );
    assert!(reglages.starts_with("/private/tmp/"));
    let base = reglages.parent().unwrap();
    let dossier = base.join("images");
    assert!(!dossier.exists(), "dossier neuf obligatoire");
    std::fs::create_dir_all(&dossier).unwrap();
    if avif {
        println!(
            "PHASE generation_avif t={:.3} epoch={:.6}",
            origine.elapsed().as_secs_f64(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs_f64()
        );
        let source = base.join("source.png");
        ecrire(&source, 8192, 4096);
        let conversion = objc2::rc::autoreleasepool(|_| {
            mural::avif::convertir(&source, &base.join("conversion"))
        })
        .unwrap();
        std::fs::copy(conversion.chemin, dossier.join("a.avif")).unwrap();
        println!(
            "PHASE lecture_avif t={:.3}",
            origine.elapsed().as_secs_f64()
        );
    } else {
        ecrire(&dossier.join("a.png"), 6400, 2624);
    }
    ecrire(&dossier.join("b.png"), 1600, 1600);
    let mtm = MainThreadMarker::new().unwrap();
    let _app = NSApplication::sharedApplication(mtm);
    let debut = Instant::now();
    let (fenetre, etat) =
        objc2::rc::autoreleasepool(|_| mural::app::construire(mtm, dossier.clone(), false));
    println!(
        "CONSTRUCTION_MS {:.3}",
        debut.elapsed().as_secs_f64() * 1000.0
    );
    let contenu = fenetre.contentView().unwrap();
    let vue = trouver::<NSImageView>(&contenu).expect("vue d'apercu");
    let premiere = vue.image().expect("apercu initial");
    let adresse = *unsafe { &*etat }.cellules.keys().next().unwrap();
    let cible = unsafe { &*(adresse as *const NSButton) }
        .target()
        .unwrap()
        .downcast::<NSObject>()
        .unwrap();
    let mut durees = Vec::new();
    for _ in 0..6 {
        durees.push(option(&cible));
    }
    let reutilisee = meme(&premiere, &vue.image().unwrap());
    durees.sort_by(f64::total_cmp);
    println!(
        "OPTIONS_MS_MEDIANE {:.3} MIN {:.3} MAX {:.3} REUTILISE {}",
        (durees[2] + durees[3]) / 2.0,
        durees[0],
        durees[5],
        reutilisee
    );
    if !std::env::args().any(|a| a == "--mesure") {
        assert!(
            reutilisee,
            "un changement d'option redecode l'apercu inchange"
        );
    }
    if avif {
        assert!(contient_texte(&contenu, "8192 x 4096 px"));
        println!(
            "PHASE avif_repos_debut t={:.3}",
            origine.elapsed().as_secs_f64()
        );
        let fin = Instant::now() + std::time::Duration::from_secs(10);
        while let Some(reste) = fin.checked_duration_since(Instant::now()) {
            if let Some(e) = _app.nextEventMatchingMask_untilDate_inMode_dequeue(
                objc2_app_kit::NSEventMask::Any,
                Some(&NSDate::dateWithTimeIntervalSinceNow(reste.as_secs_f64())),
                unsafe { objc2_foundation::NSDefaultRunLoopMode },
                true,
            ) {
                _app.sendEvent(&e);
            }
            _app.updateWindows();
        }
        println!(
            "PHASE avif_repos_fin t={:.3}",
            origine.elapsed().as_secs_f64()
        );
        fenetre.close();
        println!(
            "OK : AVIF 8192 x 4096, apercu froid puis six reutilisations ; aucun fond applique"
        );
        return;
    }
    // Le fichier change au meme emplacement : nouvelle taille et nouvelles dimensions.
    ecrire(&dossier.join("a.png"), 3200, 1200);
    option(&cible);
    let modifiee = vue.image().unwrap();
    assert!(
        !meme(&premiere, &modifiee),
        "ancien contenu conserve apres modification"
    );
    assert!(
        contient_texte(&contenu, "3200 x 1200 px"),
        "dimensions non actualisees"
    );
    let (adresse, _) = unsafe { &*etat }
        .cellules
        .iter()
        .find(|(_, i)| **i == 1)
        .unwrap();
    let bouton = unsafe { &*(*adresse as *const NSButton) };
    let _: () = unsafe { msg_send![&*cible, celluleCliquee: bouton] };
    let seconde = vue.image().unwrap();
    assert!(!meme(&seconde, &modifiee));
    assert_eq!(
        seconde.size().width,
        seconde.size().height,
        "mauvais apercu apres selection"
    );
    let absente = dossier.join("b.indisponible");
    std::fs::rename(dossier.join("b.png"), &absente).unwrap();
    option(&cible);
    assert!(
        vue.image().is_none(),
        "apercu obsolete apres disparition du fichier"
    );
    std::fs::rename(&absente, dossier.join("b.png")).unwrap();
    option(&cible);
    assert!(
        vue.image().is_some(),
        "apercu non recharge apres retour du fichier"
    );
    unsafe { &mut *etat }.selection = None;
    option(&cible);
    assert!(vue.image().is_none(), "apercu conserve sans selection");
    fenetre.close();
    if std::env::args().any(|a| a == "--mesure") {
        println!(
            "OK : mesure et invalidations exercees ; reutilisation rapportee sans assertion ; aucun fond applique"
        );
    } else {
        println!(
            "OK : apercu reutilise si inchange, invalide apres modification, selection, disparition et absence de selection ; aucun fond applique"
        );
    }
}
