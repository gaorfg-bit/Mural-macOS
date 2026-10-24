//! Panneau AVIF et conversion sequentielle hors du fil AppKit.
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, msg_send, sel};
use objc2_app_kit::{
    NSBackingStoreType, NSBezelStyle, NSButton, NSFont, NSLineBreakMode, NSProgressIndicator,
    NSTextField, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize, NSString};

#[derive(Clone, Default)]
pub(super) struct Evenement {
    pub total: usize,
    pub traitees: usize,
    pub converties: usize,
    pub reutilisees: usize,
    pub echecs: usize,
    pub octets_source: u64,
    pub octets_avif: u64,
    pub erreur: Option<String>,
    pub fin: bool,
    pub annulee: bool,
}

pub(super) struct Tache {
    pub annulation: Arc<AtomicBool>,
    pub reception: mpsc::Receiver<Evenement>,
}

pub(super) fn lancer(chemins: Vec<PathBuf>, cache: PathBuf, cible: usize) -> Tache {
    let (envoi, reception) = mpsc::channel();
    let annulation = Arc::new(AtomicBool::new(false));
    let arret = Arc::clone(&annulation);
    std::thread::spawn(move || {
        let mut bilan = Evenement {
            total: chemins.len(),
            ..Default::default()
        };
        let publier = |bilan: &Evenement| {
            if envoi.send(bilan.clone()).is_ok() {
                // Comme pour les vignettes : la cible vit jusqu'a la sortie.
                // Seul le selecteur programme est execute sur le fil principal.
                unsafe {
                    let objet = &*(cible as *const NSObject);
                    let _: () = msg_send![objet, performSelectorOnMainThread: sel!(conversionAvifPrete:), withObject: std::ptr::null::<NSObject>(), waitUntilDone: false];
                }
            }
        };
        for chemin in chemins {
            if arret.load(Ordering::Acquire) {
                break;
            }
            let resultat = objc2::rc::autoreleasepool(|_| crate::avif::convertir(&chemin, &cache));
            match resultat {
                Ok(copie) => {
                    if copie.reutilisee {
                        bilan.reutilisees += 1;
                    } else {
                        bilan.converties += 1;
                    }
                    bilan.octets_source += copie.octets_source;
                    bilan.octets_avif += copie.octets_avif;
                }
                Err(erreur) => {
                    bilan.echecs += 1;
                    bilan.erreur = Some(format!(
                        "{}: {erreur}",
                        chemin.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
            }
            bilan.traitees += 1;
            publier(&bilan);
        }
        bilan.fin = true;
        bilan.annulee = arret.load(Ordering::Acquire) && bilan.traitees < bilan.total;
        publier(&bilan);
    });
    Tache {
        annulation,
        reception,
    }
}

pub(super) struct Panneau {
    pub fenetre: Retained<NSWindow>,
    dossier: Retained<NSTextField>,
    statut: Retained<NSTextField>,
    detail: Retained<NSTextField>,
    progression: Retained<NSProgressIndicator>,
    pub convertir: Retained<NSButton>,
    pub annuler: Retained<NSButton>,
    ouvrir: Retained<NSButton>,
}

fn cadre(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect {
        origin: NSPoint { x, y },
        size: NSSize { width, height },
    }
}

fn texte(
    parent: &NSView,
    mtm: MainThreadMarker,
    valeur: &str,
    y: f64,
    hauteur: f64,
) -> Retained<NSTextField> {
    let champ = NSTextField::wrappingLabelWithString(&NSString::from_str(valeur), mtm);
    champ.setFrame(cadre(20.0, y, 520.0, hauteur));
    parent.addSubview(&champ);
    champ
}

impl Panneau {
    pub fn nouveau(mtm: MainThreadMarker, cible: &NSObject) -> Self {
        let fenetre = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                cadre(0.0, 0.0, 560.0, 300.0),
                NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe {
            fenetre.setReleasedWhenClosed(false);
        }
        fenetre.setTitle(&NSString::from_str("AVIF Cache"));
        fenetre.center();
        let contenu = fenetre.contentView().expect("AVIF content view");
        let titre = texte(&contenu, mtm, "AVIF copies", 252.0, 26.0);
        titre.setFont(Some(&NSFont::boldSystemFontOfSize(17.0)));
        let dossier = texte(&contenu, mtm, "", 220.0, 24.0);
        dossier.setMaximumNumberOfLines(1);
        dossier.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
        texte(
            &contenu,
            mtm,
            "Creates compressed copies in Mural’s cache. Originals and wallpapers stay unchanged.",
            174.0,
            40.0,
        );
        let statut = texte(&contenu, mtm, "", 140.0, 26.0);
        let progression = NSProgressIndicator::new(mtm);
        progression.setFrame(cadre(20.0, 116.0, 520.0, 16.0));
        progression.setIndeterminate(false);
        progression.setMinValue(0.0);
        progression.setMaxValue(1.0);
        progression.setDoubleValue(0.0);
        contenu.addSubview(&progression);
        let detail = texte(&contenu, mtm, "", 65.0, 42.0);
        let convertir = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str("Convert this folder"),
                Some(cible),
                Some(sel!(convertirAvif:)),
                mtm,
            )
        };
        convertir.setBezelStyle(NSBezelStyle::Automatic);
        convertir.setFrame(cadre(360.0, 18.0, 180.0, 32.0));
        contenu.addSubview(&convertir);
        let annuler = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str("Cancel"),
                Some(cible),
                Some(sel!(annulerAvif:)),
                mtm,
            )
        };
        annuler.setFrame(cadre(244.0, 18.0, 106.0, 32.0));
        annuler.setEnabled(false);
        contenu.addSubview(&annuler);
        let ouvrir = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str("Show cache"),
                Some(cible),
                Some(sel!(ouvrirCacheAvif:)),
                mtm,
            )
        };
        ouvrir.setFrame(cadre(20.0, 18.0, 122.0, 32.0));
        contenu.addSubview(&ouvrir);
        Self {
            fenetre,
            dossier,
            statut,
            detail,
            progression,
            convertir,
            annuler,
            ouvrir,
        }
    }

    pub fn preparer(&self, dossier: &Path, total: usize) {
        self.dossier
            .setStringValue(&NSString::from_str(&dossier.to_string_lossy()));
        self.dossier
            .setToolTip(Some(&NSString::from_str(&dossier.to_string_lossy())));
        let capable = crate::formats::avif_encodable();
        let cache = crate::avif::dossier_cache().is_some();
        let message = if !capable {
            "AVIF encoding is not available on this Mac.".to_owned()
        } else if !cache {
            "AVIF cache is unavailable.".to_owned()
        } else {
            format!(
                "{total} eligible image(s) · Library limit {}",
                crate::bibliotheque::LIMITE_DOSSIER
            )
        };
        self.statut.setStringValue(&NSString::from_str(&message));
        self.detail.setStringValue(&NSString::from_str(&format!(
            "PNG, JPEG, BMP and WebP • Native encoder • Quality {:.0}%",
            crate::avif::QUALITE * 100.0
        )));
        self.progression.setDoubleValue(0.0);
        self.convertir.setEnabled(capable && cache && total > 0);
        self.annuler.setEnabled(false);
        self.ouvrir
            .setEnabled(crate::avif::dossier_cache().is_some_and(|p| p.is_dir()));
    }

    pub fn demarrer(&self, total: usize) {
        self.progression.setMaxValue(total.max(1) as f64);
        self.progression.setDoubleValue(0.0);
        self.convertir.setEnabled(false);
        self.annuler.setEnabled(true);
        self.statut
            .setStringValue(&NSString::from_str(&format!("Converting 0/{total}…")));
        self.detail.setStringValue(&NSString::from_str(
            "You can keep using Mural. Cancel stops after the current image.",
        ));
    }

    pub fn annulation(&self) {
        self.annuler.setEnabled(false);
        self.statut
            .setStringValue(&NSString::from_str("Stopping after the current image…"));
    }

    pub fn actualiser(&self, e: &Evenement, annulation: bool) {
        self.progression.setDoubleValue(e.traitees as f64);
        let prefixe = if e.fin {
            if e.annulee { "Cancelled" } else { "Finished" }
        } else {
            "Converting"
        };
        self.statut.setStringValue(&NSString::from_str(&format!(
            "{prefixe} {}/{} · New {} · Reused {} · Failed {}",
            e.traitees, e.total, e.converties, e.reutilisees, e.echecs
        )));
        let detail = if let Some(erreur) = &e.erreur {
            erreur.clone()
        } else {
            format!(
                "Source {:.1} MB · AVIF copies {:.1} MB · Originals kept",
                e.octets_source as f64 / 1_000_000.0,
                e.octets_avif as f64 / 1_000_000.0
            )
        };
        self.detail.setStringValue(&NSString::from_str(&detail));
        self.convertir.setEnabled(e.fin);
        self.ouvrir
            .setEnabled(crate::avif::dossier_cache().is_some_and(|p| p.is_dir()));
        self.annuler.setEnabled(!e.fin && !annulation);
        if !e.fin && annulation {
            self.annulation();
        }
    }
}
