//! Galerie volumineuse et defilement, sans aucune application de fond.
use objc2::{MainThreadMarker, Message, msg_send};
use objc2_app_kit::{NSApplication, NSButton, NSEventMask, NSScrollView, NSSearchField, NSView};
use objc2_foundation::{NSDate, NSPoint, NSSize, NSString};
use std::time::{Duration, Instant};

fn trouver(vue: &NSView) -> Option<objc2::rc::Retained<NSScrollView>> {
    if let Some(v) = vue.downcast_ref::<NSScrollView>() {
        return Some(v.retain());
    }
    vue.subviews().iter().find_map(|v| trouver(&v))
}
fn recherche(vue: &NSView) -> Option<objc2::rc::Retained<NSSearchField>> {
    if let Some(v) = vue.downcast_ref::<NSSearchField>() {
        return Some(v.retain());
    }
    vue.subviews().iter().find_map(|v| recherche(&v))
}
fn attendre(app: &NSApplication, secondes: f64) {
    let fin = Instant::now() + Duration::from_secs_f64(secondes);
    while Instant::now() < fin {
        objc2::rc::autoreleasepool(|_| {
            if let Some(e) = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                Some(&NSDate::dateWithTimeIntervalSinceNow(0.02)),
                unsafe { objc2_foundation::NSDefaultRunLoopMode },
                true,
            ) {
                app.sendEvent(&e);
            }
            app.updateWindows();
        });
    }
}
fn main() {
    let base = std::env::var("MURAL_REGLAGES").expect("reglages isoles obligatoires");
    assert!(base.starts_with("/private/tmp/"));
    let base = std::path::Path::new(&base).parent().unwrap();
    mural::preferences::sauvegarder(&mural::preferences::Reglages {
        largeur_fenetre: 1180.0,
        hauteur_fenetre: 760.0,
        ..Default::default()
    })
    .unwrap();
    let dossier = base.join("images");
    std::fs::create_dir_all(&dossier).unwrap();
    let source = dossier.join("image-000.png");
    if !source.exists() {
        image::RgbImage::from_fn(720, 440, |x, y| {
            image::Rgb([x as u8, y as u8, (x + y) as u8])
        })
        .save(&source)
        .unwrap();
        for i in 1..500 {
            std::fs::copy(&source, dossier.join(format!("image-{i:03}.png"))).unwrap();
        }
    }
    let mtm = MainThreadMarker::new().unwrap();
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Regular);
    app.finishLaunching();
    let (fenetre, etat) =
        objc2::rc::autoreleasepool(|_| mural::app::construire(mtm, dossier, true));
    attendre(&app, 6.0);
    let nombre = || unsafe { &*etat }.vignettes.iter().flatten().count();
    println!(
        "IMAGES {} MINIATURES_CHARGEES {}",
        unsafe { &*etat }.images.len(),
        nombre()
    );
    let mesure = std::process::Command::new("footprint")
        .args(["-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    print!("{}", String::from_utf8_lossy(&mesure.stdout));
    assert!(
        nombre() > 0 && nombre() < 100,
        "les miniatures hors champ restent en memoire : {}",
        nombre()
    );
    let scroll = trouver(&fenetre.contentView().unwrap()).unwrap();
    let document = scroll.documentView().unwrap();
    for y in [2000.0_f64, 6000.0, 12000.0, 0.0] {
        let y = y.min((document.frame().size.height - scroll.contentSize().height).max(0.0));
        objc2::rc::autoreleasepool(|_| {
            scroll.contentView().scrollToPoint(NSPoint::new(0.0, y));
            scroll.reflectScrolledClipView(&scroll.contentView());
        });
        attendre(&app, 1.0);
        let etat = unsafe { &*etat };
        let visibles: Vec<_> = etat
            .cellules
            .iter()
            .filter(|(adresse, _)| {
                let bouton = unsafe { &*(**adresse as *const NSButton) };
                let a = bouton.frame();
                let b = document.visibleRect();
                a.origin.y < b.origin.y + b.size.height && a.origin.y + a.size.height > b.origin.y
            })
            .collect();
        assert!(!visibles.is_empty());
        for (adresse, index) in visibles {
            assert!(
                etat.vignettes[*index].is_some(),
                "image visible absente : {index}"
            );
            assert!(unsafe { &*(*adresse as *const NSButton) }.image().is_some());
        }
        assert!(nombre() < 100, "accumulation au defilement");
        println!("DEFILEMENT {y} MINIATURES_CHARGEES {}", nombre());
    }
    let adresse = *unsafe { &*etat }.cellules.keys().next().unwrap();
    let cible = unsafe { &*(adresse as *const NSButton) }.target().unwrap();
    objc2::rc::autoreleasepool(|_| unsafe {
        let _: () =
            msg_send![&*cible, redessinerGrille: std::ptr::null::<objc2_foundation::NSObject>()];
    });
    attendre(&app, 1.0);
    assert!(nombre() < 100);
    let champ = recherche(&fenetre.contentView().unwrap()).unwrap();
    for filtre in ["image-499", "absent", ""] {
        objc2::rc::autoreleasepool(|_| {
            champ.setStringValue(&NSString::from_str(filtre));
            unsafe {
                let _: () = msg_send![&*cible, rechercher: &*champ];
            }
        });
        attendre(&app, 0.5);
        if filtre == "image-499" {
            assert_eq!(unsafe { &*etat }.cellules.len(), 1);
            assert!(unsafe { &*etat }.vignettes[499].is_some());
        } else if filtre == "absent" {
            assert_eq!(nombre(), 0);
        } else {
            assert!(nombre() > 0 && nombre() < 100);
        }
    }
    objc2::rc::autoreleasepool(|_| fenetre.setContentSize(NSSize::new(900.0, 600.0)));
    attendre(&app, 1.0);
    assert!(nombre() > 0 && nombre() < 100);
    let mesure = std::process::Command::new("footprint")
        .args(["-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    print!(
        "APRES PARCOURS\n{}",
        String::from_utf8_lossy(&mesure.stdout)
    );
    fenetre.close();
    println!("OK : 500 images, chargement borne, defilement et retour sans image manquante");
}
