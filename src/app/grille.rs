//! Galerie : reconstruction de la grille, chargement des zones visibles, selection.
use super::*;

/// Mesures de la grille pour une largeur donnee : colonnes, largeur, hauteur
/// d'une cellule. Isole ici parce que la construction et la reconstruction
/// doivent donner exactement le meme resultat.
pub(super) fn mesures_grille(largeur: f64) -> (usize, f64, f64) {
    let marge = 2.0;
    let largeur_utile = largeur - 2.0 * marge;
    let colonnes =
        (((largeur_utile + ESPACEMENT) / (LARGEUR_VIGNETTE + ESPACEMENT)).floor() as usize).max(2);
    let largeur_cellule = (largeur_utile - ESPACEMENT * (colonnes as f64 - 1.0)) / colonnes as f64;
    let hauteur_cellule = (largeur_cellule * bibliotheque::ASPECT_VIGNETTE).round();
    (colonnes, largeur_cellule, hauteur_cellule)
}

impl Cible {
    /// Seuls les pixels visibles et une rangee de marge restent charges.
    pub(super) fn charger_zone_visible(&self) {
        let ivars = self.ivars();
        if ivars.etat.get().is_null() || ivars.document.get().is_null() { return; }
        let generation = ivars.generation_grille.get();
        let cellules = {
            let etat = self.etat();
            if etat.chargement_integral { return; }
            etat.cellules.iter().map(|(&adresse, &index)| (
                unsafe { &*(adresse as *const NSButton) }.retain(), index,
            )).collect::<Vec<_>>()
        };
        let visible = unsafe { &*ivars.document.get() }.visibleRect();
        let mut indices: Vec<_> = cellules.iter().filter_map(|(bouton, index)| {
            let cadre = bouton.frame();
            let marge = cadre.size.height + ESPACEMENT;
            (cadre.origin.y < visible.origin.y + visible.size.height + marge
                && cadre.origin.y + cadre.size.height > visible.origin.y - marge).then_some(*index)
        }).collect();
        indices.sort_unstable();
        if ivars.generation_grille.get() != generation { return; }
        let (a_vider, statut) = {
            let mut etat = self.etat_mut();
            etat.demander_vignettes(indices, Some(self as *const Self as usize));
            let a_vider = cellules.into_iter().filter_map(|(bouton, index)|
                etat.vignettes[index].is_none().then_some((bouton, etat.vignettes_en_attente[index]))
            ).collect::<Vec<_>>();
            (a_vider, etat.statut_bibliotheque())
        };
        for (bouton, en_attente) in a_vider {
            bouton.setImage(None);
            bouton.setTitle(&NSString::from_str(if en_attente { "…" } else { "" }));
        }
        unsafe { &*ivars.etat_texte.get() }.setStringValue(&NSString::from_str(&statut));
    }

