//! Planification pure : aucune ecriture sur le bureau depuis ce module.
use std::collections::{BTreeMap, BTreeSet};

pub fn planifier(
    images: &[String],
    ecrans: &[String],
    courantes: &BTreeMap<String, String>,
    meme_image: bool,
    aleatoire: bool,
    graine: u64,
) -> Vec<(String, String)> {
    if images.is_empty() || ecrans.is_empty() {
        return Vec::new();
    }
    let indice_suivant = |ecran: &str, position: usize| {
        let courant = courantes
            .get(ecran)
            .and_then(|p| images.iter().position(|i| i == p));
        if aleatoire && images.len() > 1 {
            let melange = graine
                .wrapping_add(position as u64)
                .wrapping_mul(0x9e3779b97f4a7c15)
                .rotate_left(17);
            if let Some(courant) = courant {
                let choix = melange as usize % (images.len() - 1);
                if choix >= courant { choix + 1 } else { choix }
            } else {
                melange as usize % images.len()
            }
        } else {
            courant
                .map(|i| (i + 1) % images.len())
                .unwrap_or(position % images.len())
        }
    };
    // Option commune : les écrans peuvent encore afficher des images différentes
    // (option activée en cours de route). On écarte les images de la sélection
    // qu'ils montrent déjà ; une courante hors sélection n'occupe aucune place.
    let affichees: BTreeSet<usize> = ecrans
        .iter()
        .filter_map(|e| courantes.get(e))
        .filter_map(|courante| images.iter().position(|i| i == courante))
        .collect();
    let commune = meme_image.then(|| {
        let preferee = indice_suivant(&ecrans[0], 0);
        // En séquentiel, on garde exactement l'image suivante de l'écran de
        // référence, même si un autre participant l'affiche déjà.
        if !aleatoire {
            return preferee;
        }
        // En aléatoire seulement : parcours déterministe de la sélection à partir
        // de la candidate préférée, première image libre s'il en existe, sinon
        // repli sur la candidate (toutes les images sont alors déjà affichées).
        (0..images.len())
            .map(|decalage| (preferee + decalage) % images.len())
            .find(|indice| !affichees.contains(indice))
            .unwrap_or(preferee)
    });
    ecrans
        .iter()
        .enumerate()
        .map(|(i, e)| {
            (
                e.clone(),
                images[commune.unwrap_or_else(|| indice_suivant(e, i))].clone(),
            )
        })
        .collect()
}

/// Planifie une rotation **independante par ecran** : chaque ecran ne pioche
/// que dans sa propre liste. Un ecran absent de `listes` ou dont la liste est
/// vide ne recoit aucun plan (pas de repli global implicite).
pub fn planifier_par_ecran(
    listes: &BTreeMap<String, Vec<String>>,
    ecrans: &[String],
    courantes: &BTreeMap<String, String>,
    aleatoire: bool,
    graine: u64,
) -> Vec<(String, String)> {
    ecrans
        .iter()
        .enumerate()
        .filter_map(|(position, ecran)| {
            let images = listes.get(ecran)?;
            if images.is_empty() {
                return None;
            }
            let choix = planifier(
                images,
                std::slice::from_ref(ecran),
                courantes,
                false,
                aleatoire,
                graine.wrapping_add(position as u64),
            );
            choix.into_iter().next()
        })
        .collect()
}

