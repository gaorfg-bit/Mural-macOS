//! Construction de la fenetre complete et point d'entree AppKit.
use super::*;

/// Construit la fenetre complete. `afficher` la rend visible (le rendu hors
/// ecran la laisse cachee).
pub fn construire(
    mtm: MainThreadMarker,
    dossier: PathBuf,
    afficher: bool,
) -> (Retained<NSWindow>, *mut Etat) {
    let debut = std::time::Instant::now();
    journal::demarrage();
    let cible = Cible::nouvelle(mtm);
    let etat = Box::into_raw(Box::new(Etat::nouveau(dossier, !afficher)));
    cible.definir_etat(etat);
    if !afficher {
        // Les outils hors ecran n'executent pas la boucle AppKit : attendre
        // explicitement leurs images permet un rendu reproductible.
        let etat = unsafe { &mut *etat };
        if let Some(reception) = etat.reception.take() {
            for (index, vignette) in reception {
                etat.vignettes_en_attente[index] = false;
                etat.vignettes[index] = vignette.map(|v| {
                    NSImage::initWithCGImage_size(
                        NSImage::alloc(),
                        &v.image,
                        NSSize {
                            width: v.largeur as f64,
                            height: v.hauteur as f64,
                        },
                    )
                });
            }
        }
    }

    let (largeur, hauteur) = unsafe { ((*etat).largeur, (*etat).hauteur) };
    let cadre = NSRect {
        origin: NSPoint { x: 0.0, y: 0.0 },
        size: NSSize {
            width: largeur,
            height: hauteur,
        },
    };
    let fenetre = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            cadre,
            NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable
                | NSWindowStyleMask::Resizable
                | NSWindowStyleMask::FullSizeContentView,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe {
        fenetre.setReleasedWhenClosed(false);
        fenetre.setTitle(&NSString::from_str("Mural"));
        fenetre.setTitleVisibility(objc2_app_kit::NSWindowTitleVisibility::Hidden);
        fenetre.setTitlebarAppearsTransparent(true);
        fenetre.setAcceptsMouseMovedEvents(true);
        fenetre.setContentMinSize(NSSize {
            width: 880.0,
            height: 520.0,
        });
        fenetre.center();
    }

    let contenu = interface::Racine::new(mtm);
    unsafe {
        contenu.setFrame(cadre);
        fenetre.setContentView(Some(&contenu));
    }

    // --- Barre d'outils -----------------------------------------------------
    let barre = NSView::new(mtm);
    unsafe {
        barre.setFrame(NSRect {
            origin: NSPoint {
                x: 0.0,
                y: hauteur - HAUTEUR_BARRE,
            },
            size: NSSize {
                width: largeur,
                height: HAUTEUR_BARRE,
            },
        });
        contenu.addSubview(&barre);
    }

    // Bouton dossier : action branchee a la creation (target non retenu).
    let bouton_dossier = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(""),
            Some(&cible),
            Some(sel!(ouvrirDossier:)),
            mtm,
        )
    };
    unsafe {
        bouton_dossier.setBordered(false);
        bouton_dossier.setImage(Some(&symbole("folder")));
        bouton_dossier.setToolTip(Some(&NSString::from_str("Open folder")));
        bouton_dossier.setFrame(NSRect {
            origin: NSPoint { x: 14.0, y: 9.0 },
            size: NSSize {
                width: 30.0,
                height: 28.0,
            },
        });
        barre.addSubview(&bouton_dossier);
    }

    let bouton_favori = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(""),
            Some(&cible),
            Some(sel!(dossierFavori:)),
            mtm,
        )
    };
    let bouton_bibliotheque = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(""),
            Some(&cible),
            Some(sel!(dossiersFavoris:)),
            mtm,
        )
    };
    for (bouton, x, icone, aide) in [
        (
            &bouton_favori,
            48.0,
            "star",
            "Toggle favorite folder",
        ),
        (
            &bouton_bibliotheque,
            82.0,
            "books.vertical",
            "Favorite folders",
        ),
    ] {
        bouton.setBordered(false);
        bouton.setImage(Some(&symbole(icone)));
        bouton.setToolTip(Some(&NSString::from_str(aide)));
        bouton.setFrame(NSRect {
            origin: NSPoint { x, y: 9.0 },
            size: NSSize {
                width: 30.0,
                height: 28.0,
            },
        });
        barre.addSubview(bouton);
    }

    // Champ de recherche : action validee par Entree.
    let recherche = NSSearchField::new(mtm);
    unsafe {
        recherche.setFrame(NSRect {
            origin: NSPoint { x: 120.0, y: 9.0 },
            size: NSSize {
                width: 190.0,
                height: 28.0,
            },
        });
    }
    unsafe {
        recherche.setTarget(Some(&cible));
        recherche.setAction(Some(sel!(rechercher:)));
        recherche.setPlaceholderString(Some(&NSString::from_str("Search")));
        barre.addSubview(&recherche);
    }

    // Selection d'ecran : compacte, centree.
    let etiquettes: Vec<Retained<NSString>> = unsafe { &*etat }
        .ecrans
        .iter()
        .map(|e| NSString::from_str(&format!("Display {}", e.index + 1)))
        .collect();
    let liste = NSArray::from_retained_slice(&etiquettes);
    let segments = unsafe {
        NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
            &liste,
            NSSegmentSwitchTracking::SelectOne,
            Some(&cible),
            Some(sel!(ecranChange:)),
            mtm,
        )
    };
    let largeur_segments = 168.0;
    unsafe {
        segments.setFrame(NSRect {
            origin: NSPoint {
                x: (largeur - largeur_segments) / 2.0,
                y: 9.0,
            },
            size: NSSize {
                width: largeur_segments,
                height: 28.0,
            },
        });
        segments.setSelectedSegment(unsafe { &*etat }.ecran_courant as isize);
        barre.addSubview(&segments);
    }

    // Bouton principal.
    let bouton_appliquer = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str("Apply"),
            Some(&cible),
            Some(sel!(appliquer:)),
            mtm,
        )
    };
    unsafe {
        bouton_appliquer.setBezelStyle(NSBezelStyle::Rounded);
        bouton_appliquer.setFrame(NSRect {
            origin: NSPoint {
                x: largeur - 148.0,
                y: 8.0,
            },
            size: NSSize {
                width: 134.0,
                height: 30.0,
            },
        });
        barre.addSubview(&bouton_appliquer);
    }

    let bouton_diaporama = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(""),
            Some(&cible),
            Some(sel!(basculerDiaporama:)),
            mtm,
        )
    };
    let bouton_suivante = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(""),
            Some(&cible),
            Some(sel!(suivanteDiaporama:)),
            mtm,
        )
    };
    for (bouton, distance, icone, aide) in [
        (
            &bouton_diaporama,
            256.0,
            "play.fill",
            "Start slideshow",
        ),
        (
            &bouton_suivante,
            220.0,
            "forward.end.fill",
            "Next slideshow image",
        ),
    ] {
        bouton.setBordered(false);
        bouton.setImage(Some(&symbole(icone)));
        bouton.setToolTip(Some(&NSString::from_str(aide)));
        bouton.setFrame(NSRect {
            origin: NSPoint {
                x: largeur - distance,
                y: 9.0,
            },
            size: NSSize {
                width: 30.0,
                height: 28.0,
            },
        });
        bouton.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
        barre.addSubview(bouton);
    }

    // Menu « ... » : entrees branchees sur la meme cible.
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    let bouton_menu = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(""),
            Some(&cible),
            Some(sel!(menuOuvrir:)),
            mtm,
        )
    };
    unsafe {
        bouton_menu.setBordered(false);
        bouton_menu.setImage(Some(&symbole("ellipsis.circle")));
        bouton_menu.setToolTip(Some(&NSString::from_str("More options")));
        bouton_menu.setFrame(NSRect {
            origin: NSPoint {
                x: largeur - 184.0,
                y: 9.0,
            },
            size: NSSize {
                width: 30.0,
                height: 28.0,
            },
        });
        barre.addSubview(&bouton_menu);
    }

    // --- Apercu panoramique -------------------------------------------------
    let cadre_apercu = NSRect {
        origin: NSPoint {
            x: 0.0,
            y: hauteur - HAUTEUR_BARRE - HAUTEUR_APERCU,
        },
        size: NSSize {
            width: largeur,
            height: HAUTEUR_APERCU,
        },
    };
    let fond_apercu = NSImageView::new(mtm);
    let apercu = interface::Apercu::new(mtm);
    unsafe {
        fond_apercu.setFrame(cadre_apercu);
        fond_apercu.setImageScaling(NSImageScaling::ScaleNone);
        fond_apercu.setWantsLayer(true);
        if let Some(couche) = fond_apercu.layer() {
            couche.setBackgroundColor(Some(&NSColor::underPageBackgroundColor().CGColor()));
        }
        contenu.addSubview(&fond_apercu);

        apercu.setFrame(cadre_apercu);
        apercu.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        contenu.addSubview(&apercu);
    }

    // --- Grille -------------------------------------------------------------
    let haut_grille = hauteur - HAUTEUR_BARRE - HAUTEUR_APERCU;
    let cadre_grille = NSRect {
        origin: NSPoint {
            x: 0.0,
            y: HAUTEUR_ETAT,
        },
        size: NSSize {
            width: largeur,
            height: haut_grille - HAUTEUR_ETAT,
        },
    };
    let defilement = unsafe { NSScrollView::new(mtm) };
    let document = VueRetournee::new(mtm);
    unsafe {
        defilement.setDocumentView(Some(&document));
        defilement.setFrame(cadre_grille);
        defilement.setHasVerticalScroller(true);
        defilement.setScrollerStyle(objc2_app_kit::NSScrollerStyle::Overlay);
        defilement.setDrawsBackground(true);
        defilement.setBackgroundColor(&NSColor::windowBackgroundColor());
        contenu.addSubview(&defilement);
    }

    // --- Barre d'etat -------------------------------------------------------
    let etat_texte =
        unsafe { NSTextField::labelWithString(&NSString::from_str("Select an image"), mtm) };
    let nom_fichier = unsafe { NSTextField::labelWithString(&NSString::from_str(""), mtm) };
    let dimensions = unsafe { NSTextField::labelWithString(&NSString::from_str(""), mtm) };
    unsafe {
        etat_texte.setFrame(NSRect {
            origin: NSPoint { x: 12.0, y: 8.0 },
            size: NSSize {
                width: largeur * 0.42,
                height: 18.0,
            },
        });
        nom_fichier.setFrame(NSRect {
            origin: NSPoint {
                x: largeur * 0.45,
                y: 8.0,
            },
            size: NSSize {
                width: largeur * 0.28,
                height: 18.0,
            },
        });
        nom_fichier.setFont(Some(&NSFont::boldSystemFontOfSize(11.0)));
        dimensions.setFrame(NSRect {
            origin: NSPoint {
                x: largeur * 0.74,
                y: 8.0,
            },
            size: NSSize {
                width: largeur * 0.25,
                height: 18.0,
            },
        });
        for vue in [&etat_texte, &nom_fichier, &dimensions] {
            vue.setBezeled(false);
            vue.setEditable(false);
            vue.setSelectable(false);
            vue.setDrawsBackground(false);
            vue.setTextColor(Some(&NSColor::secondaryLabelColor()));
            contenu.addSubview(vue);
        }
    }

    // AppKit maintient les ancrages ; la notification recalcule les colonnes.
    let largeur_flexible = NSAutoresizingMaskOptions::ViewWidthSizable;
    let haut_fixe = largeur_flexible | NSAutoresizingMaskOptions::ViewMinYMargin;
    barre.setAutoresizingMask(haut_fixe);
    fond_apercu.setAutoresizingMask(haut_fixe);
    apercu.setAutoresizingMask(haut_fixe);
    defilement.setAutoresizingMask(largeur_flexible | NSAutoresizingMaskOptions::ViewHeightSizable);
    segments.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMaxXMargin,
    );
    etat_texte.setAutoresizingMask(largeur_flexible);
    bouton_appliquer.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
    bouton_menu.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
    nom_fichier.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
    dimensions.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);

    // Composition validee : deux surfaces et des commandes flottantes.
    // Le split natif gere le drag sans minuterie ; la jonction reste invisible.
    let separation = interface::Separation::new(mtm);
    separation.setFrame(contenu.bounds());
    separation.setVertical(false);
    separation.setAutoresizingMask(largeur_flexible | NSAutoresizingMaskOptions::ViewHeightSizable);
    fond_apercu.removeFromSuperview();
    apercu.removeFromSuperview();
    defilement.removeFromSuperview();
    apercu.setFrame(NSRect::new(NSPoint::ZERO, NSSize::new(largeur, hauteur * 0.48)));
    defilement.setFrame(NSRect::new(NSPoint::ZERO, NSSize::new(largeur, hauteur * 0.52 - 1.0)));
    separation.addSubview(&apercu);
    separation.addSubview(&defilement);
    separation.adjustSubviews();
    separation.setPosition_ofDividerAtIndex(contenu.bounds().size.height * 0.48, 0);
    contenu.addSubview(&separation);
    // Reconstruire la grille aussi pendant le redimensionnement de l'aperçu.
    unsafe {
        objc2_foundation::NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &cible, sel!(fenetreRedimensionnee:),
            Some(objc2_app_kit::NSSplitViewDidResizeSubviewsNotification), Some(&separation));
    }
    let largeur_outils = (largeur_segments + 416.0).max(500.0);
    let hauteur_surface = contenu.bounds().size.height;
    let cadre_outils = NSRect::new(NSPoint::new((largeur-largeur_outils)/2.0, hauteur_surface-54.0), NSSize::new(largeur_outils, 42.0));
    let outils = interface::panneau(mtm, cadre_outils);
    outils.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMaxXMargin | NSAutoresizingMaskOptions::ViewMinYMargin);
    barre.removeFromSuperview();
    barre.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
    interface::remplir(&outils, &barre);
    contenu.addSubview(&outils);
    recherche.setFrame(NSRect::new(NSPoint::new(8.0, 7.0), NSSize::new(150.0, 28.0)));
    segments.setFrame(NSRect::new(NSPoint::new(164.0, 7.0), NSSize::new(largeur_segments, 28.0)));
    segments.setAutoresizingMask(NSAutoresizingMaskOptions::empty());
    let x = 170.0 + largeur_segments;
    bouton_appliquer.setFrame(NSRect::new(NSPoint::new(x, 6.0), NSSize::new(134.0, 30.0)));
    bouton_diaporama.setFrame(NSRect::new(NSPoint::new(x+140.0, 7.0), NSSize::new(28.0, 28.0)));
    bouton_suivante.setHidden(true); // retenu par la barre ; accessible par le menu Diaporama
    bouton_menu.setFrame(NSRect::new(NSPoint::new(largeur_outils-36.0, 7.0), NSSize::new(28.0, 28.0)));
    let details_bouton = unsafe { NSButton::buttonWithTitle_target_action(
        &NSString::from_str(""), Some(&cible), Some(sel!(afficherDetails:)), mtm) };
    details_bouton.setBordered(false);
    details_bouton.setImage(Some(&symbole("info.circle")));
    details_bouton.setToolTip(Some(&NSString::from_str("Image details")));
    details_bouton.setFrame(NSRect::new(NSPoint::new(largeur_outils-68.0, 7.0), NSSize::new(28.0,28.0)));
    barre.addSubview(&details_bouton);

    let dossiers = interface::panneau(mtm,
        NSRect::new(NSPoint::new(8.0, hauteur_surface-176.0), NSSize::new(184.0, 112.0)));
    dossiers.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    let dossiers_contenu = NSView::new(mtm);
    interface::remplir(&dossiers, &dossiers_contenu);
    for (bouton, y, titre) in [(&bouton_dossier, 72.0, "Open folder…"),
        (&bouton_bibliotheque, 40.0, "Favorite folders"), (&bouton_favori, 8.0, "Favorite this folder")] {
        bouton.removeFromSuperview();
        bouton.setTitle(&NSString::from_str(titre));
        bouton.setImagePosition(objc2_app_kit::NSCellImagePosition::ImageLeft);
        bouton.setFrame(NSRect::new(NSPoint::new(8.0, y), NSSize::new(168.0,28.0)));
        dossiers_contenu.addSubview(bouton);
    }
    contenu.addSubview(&dossiers);
    let details = interface::panneau(mtm,
        NSRect::new(NSPoint::new(largeur-382.0, 12.0), NSSize::new(370.0,88.0)));
    details.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
    let details_contenu = NSView::new(mtm);
    interface::remplir(&details, &details_contenu);
    for (vue, y) in [(&nom_fichier, 60.0), (&dimensions, 36.0), (&etat_texte, 12.0)] {
        vue.removeFromSuperview();
        vue.setFrame(NSRect::new(NSPoint::new(12.0,y),NSSize::new(346.0,18.0)));
        vue.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
        details_contenu.addSubview(vue);
    }
    details.setHidden(true);
    contenu.addSubview(&details);
    contenu.panneaux(&outils, &dossiers);
    *cible.ivars().surface.borrow_mut() = Some(contenu.clone());
    *cible.ivars().details.borrow_mut() = Some(details);

    // Les vues existent : on les branche.
    cible.definir_vues(
        &apercu,
        &etat_texte,
        &nom_fichier,
        &dimensions,
        &bouton_appliquer,
        &document,
        &defilement,
        &recherche,
        &menu,
    );

    defilement.contentView().setPostsBoundsChangedNotifications(true);
    unsafe {
        objc2_foundation::NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &cible, sel!(zoneVisibleModifiee:),
            Some(objc2_app_kit::NSViewBoundsDidChangeNotification), Some(&defilement.contentView()));
    }
    unsafe {
        objc2_foundation::NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &cible,
            sel!(fenetreRedimensionnee:),
            Some(objc2_app_kit::NSWindowDidResizeNotification),
            Some(&fenetre),
        );
    }
    unsafe {
        let ivars = cible.ivars();
        ivars.segments.set(&*segments as *const NSSegmentedControl as *mut NSSegmentedControl);
        ivars.fenetre.set(&*fenetre as *const NSWindow as *mut NSWindow);
        ivars.favori.set(&*bouton_favori as *const NSButton as *mut NSButton);
        ivars.bouton_diaporama.set(&*bouton_diaporama as *const NSButton as *mut NSButton);
        ivars.bouton_suivante.set(&*bouton_suivante as *const NSButton as *mut NSButton);
        for nom in [objc2_app_kit::NSWindowWillCloseNotification] {
            objc2_foundation::NSNotificationCenter::defaultCenter()
                .addObserver_selector_name_object(
                    &cible,
                    sel!(enregistrerEtat:),
                    Some(nom),
                    Some(&fenetre),
                );
        }
        objc2_foundation::NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &cible,
            sel!(enregistrerEtat:),
            Some(objc2_app_kit::NSApplicationWillTerminateNotification),
            None,
        );
        objc2_foundation::NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &cible,
            sel!(ecransModifies:),
            Some(objc2_app_kit::NSApplicationDidChangeScreenParametersNotification),
            None,
        );
    }
    cible.actualiser_menu();
    cible.installer_menu_principal();
    NSApplication::sharedApplication(mtm).setDelegate(Some(ProtocolObject::from_ref(&*cible)));
    cible.rafraichir_grille();
    cible.rafraichir();

    // AppKit ne retient pas le target d'un NSControl : sans cette ligne,
    // aucune action ne se declenche jamais.
    std::mem::forget(cible);

    if afficher {
        unsafe { fenetre.makeKeyAndOrderFront(None) };
        journal::info(&format!(
            "fenetre affichee en {} ms (vignettes en arriere-plan)",
            debut.elapsed().as_millis()
        ));
    }
    (fenetre, etat)
}

/// Point d'entree AppKit, sur le fil principal.
pub fn lancer(mtm: MainThreadMarker, dossier: PathBuf) {
    let application = NSApplication::sharedApplication(mtm);
    unsafe {
        application.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    }
    let (_fenetre, _etat) = objc2::rc::autoreleasepool(|_| construire(mtm, dossier, true));
    // Purge du cache de vignettes hors fil principal ; les recettes n'appellent pas `lancer`.
    std::thread::spawn(|| {
        if let Some(cache) = crate::cache::dossier_defaut() {
            let retires = crate::cache::purger_anciens(&cache, crate::cache::AGE_MAX_PURGE);
            if retires > 0 {
                journal::info(&format!("cache de vignettes : {retires} fichier(s) ancien(s) purge(s)"));
            }
        }
    });
    crate::mises_a_jour::demarrer(mtm);
    unsafe {
        // `activate` n'est disponible qu'a partir de macOS 14.
        if application.respondsToSelector(sel!(activate)) {
            application.activate();
        } else {
            #[allow(deprecated)]
            application.activateIgnoringOtherApps(false);
        }
        application.run();
    }
}