    /// (Re)construit les cellules de la grille pour l'etat courant.
    pub(super) fn rafraichir_grille(&self) {
        let ivars = self.ivars();
        if ivars.etat.get().is_null() || ivars.document.get().is_null() {
            return;
        }
        let debut = std::time::Instant::now();
        let generation = ivars.generation_grille.get().wrapping_add(1);
        ivars.generation_grille.set(generation);
        // Aucun emprunt de l'etat pendant les notifications synchrones AppKit.
        let (affiches, filtre, vide) = {
            let mut etat = self.etat_mut();
            etat.cellules.clear();
            etat.boutons.clear();
            let images = etat.indices_affiches().into_iter().map(|index| (
                index, etat.images[index].clone(), etat.vignettes[index].clone(),
                etat.vignettes_en_attente[index],
            )).collect::<Vec<_>>();
            let vide = images.is_empty().then(|| message_galerie_vide(
                &etat.dossier, etat.dossier.is_dir(), etat.images.is_empty(), &etat.filtre));
            (images, etat.filtre.clone(), vide)
        };
        // Les deux symboles des badges sont resolus une fois, pas pour chaque cellule.
        let symbole_actif = symbole("checkmark.circle.fill");
        let symbole_diaporama = symbole("star.fill");
        let mut cellules = HashMap::new();
        let mut boutons: Vec<Retained<NSButton>> = Vec::new();
        let document = unsafe { &*ivars.document.get() };
        let defilement = unsafe { &*ivars.defilement.get() };
        defilement.contentView().setPostsBoundsChangedNotifications(false);

        // On retire les anciennes cellules avant d'en recreer.
        let anciennes = unsafe { document.subviews() };
        for index in 0..anciennes.count() {
            let vue = unsafe { anciennes.objectAtIndex(index) };
            unsafe { vue.removeFromSuperview() };
        }
        if ivars.generation_grille.get() != generation { return; }

        let largeur_grille = defilement.contentSize().width;
        let (colonnes, largeur_cellule, mut hauteur_cellule) = mesures_grille(largeur_grille);
        let marge = 2.0;
        let lignes = affiches.len().div_ceil(colonnes.max(1));
        if lignes > 0 {
            hauteur_cellule = hauteur_cellule.max((defilement.contentSize().height - 8.0
                - ESPACEMENT * (lignes.saturating_sub(1)) as f64) / lignes as f64);
        }

        for (position, (index, image, vignette, en_attente)) in affiches.iter().enumerate() {
            let colonne = position % colonnes;
            let ligne = position / colonnes;
            let bouton = unsafe {
                {
                    let Some(mtm) = MainThreadMarker::new() else {
                        return;
                    };
                    let bouton: Retained<BoutonVignette> = msg_send![
                        super(mtm.alloc::<BoutonVignette>().set_ivars(image.chemin.clone())),
                        init
                    ];
                    bouton.setTitle(&NSString::from_str(""));
                    bouton.setTarget(Some(self));
                    bouton.setAction(Some(sel!(celluleCliquee:)));
                    bouton.setImagePosition(objc2_app_kit::NSCellImagePosition::ImageOnly);
                    Retained::into_super(bouton)
                }
            };
            if ivars.generation_grille.get() != generation { return; }
            unsafe {
                bouton.setBordered(false);
                // Le titre devient vide apres le chargement de l'image :
                // conserver un nom accessible independant de ce placeholder.
                let nom = NSString::from_str(&image.nom);
                let _: () = msg_send![&*bouton, setAccessibilityLabel: &*nom];
                bouton.setToolTip(Some(&nom));
                bouton.setFrame(NSRect {
                    origin: NSPoint {
                        x: marge + colonne as f64 * (largeur_cellule + ESPACEMENT),
                        y: 4.0 + ligne as f64 * (hauteur_cellule + ESPACEMENT),
                    },
                    size: NSSize {
                        width: largeur_cellule,
                        height: hauteur_cellule,
                    },
                });
                if let Some(vignette) = vignette.as_deref() {
                    bouton.setImage(Some(vignette));
                    bouton.setImageScaling(NSImageScaling::ScaleAxesIndependently);
                } else {
                    bouton.setTitle(&NSString::from_str(if *en_attente {
                        "…"
                    } else {
                        "(unreadable)"
                    }));
                }
                bouton.setWantsLayer(true);
                if let Some(couche) = bouton.layer() {
                    couche.setCornerRadius(6.0);
                    couche.setMasksToBounds(true);
                }
                for (tag, icone, legende, x, y) in [
                    (8101, &symbole_actif, "Wallpaper path active on a display", largeur_cellule - 24.0,
                        if bouton.isFlipped() { 6.0 } else { hauteur_cellule - 24.0 }),
                    (8102, &symbole_diaporama, "Included in this display’s slideshow", 6.0,
                        if bouton.isFlipped() { hauteur_cellule - 24.0 } else { 6.0 }),
                ] {
                    let mtm = MainThreadMarker::new().expect("badge sur le fil principal");
                    let badge: Retained<BadgeVignette> = msg_send![mtm.alloc::<BadgeVignette>(), init];
                    badge.setTag(tag);
                    badge.setFrame(NSRect { origin: NSPoint { x, y }, size: NSSize { width: 18.0, height: 18.0 } });
                    badge.setImage(Some(icone));
                    let teinte = if tag == 8101 { NSColor::controlAccentColor() } else { NSColor::systemYellowColor() };
                    badge.setContentTintColor(Some(&teinte));
                    let _: () = msg_send![&*badge, setAccessibilityLabel: &*NSString::from_str(legende)];
                    badge.setWantsLayer(true);
                    if let Some(couche) = badge.layer() {
                        couche.setBackgroundColor(Some(&NSColor::windowBackgroundColor().CGColor()));
                        couche.setCornerRadius(9.0);
                    }
                    badge.setHidden(true);
                    bouton.addSubview(&badge);
                }
                document.addSubview(&bouton);
            }
            if ivars.generation_grille.get() != generation { return; }
            cellules.insert(unsafe { &*bouton } as *const NSButton as usize, *index);
            boutons.push(bouton.clone());
        }

        if let Some((texte, proposer_dossier)) = &vide {
            self.poser_etat_vide(document, defilement.contentSize(), texte, *proposer_dossier);
        }
        // Galerie vide (hors recherche) : rien a survoler, les commandes restent visibles.
        if let Some(surface) = ivars.surface.borrow().as_ref() {
            surface.epingler(vide.as_ref().is_some_and(|(_, proposer)| *proposer));
        }

        let lignes = affiches.len().div_ceil(colonnes.max(1));
        let hauteur_document = 8.0 + lignes as f64 * hauteur_cellule + lignes.saturating_sub(1) as f64 * ESPACEMENT;
        unsafe {
            document.setFrame(NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: largeur_grille,
                    height: hauteur_document.max(unsafe { defilement.frame() }.size.height),
                },
            });
        }
        journal::info(&format!(
            "grille : {} vignette(s) affichee(s) sur {} colonnes en {} ms{}",
            affiches.len(),
            colonnes,
            debut.elapsed().as_millis(),
            if filtre.is_empty() {
                String::new()
            } else {
                format!(" (filtre « {} »)", filtre)
            }
        ));
        if ivars.generation_grille.get() != generation { return; }
        {
            let mut etat = self.etat_mut();
            etat.cellules = cellules;
            etat.boutons = boutons;
        }
        defilement.contentView().setPostsBoundsChangedNotifications(true);
        self.charger_zone_visible();
    }

    /// Message centre dans la grille quand rien n'est affiche. La vue est un
    /// enfant du document : la prochaine reconstruction la retire avec les cellules.
    fn poser_etat_vide(&self, document: &NSView, taille: NSSize, texte: &str, proposer_dossier: bool) {
        let Some(mtm) = MainThreadMarker::new() else { return; };
        let largeur = (taille.width - 48.0).clamp(120.0, 420.0);
        let message = NSTextField::wrappingLabelWithString(&NSString::from_str(texte), mtm);
        message.setAlignment(objc2_app_kit::NSTextAlignment::Center);
        message.setTextColor(Some(&NSColor::secondaryLabelColor()));
        message.setFont(Some(&NSFont::systemFontOfSize(13.0)));
        message.setPreferredMaxLayoutWidth(largeur);
        let hauteur_texte = message.fittingSize().height.max(18.0);
        let hauteur_bouton = if proposer_dossier { 36.0 } else { 0.0 };
        // Document retourne : y croit vers le bas.
        let haut = ((taille.height - hauteur_texte - hauteur_bouton) / 2.0).max(8.0);
        message.setFrame(NSRect::new(NSPoint::new((taille.width - largeur) / 2.0, haut),
            NSSize::new(largeur, hauteur_texte)));
        document.addSubview(&message);
        if proposer_dossier {
            let bouton = unsafe { NSButton::buttonWithTitle_target_action(
                &NSString::from_str("Open folder…"), Some(self), Some(sel!(ouvrirDossier:)), mtm) };
            bouton.setToolTip(Some(&NSString::from_str("Choose a folder of images (⌘O)")));
            let largeur_bouton = bouton.fittingSize().width.max(140.0);
            bouton.setFrame(NSRect::new(NSPoint::new((taille.width - largeur_bouton) / 2.0, haut + hauteur_texte + 10.0),
                NSSize::new(largeur_bouton, 28.0)));
            document.addSubview(&bouton);
        }
    }

    /// Reapplique l'etat : apercu, informations, libelle du bouton, selection.
    pub(super) fn rafraichir(&self) {
        let ivars = self.ivars();
        if ivars.etat.get().is_null() {
            return;
        }
        let (image, dossier, favori, actif, disponible, libelle, legende, selection, nb_ecrans, courant) = {
            let etat = self.etat();
            let libelle = if etat.reglages.meme_image_partout {
                "Apply to all".to_owned()
            } else if etat.ecrans.len() > 1 {
                format!("Apply (display {})", etat.ecran_courant + 1)
            } else { "Apply".to_owned() };
            (etat.selection.and_then(|i| etat.images.get(i)).cloned(), etat.dossier.clone(),
                etat.reglages.dossiers_favoris.contains(&etat.dossier.to_string_lossy().into_owned()),
                etat.diaporama_actif, !etat.plan_diaporama().is_empty(), libelle,
                etat.statut_bibliotheque(), etat.selection.is_some(), etat.ecrans.len(), etat.ecran_courant)
        };

        let (apercu, nom, dims) = match image.as_ref() {
            Some(image) => {
                let (apercu, dimensions) = self.charger_apercu(image, 1600, 780);
                (apercu, image.nom.clone(), dimensions)
            }
            None => {
                let ancien = ivars.apercu_memoire.borrow_mut().take();
                drop(ancien);
                (None, String::new(), String::new())
            }
        };

        if !ivars.favori.get().is_null() {
            unsafe { &*ivars.favori.get() }.setImage(Some(&symbole(if favori {
                "star.fill"
            } else {
                "star"
            })));
            unsafe { &*ivars.favori.get() }.setToolTip(Some(&NSString::from_str(if favori {
                "Remove folder from favorites"
            } else {
                "Add folder to favorites"
            })));
        }
        if !ivars.fenetre.get().is_null() {
            unsafe { &*ivars.fenetre.get() }.setTitle(&NSString::from_str(&format!(
                "Mural — {}",
                dossier
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            )));
            unsafe { &*ivars.fenetre.get() }
                .setRepresentedFilename(&NSString::from_str(&dossier.to_string_lossy()));
        }
        if !ivars.bouton_diaporama.get().is_null() {
            let bouton = unsafe { &*ivars.bouton_diaporama.get() };
            bouton.setImage(Some(&symbole(if actif {
                "pause.fill"
            } else {
                "play.fill"
            })));
            bouton.setToolTip(Some(&NSString::from_str(if actif {
                "Pause slideshow"
            } else {
                "Start slideshow"
            })));
            bouton.setEnabled(actif || disponible);
            unsafe { &*ivars.bouton_suivante.get() }.setEnabled(disponible);
        }
        unsafe {
            (*ivars.apercu.get()).setImage(apercu.as_deref());
            (*ivars.nom_fichier.get()).setStringValue(&NSString::from_str(&nom));
            (*ivars.dimensions.get()).setStringValue(&NSString::from_str(&dims));
            (*ivars.dimensions.get()).setToolTip(Some(&NSString::from_str(&dims)));
            (*ivars.etat_texte.get()).setStringValue(&NSString::from_str(
                &legende,
            ));
            (*ivars.bouton_appliquer.get()).setTitle(&NSString::from_str(&libelle));
            (*ivars.bouton_appliquer.get())
                .setEnabled(selection && nb_ecrans != 0);
            // Le panneau de reglages, la barre principale et les menus lisent
            // tous le meme etat : la selection d'ecran reste synchronisee.
            if !ivars.segments.get().is_null() {
                (*ivars.segments.get()).setSelectedSegment(if nb_ecrans == 0 {
                    -1
                } else {
                    courant as isize
                });
            }
        }

        let erreur = self.etat().erreur_sauvegarde.is_some();
        if erreur {
            let details = ivars.details.borrow().clone();
            if let Some(details) = details { details.setHidden(false); }
        }
        self.rafraichir_selection();
        self.rafraichir_panneau();
    }

    pub(super) fn rafraichir_selection(&self) {
        let cellules: Vec<_> = {
            let etat = self.etat();
            etat.cellules.iter().filter_map(|(pointeur, index)| {
                let image = etat.images.get(*index)?;
                let chemin = image.chemin.to_string_lossy();
                Some((unsafe { &*(*pointeur as *const NSButton) }.retain(),
                    etat.selection == Some(*index),
                    etat.fonds_actifs.iter().any(|p| p == chemin.as_ref()),
                    etat.reglages.favori_inclus(&chemin, etat.cle_ecran_courant())))
            }).collect()
        };
        let accent = NSColor::controlAccentColor();
        let couleur = accent.CGColor();
        for (bouton, selectionnee, actif, inclus) in cellules {
            for badge in bouton.subviews() {
                match badge.tag() {
                    8101 => badge.setHidden(!actif),
                    8102 => badge.setHidden(!inclus),
                    _ => {}
                }
            }
            unsafe {
                bouton.setWantsLayer(true);
                if let Some(couche) = bouton.layer() {
                    couche.setCornerRadius(6.0);
                    couche.setMasksToBounds(true);
                    couche.setBorderWidth(if selectionnee { 3.0 } else { 0.0 });
                    couche.setBorderColor(Some(&couleur));
                }
            }
        }
    }
}

