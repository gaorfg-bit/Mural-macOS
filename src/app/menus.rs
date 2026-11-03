//! Menus de la barre, du panneau et des vignettes.
use super::*;

impl Cible {
    pub(super) fn menu_diaporama(&self) -> Retained<NSMenu> {
        let mtm = MainThreadMarker::new().expect("menu sur le fil principal");
        let etat = self.etat().instantane_reglages();
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        let demarrer = self.element_menu(
            if etat.diaporama_actif {
                "Pause"
            } else {
                "Start slideshow"
            },
            Some(sel!(basculerDiaporama:)),
            "",
            None,
        );
        demarrer.setEnabled(etat.diaporama_actif || !etat.plan_vide);
        menu.addItem(&demarrer);
        let suivante =
            self.element_menu("Next image", Some(sel!(suivanteDiaporama:)), "", None);
        suivante.setEnabled(!etat.plan_vide);
        menu.addItem(&suivante);
        let aleatoire =
            self.element_menu("Random order", Some(sel!(aleatoireDiaporama:)), "", None);
        aleatoire.setState(if etat.reglages.diaporama_aleatoire {
            1
        } else {
            0
        });
        menu.addItem(&aleatoire);
        let intervalle = self.element_menu("Interval", None, "", None);
        let choix = NSMenu::new(mtm);
        for secondes in [60, 300, 900, 1800, 3600] {
            let item = self.element_menu(
                &format!("{} min", secondes / 60),
                Some(sel!(intervalleDiaporama:)),
                "",
                None,
            );
            item.setTag(secondes);
            item.setState(if etat.reglages.diaporama_intervalle == secondes as u64 {
                1
            } else {
                0
            });
            choix.addItem(&item);
        }
        intervalle.setSubmenu(Some(&choix));
        menu.addItem(&intervalle);
        let ecrans = self.element_menu("Participating displays", None, "", None);
        let choix = NSMenu::new(mtm);
        for ecran in &etat.ecrans {
            let item = self.element_menu(
                &format!("Display {} — {}", ecran.index + 1, ecran.nom),
                Some(sel!(ecranDiaporama:)),
                "",
                None,
            );
            unsafe {
                item.setRepresentedObject(Some(&NSString::from_str(&ecran.cle)));
            }
            item.setState(
                if etat.reglages.diaporama_ecrans.as_ref().map(|cles| cles.contains(&ecran.cle)).unwrap_or(true) {
                    1
                } else {
                    0
                },
            );
            choix.addItem(&item);
        }
        ecrans.setSubmenu(Some(&choix));
        menu.addItem(&ecrans);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        for (titre, action) in [
            (
                "Add folder to selection",
                sel!(ajouterDossierDiaporama:),
            ),
            (
                "Remove folder from selection",
                sel!(retirerDossierDiaporama:),
            ),
            (
                "Show slideshow images only",
                sel!(filtrerDiaporama:),
            ),
        ] {
            let item = self.element_menu(titre, Some(action), "", None);
            if action == sel!(filtrerDiaporama:) {
                item.setState(if etat.filtre_diaporama { 1 } else { 0 });
            }
            menu.addItem(&item);
        }
        let information =
            self.element_menu("Continues with window closed · ⌘Q to quit", None, "", None);
        information.setEnabled(false);
        menu.addItem(&information);
        menu
    }