/// Liste de reference du mode « image commune ».
///
/// Choix simple et explicite : l'ecran selectionne s'il participe **et** a une
/// liste non vide ; sinon le premier ecran participant dont la liste n'est pas
/// vide. Aucune liste eligible renvoie `None` (aucun plan, aucun repli global).
pub fn liste_reference<'a>(
    listes: &'a BTreeMap<String, Vec<String>>,
    ecrans: &[String],
    ecran_selectionne: Option<&str>,
) -> Option<(&'a str, &'a Vec<String>)> {
    if let Some(cle) = ecran_selectionne
        && ecrans.iter().any(|e| e == cle)
        && let Some((cle, liste)) = listes.get_key_value(cle)
        && !liste.is_empty()
    {
        return Some((cle.as_str(), liste));
    }
    ecrans
        .iter()
        .find_map(|cle| {
            listes.get_key_value(cle)
                .filter(|(_, liste)| !liste.is_empty())
                .map(|(cle, liste)| (cle.as_str(), liste))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn liste(elements: &[&str]) -> Vec<String> {
        elements.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn sequence_independante_et_bouclage() {
        let courantes = BTreeMap::from([("a".into(), "3".into()), ("b".into(), "1".into())]);
        assert_eq!(
            planifier(
                &liste(&["1", "2", "3"]),
                &liste(&["a", "b"]),
                &courantes,
                false,
                false,
                0
            ),
            vec![("a".into(), "1".into()), ("b".into(), "2".into())]
        );
    }
    #[test]
    fn meme_image_sur_les_ecrans_participants() {
        let plan = planifier(
            &liste(&["1", "2"]),
            &liste(&["a", "c"]),
            &BTreeMap::new(),
            true,
            false,
            0,
        );
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].1, plan[1].1);
        assert_eq!(plan[1].0, "c");
    }
    #[test]
    fn aleatoire_ne_repete_pas_immediatement() {
        let courantes = BTreeMap::from([("a".into(), "2".into())]);
        for graine in 0..50 {
            assert_ne!(
                planifier(
                    &liste(&["1", "2", "3"]),
                    &liste(&["a"]),
                    &courantes,
                    false,
                    true,
                    graine
                )[0]
                .1,
                "2"
            );
        }
    }
    #[test]
    fn selection_ou_ecrans_vides_aucune_action() {
        assert!(planifier(&[], &liste(&["a"]), &BTreeMap::new(), false, false, 0).is_empty());
        assert!(planifier(&liste(&["1"]), &[], &BTreeMap::new(), false, false, 0).is_empty());
    }

    #[test]
    fn une_seule_image_disponible_est_toujours_choisie() {
        for meme_image in [false, true] {
            for aleatoire in [false, true] {
                for courantes in [
                    BTreeMap::new(),
                    BTreeMap::from([("a".to_string(), "1".to_string())]),
                ] {
                    assert_eq!(
                        planifier(
                            &liste(&["1"]),
                            &liste(&["a", "b"]),
                            &courantes,
                            meme_image,
                            aleatoire,
                            7
                        ),
                        vec![("a".into(), "1".into()), ("b".into(), "1".into())],
                        "meme_image={meme_image} aleatoire={aleatoire}"
                    );
                }
            }
        }
    }

    #[test]
    fn derniere_image_revient_a_la_premiere() {
        let plan = |images: &[&str], courante: &str| {
            planifier(
                &liste(images),
                &liste(&["a"]),
                &BTreeMap::from([("a".to_string(), courante.to_string())]),
                false,
                false,
                0,
            )
        };
        assert_eq!(plan(&["1", "2", "3"], "3")[0].1, "1");
        assert_eq!(plan(&["1", "2"], "2")[0].1, "1");
        // Une image unique boucle sur elle-même, sans sortir de la sélection.
        assert_eq!(plan(&["1"], "1")[0].1, "1");
    }

    #[test]
    fn image_courante_absente_de_la_selection() {
        let images = liste(&["1", "2", "3"]);
        let courantes = BTreeMap::from([("a".to_string(), "disparue".to_string())]);
        for graine in 0..50 {
            for aleatoire in [false, true] {
                let plan = planifier(
                    &images,
                    &liste(&["a"]),
                    &courantes,
                    false,
                    aleatoire,
                    graine,
                );
                assert_eq!(plan.len(), 1);
                assert_eq!(plan[0].0, "a");
                assert!(
                    images.contains(&plan[0].1),
                    "graine {graine}, aleatoire={aleatoire} : {} hors sélection",
                    plan[0].1
                );
                assert_eq!(
                    plan,
                    planifier(
                        &images,
                        &liste(&["a"]),
                        &courantes,
                        false,
                        aleatoire,
                        graine
                    )
                );
            }
        }
    }

    #[test]
    fn aucun_resultat_pour_un_ecran_non_participant() {
        let plan = planifier(
            &liste(&["1", "2"]),
            &liste(&["a"]),
            &BTreeMap::new(),
            false,
            false,
            0,
        );
        assert_eq!(plan.len(), 1);
        assert!(plan.iter().all(|(ecran, _)| ecran != "b"));
        assert_eq!(plan[0].0, "a");
        // Aucun écran choisi : aucun plan, même avec des images et des courantes.
        assert!(
            planifier(
                &liste(&["1"]),
                &[],
                &BTreeMap::from([("a".to_string(), "1".to_string())]),
                true,
                false,
                0
            )
            .is_empty()
        );
    }

    #[test]
    fn aleatoire_ne_repete_pas_immediatement_sur_chaque_ecran() {
        let images = liste(&["1", "2", "3"]);
        let ecrans = liste(&["a", "b", "c"]);
        let courantes = BTreeMap::from([
            ("a".to_string(), "2".to_string()),
            ("b".to_string(), "2".to_string()),
            ("c".to_string(), "2".to_string()),
        ]);
        for graine in 0..200 {
            let plan = planifier(&images, &ecrans, &courantes, false, true, graine);
            assert_eq!(plan.len(), 3);
            for (ecran, image) in &plan {
                assert_ne!(image, "2", "graine {graine} : répétition sur {ecran}");
            }
        }
    }

    #[test]
    fn option_commune_ne_repete_sur_aucun_ecran() {
        // Écrans encore désaccordés : l'option commune vient d'être activée.
        let images = liste(&["1", "2", "3"]);
        let ecrans = liste(&["a", "b"]);
        let courantes = BTreeMap::from([
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "2".to_string()),
        ]);
        for graine in 0..500 {
            let plan = planifier(&images, &ecrans, &courantes, true, true, graine);
            assert_eq!(plan[0].1, plan[1].1, "graine {graine} : images différentes");
            assert_ne!(
                plan[0].1, "1",
                "graine {graine} : l'écran a réaffiche son image"
            );
            assert_ne!(
                plan[1].1, "2",
                "graine {graine} : l'écran b réaffiche son image"
            );
        }
    }

    #[test]
    fn option_commune_ignore_les_courantes_hors_selection() {
        // « c » affiche une image qui n'est plus dans la sélection : elle ne doit
        // pas compter comme une image libre en moins.
        let plan = planifier(
            &liste(&["1", "2", "3"]),
            &liste(&["a", "b", "c"]),
            &BTreeMap::from([
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "2".to_string()),
                ("c".to_string(), "hors-selection".to_string()),
            ]),
            true,
            true,
            0,
        );
        assert_eq!(plan.len(), 3);
        assert!(
            plan.iter().all(|(_, image)| image == &plan[0].1),
            "l'option commune doit donner la même image partout"
        );
        assert_ne!(plan[0].1, "1", "l'écran a réaffiche son image");
        assert_ne!(plan[1].1, "2", "l'écran b réaffiche son image");
    }

    #[test]
    fn option_commune_sequentielle_suit_l_ordre_sans_sauter() {
        // Écrans accordés : simple pas en avant de la séquence.
        assert_eq!(
            planifier(
                &liste(&["1", "2", "3"]),
                &liste(&["a", "b"]),
                &BTreeMap::from([
                    ("a".to_string(), "1".to_string()),
                    ("b".to_string(), "1".to_string()),
                ]),
                true,
                false,
                0,
            ),
            vec![("a".into(), "2".into()), ("b".into(), "2".into())]
        );
        // Écrans désaccordés : l'image suivante de l'écran de référence est
        // conservée telle quelle, même si « b » l'affiche déjà (pas de saut).
        assert_eq!(
            planifier(
                &liste(&["1", "2", "3"]),
                &liste(&["a", "b"]),
                &BTreeMap::from([
                    ("a".to_string(), "1".to_string()),
                    ("b".to_string(), "2".to_string()),
                ]),
                true,
                false,
                0,
            ),
            vec![("a".into(), "2".into()), ("b".into(), "2".into())]
        );
    }

    #[test]
    fn repli_commun_quand_toutes_les_images_sont_affichees() {
        // Deux images, toutes deux déjà à l'écran : aucune libre. Le repli reste
        // la candidate de l'écran de référence, identique partout.
        let plan = planifier(
            &liste(&["1", "2"]),
            &liste(&["a", "b"]),
            &BTreeMap::from([
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "2".to_string()),
            ]),
            true,
            true,
            0,
        );
        assert_eq!(
            plan,
            vec![("a".into(), "2".into()), ("b".into(), "2".into())]
        );
    }

    #[test]
    fn graine_deterministe_et_utilisee() {
        let images = liste(&["1", "2", "3"]);
        let plan = |graine: u64| {
            planifier(
                &images,
                &liste(&["a"]),
                &BTreeMap::new(),
                false,
                true,
                graine,
            )
        };
        assert_eq!(plan(42), plan(42));
        let vues: BTreeSet<String> = (0..20).map(|graine| plan(graine)[0].1.clone()).collect();
        assert_eq!(
            vues.len(),
            images.len(),
            "la graine doit parcourir la sélection"
        );
    }

    fn map_listes(entrees: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        entrees
            .iter()
            .map(|(cle, images)| ((*cle).to_string(), liste(images)))
            .collect()
    }

    #[test]
    fn listes_par_ecran_independantes_et_liste_vide_sans_repli() {
        let listes = map_listes(&[("a", &["a1", "a2"]), ("b", &["b1"]), ("c", &[] as &[&str])]);
        let plan = planifier_par_ecran(
            &listes,
            &liste(&["a", "b", "c", "d"]),
            &BTreeMap::new(),
            false,
            0,
        );
        // « c » a une liste vide et « d » n'en a pas : aucun plan pour eux,
        // et jamais d'emprunt à la liste d'un autre écran.
        assert_eq!(
            plan,
            vec![("a".into(), "a1".into()), ("b".into(), "b1".into())]
        );
    }

    #[test]
    fn listes_par_ecran_avancent_independamment() {
        let listes = map_listes(&[("a", &["a1", "a2"]), ("b", &["b1", "b2"])]);
        let courantes = BTreeMap::from([
            ("a".to_string(), "a1".to_string()),
            ("b".to_string(), "b1".to_string()),
        ]);
        let plan = planifier_par_ecran(
            &listes,
            &liste(&["a", "b"]),
            &courantes,
            false,
            0,
        );
        assert_eq!(
            plan,
            vec![("a".into(), "a2".into()), ("b".into(), "b2".into())]
        );
    }

    #[test]
    fn reference_du_mode_commun() {
        let listes = map_listes(&[("a", &["a1"]), ("b", &["b1", "b2"])]);
        let ecrans = liste(&["a", "b"]);
        // Ecran selectionne participant et non vide : sa liste gagne.
        assert_eq!(
            liste_reference(&listes, &ecrans, Some("b")).map(|(_, liste)| liste),
            Some(&listes["b"])
        );
        // Ecran selectionne non participant : premier participant eligible.
        assert_eq!(
            liste_reference(&listes, &ecrans, Some("z")).map(|(_, liste)| liste),
            Some(&listes["a"])
        );
        // Ecran selectionne participant mais vide : premier eligible.
        let listes = map_listes(&[("a", &["a1"]), ("b", &[] as &[&str])]);
        assert_eq!(
            liste_reference(&listes, &ecrans, Some("b")).map(|(_, liste)| liste),
            Some(&listes["a"])
        );
        // Aucune liste eligible : aucun plan, pas de repli global.
        let vides = map_listes(&[("a", &[] as &[&str]), ("b", &[] as &[&str])]);
        assert_eq!(liste_reference(&vides, &ecrans, Some("a")).map(|(_, liste)| liste), None);
        assert_eq!(liste_reference(&BTreeMap::new(), &ecrans, None).map(|(_, liste)| liste), None);
    }

    #[test]
    fn mode_commun_applique_la_meme_image_aux_participants() {
        let listes = map_listes(&[("a", &["a1", "a2"]), ("b", &["b1"])]);
        let ecrans = liste(&["a", "b"]);
        let (_, reference) = liste_reference(&listes, &ecrans, Some("a")).unwrap();
        let plan = planifier(
            reference,
            &ecrans,
            &BTreeMap::from([
                ("a".to_string(), "a1".to_string()),
                ("b".to_string(), "b1".to_string()),
            ]),
            true,
            false,
            0,
        );
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].1, plan[1].1);
        assert_eq!(plan[0].1, "a2", "suite de la liste de référence");
    }
}