/// Texte de la galerie vide et s'il faut proposer d'ouvrir un dossier.
/// `aucune_image` : le dossier ne contient aucune image affichable (avant filtre).
pub(super) fn message_galerie_vide(dossier: &std::path::Path, existe: bool, aucune_image: bool, filtre: &str) -> (String, bool) {
    if !existe {
        return (format!("This folder is unavailable:\n{}\nReconnect its disk or choose another folder.", dossier.display()), true);
    }
    if aucune_image || filtre.is_empty() {
        let nom = dossier.file_name().map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dossier.display().to_string());
        return (format!("No images in “{nom}”.\nChoose a folder of PNG, JPEG or WebP images."), true);
    }
    (format!("No images match “{filtre}”."), false)
}

#[cfg(test)]
mod tests_galerie_vide {
    use super::message_galerie_vide;
    use std::path::Path;

    #[test]
    fn dossier_absent_propose_un_autre_dossier() {
        let (texte, proposer) = message_galerie_vide(Path::new("/Volumes/Disque/Fonds"), false, true, "");
        assert!(texte.contains("unavailable") && texte.contains("/Volumes/Disque/Fonds"));
        assert!(proposer);
    }

    #[test]
    fn dossier_sans_image_nomme_le_dossier() {
        let (texte, proposer) = message_galerie_vide(Path::new("/Users/x/Pictures"), true, true, "");
        assert!(texte.contains("“Pictures”"));
        assert!(proposer);
    }

    #[test]
    fn recherche_sans_resultat_ne_bloque_pas_les_commandes() {
        let (texte, proposer) = message_galerie_vide(Path::new("/Users/x/Pictures"), true, false, "plage");
        assert_eq!(texte, "No images match “plage”.");
        assert!(!proposer);
    }
}