    pub(super) fn element_menu(
        &self,
        titre: &str,
        action: Option<Sel>,
        raccourci: &str,
        chemin: Option<&std::path::Path>,
    ) -> Retained<NSMenuItem> {
        let mtm = MainThreadMarker::new().expect("menu sur le fil principal");
        let element = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &NSString::from_str(titre),
                action,
                &NSString::from_str(raccourci),
            )
        };
        unsafe {
            element.setTarget(Some(self));
            if let Some(chemin) = chemin {
                element.setRepresentedObject(Some(&NSString::from_str(&chemin.to_string_lossy())));
            }
        }
        element
    }

    pub(super) fn menu_image(&self, chemin: &std::path::Path) -> Retained<NSMenu> {
        let inclus = {
            let etat = self.etat();
            etat.reglages.favori_inclus(&chemin.to_string_lossy(), etat.cle_ecran_courant())
        };
        let mtm = MainThreadMarker::new().expect("menu image sur le fil principal");
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        for (titre, action) in [
            ("Apply this image", sel!(appliquer:)),
            ("Full-screen preview", sel!(apercuPleinEcran:)),
            ("Show in Finder", sel!(revelerImage:)),
            ("Copy path", sel!(copierChemin:)),
        ] {
            menu.addItem(&self.element_menu(titre, Some(action), "", Some(chemin)));
        }
        let selection = self.element_menu(
            "Include in slideshow",
            Some(sel!(imageDiaporama:)),
            "",
            Some(chemin),
        );
        selection.setState(if inclus { 1 } else { 0 });
        menu.addItem(&selection);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&self.element_menu(
            "Remove from gallery",
            Some(sel!(masquerImage:)),
            "",
            Some(chemin),
        ));
        menu.addItem(&self.element_menu(
            "Delete from disk…",
            Some(sel!(supprimerImage:)),
            "",
            Some(chemin),
        ));
        menu
    }

    pub(super) fn menu_dossiers(&self) -> Retained<NSMenu> {
        let (favoris, dossier) = {
            let etat = self.etat();
            (etat.reglages.dossiers_favoris.clone(), etat.dossier.clone())
        };
        let mtm = MainThreadMarker::new().expect("menu dossiers sur le fil principal");
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        if favoris.is_empty() {
            let vide =
                self.element_menu("No favorite folders — use the star", None, "", None);
            vide.setEnabled(false);
            menu.addItem(&vide);
        }
        for texte in &favoris {
            let chemin = std::path::Path::new(texte);
            let element = self.element_menu(texte, Some(sel!(ouvrirFavori:)), "", Some(chemin));
            element.setState(if chemin == dossier { 1 } else { 0 });
            menu.addItem(&element);
        }
        if !favoris.is_empty() {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            let retirer = self.element_menu("Remove favorite folder", None, "", None);
            let sous_menu = NSMenu::new(mtm);
            for texte in &favoris {
                sous_menu.addItem(&self.element_menu(
                    texte,
                    Some(sel!(retirerFavori:)),
                    "",
                    Some(std::path::Path::new(texte)),
                ));
            }
            retirer.setSubmenu(Some(&sous_menu));
            menu.addItem(&retirer);
        }
        menu
    }

    pub(super) fn actualiser_menu(&self) {
        let mtm = MainThreadMarker::new().expect("menu sur le fil principal");
        let (etat, selection) = {
            let etat = self.etat();
            (etat.instantane_reglages(), etat.selection.is_some())
        };
        let Some(menu) = self.ivars().menu.borrow().clone() else {
            return;
        };
        menu.removeAllItems();
        for (titre, action, actif) in [
            (
                "Same image on all displays",
                sel!(memeImagePartout:),
                etat.reglages.meme_image_partout,
            ),
            (
                "Also apply to all desktops (experimental)",
                sel!(tousLesBureaux:),
                etat.reglages.tous_les_bureaux,
            ),
            (
                "Independent selection per display",
                sel!(selectionIndependante:),
                etat.reglages.selection_independante,
            ),
            ("Sort by name", sel!(trierNom:), etat.tri == Tri::Nom),
            ("Sort by date", sel!(trierDate:), etat.tri == Tri::Date),
        ] {
            let element = self.element_menu(titre, Some(action), "", None);
            element.setState(if actif { 1 } else { 0 });
            menu.addItem(&element);
        }
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let compteur = etat
            .reglages
            .favoris_selection(etat.cle_ecran_courant())
            .len();
        let diaporama = self.element_menu(
            &format!("Slideshow — {compteur} image(s)"),
            None,
            "",
            None,
        );
        diaporama.setSubmenu(Some(&self.menu_diaporama()));
        menu.addItem(&diaporama);
        let dossiers = self.element_menu("Favorite folders", None, "", None);
        dossiers.setSubmenu(Some(&self.menu_dossiers()));
        menu.addItem(&dossiers);
        for (titre, action) in [
            ("Full-screen preview", sel!(apercuPleinEcran:)),
            ("Show in Finder", sel!(revelerImage:)),
            ("Copy path", sel!(copierChemin:)),
            ("Refresh gallery", sel!(actualiser:)),
            ("AVIF cache…", sel!(cacheAvif:)),
        ] {
            let element = self.element_menu(titre, Some(action), "", None);
            if action != sel!(actualiser:) && action != sel!(cacheAvif:) {
                element.setEnabled(selection);
            }
            menu.addItem(&element);
        }
        // Conditionnelle : sans image masquee, les positions existantes ne bougent pas.
        if !etat.reglages.images_masquees.is_empty() {
            menu.addItem(&self.element_menu("Show hidden images", Some(sel!(restaurerImages:)), "", None));
        }
        // Derniere entree : les indices existants (AVIF = 11) ne bougent pas.
        menu.addItem(&self.element_menu(
            "Settings…",
            Some(sel!(ouvrirReglages:)),
            "",
            None,
        ));

    }

    pub(super) fn installer_menu_principal(&self) {
        let mtm = MainThreadMarker::new().expect("menu principal sur le fil principal");
        let principal = NSMenu::new(mtm);
        let app = self.element_menu("Mural", None, "", None);
        let sous_menu = NSMenu::new(mtm);
        let version = NSBundle::mainBundle()
            .objectForInfoDictionaryKey(&NSString::from_str("CFBundleShortVersionString"))
            .and_then(|objet| objet.downcast_ref::<NSString>().map(|texte| texte.to_string()))
            .filter(|texte| !texte.is_empty())
            .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());
        let information = self.element_menu(&format!("Version {version}"), None, "", None);
        information.setEnabled(false);
        sous_menu.addItem(&information);
        sous_menu.addItem(&NSMenuItem::separatorItem(mtm));
        // Reglages natifs : ⌘, comme toute application macOS.
        sous_menu.addItem(&self.element_menu(
            "Settings…",
            Some(sel!(ouvrirReglages:)),
            ",",
            None,
        ));
        crate::mises_a_jour::installer(&sous_menu, mtm);
        sous_menu.addItem(&NSMenuItem::separatorItem(mtm));
        let quitter = self.element_menu("Quit Mural", Some(sel!(terminate:)), "q", None);
        unsafe {
            quitter.setTarget(None);
        }
        sous_menu.addItem(&quitter);
        app.setSubmenu(Some(&sous_menu));
        principal.addItem(&app);
        let fichier = self.element_menu("File", None, "", None);
        let commandes = NSMenu::new(mtm);
        for (titre, action, cle) in [
            ("Open folder…", sel!(ouvrirDossier:), "o"),
            (
                "Toggle favorite folder",
                sel!(dossierFavori:),
                "d",
            ),
            ("Full-screen preview", sel!(apercuPleinEcran:), "\r"),
            ("Refresh", sel!(actualiser:), "r"),
            ("Find…", sel!(ciblerRecherche:), "f"),
            ("Show controls", sel!(revelerCommandes:), "t"),
            ("Image details", sel!(afficherDetails:), "i"),
            ("Show window", sel!(afficherFenetre:), "1"),
            ("AVIF cache…", sel!(cacheAvif:), ""),
            ("Show log in Finder", sel!(revelerJournal:), ""),
        ] {
            commandes.addItem(&self.element_menu(titre, Some(action), cle, None));
        }
        fichier.setSubmenu(Some(&commandes));
        principal.addItem(&fichier);

        // Les commandes texte visent le premier repondeur (champ actif),
        // contrairement aux actions propres a Mural qui ciblent self.
        let edition = self.element_menu("Edit", None, "", None);
        let commandes_texte = NSMenu::new(mtm);
        for (titre, action, cle) in [
            ("Cut", sel!(cut:), "x"),
            ("Copy", sel!(copy:), "c"),
            ("Paste", sel!(paste:), "v"),
            ("Select All", sel!(selectAll:), "a"),
        ] {
            let element = self.element_menu(titre, Some(action), cle, None);
            unsafe { element.setTarget(None); }
            commandes_texte.addItem(&element);
        }
        edition.setSubmenu(Some(&commandes_texte));
        principal.addItem(&edition);
        NSApplication::sharedApplication(mtm).setMainMenu(Some(&principal));
    }
}
