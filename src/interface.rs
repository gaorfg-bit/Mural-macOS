//! Composition epuree : apercu bord a bord, separation native invisible,
//! commandes revelees aux bords. Aucune minuterie ni boucle de surveillance.
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSColor, NSCompositingOperation, NSEvent, NSImageView, NSSplitView,
    NSSplitViewDelegate, NSTrackingArea, NSTrackingAreaOptions, NSView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowButton,
};
use objc2_foundation::{NSObjectProtocol, NSPoint, NSRect, NSSize};
use std::cell::{Cell, RefCell};

/// Rectangle centré remplissant le cadre sans deformation.
pub(crate) fn cadrage(image: NSSize, cadre: NSRect, entier: bool) -> NSRect {
    if image.width <= 0.0 || image.height <= 0.0 {
        return cadre;
    }
    let x = cadre.size.width / image.width;
    let y = cadre.size.height / image.height;
    let facteur = if entier { x.min(y) } else { x.max(y) };
    let taille = NSSize {
        width: image.width * facteur,
        height: image.height * facteur,
    };
    NSRect {
        origin: NSPoint {
            x: cadre.origin.x + (cadre.size.width - taille.width) / 2.0,
            y: cadre.origin.y + (cadre.size.height - taille.height) / 2.0,
        },
        size: taille,
    }
}

define_class!(
    #[unsafe(super(NSImageView))]
    #[name = "MuralApercuBordABord"]
    #[ivars = Cell<bool>]
    pub struct Apercu;
    impl Apercu {
        #[unsafe(method(drawRect:))]
        fn dessiner(&self, _rect: NSRect) {
            objc2_app_kit::NSBezierPath::clipRect(self.bounds());
            NSColor::windowBackgroundColor().setFill();
            unsafe { objc2_app_kit::NSRectFill(self.bounds()); }
            if let Some(image) = self.image() {
                image.drawInRect_fromRect_operation_fraction(
                    cadrage(image.size(), self.bounds(), self.ivars().get()),
                    NSRect::ZERO, NSCompositingOperation::SourceOver, 1.0);
            }
        }
        #[unsafe(method(mouseDown:))]
        fn cliquer(&self, evenement: &NSEvent) {
            if evenement.clickCount() == 2 {
                self.ivars().set(!self.ivars().get());
                let vue: &NSView = self;
                vue.setNeedsDisplay(true);
            }
        }
    }
);
impl Apercu {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(Cell::new(false));
        unsafe { msg_send![super(this), init] }
    }
}

define_class!(
    #[unsafe(super(NSSplitView))]
    #[name = "MuralSeparationInvisible"]
    pub struct Separation;
    unsafe impl NSObjectProtocol for Separation {}
    unsafe impl NSSplitViewDelegate for Separation {
        #[unsafe(method(splitView:constrainMinCoordinate:ofSubviewAt:))]
        fn minimum(&self, _split: &NSSplitView, _position: f64, _index: isize) -> f64 { 110.0 }
        #[unsafe(method(splitView:constrainMaxCoordinate:ofSubviewAt:))]
        fn maximum(&self, split: &NSSplitView, _position: f64, _index: isize) -> f64 {
            (split.bounds().size.height - 140.0).max(110.0)
        }
        #[unsafe(method(splitView:effectiveRect:forDrawnRect:ofDividerAtIndex:))]
        fn zone(&self, _split: &NSSplitView, _propose: NSRect, dessine: NSRect, _index: isize) -> NSRect {
            NSRect::new(NSPoint::new(dessine.origin.x, dessine.origin.y-4.0),
                NSSize::new(dessine.size.width, 9.0))
        }
        #[unsafe(method(splitView:resizeSubviewsWithOldSize:))]
        fn ajuster(&self, split: &NSSplitView, ancien: NSSize) {
            let enfants = split.subviews();
            if enfants.count() != 2 { return; }
            let apercu = enfants.objectAtIndex(0);
            let grille = enfants.objectAtIndex(1);
            let taille = split.bounds().size;
            let ratio = if ancien.height > 0.0 { apercu.frame().size.height / ancien.height } else { 0.48 };
            let haut = (taille.height * ratio).clamp(110.0, (taille.height-140.0).max(110.0));
            apercu.setFrame(NSRect::new(NSPoint::ZERO, NSSize::new(taille.width, haut)));
            grille.setFrame(NSRect::new(NSPoint::new(0.0, haut+1.0), NSSize::new(taille.width, (taille.height-haut-1.0).max(0.0))));
        }
    }
    impl Separation {
        #[unsafe(method(isFlipped))]
        fn retourne(&self) -> bool { true }
        #[unsafe(method(dividerThickness))]
        fn epaisseur(&self) -> f64 { 1.0 }
        #[unsafe(method(drawDividerInRect:))]
        fn dessiner(&self, _rect: NSRect) {}
    }
);
impl Separation {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this: Retained<Self> = unsafe { msg_send![mtm.alloc::<Self>(), init] };
        this.setDelegate(Some(ProtocolObject::from_ref(&*this)));
        this
    }
}

pub struct RacineIvars {
    suivi: RefCell<Option<Retained<NSTrackingArea>>>,
    outils: RefCell<Option<Retained<NSView>>>,
    dossiers: RefCell<Option<Retained<NSView>>>,
}
define_class!(
    #[unsafe(super(NSView))]
    #[name = "MuralSurface"]
    #[ivars = RacineIvars]
    pub struct Racine;
    impl Racine {
        #[unsafe(method(updateTrackingAreas))]
        fn suivi(&self) {
            unsafe { let _: () = msg_send![super(self), updateTrackingAreas]; }
            if let Some(ancien) = self.ivars().suivi.borrow_mut().take() {
                self.removeTrackingArea(&ancien);
            }
            let zone = unsafe { NSTrackingArea::initWithRect_options_owner_userInfo(
                self.mtm().alloc(), NSRect::ZERO,
                NSTrackingAreaOptions::MouseMoved | NSTrackingAreaOptions::MouseEnteredAndExited
                    | NSTrackingAreaOptions::ActiveInActiveApp | NSTrackingAreaOptions::InVisibleRect,
                Some(self), None) };
            self.addTrackingArea(&zone);
            *self.ivars().suivi.borrow_mut() = Some(zone);
        }
        #[unsafe(method(mouseMoved:))]
        fn bouger(&self, evenement: &NSEvent) {
            let p = self.convertPoint_fromView(evenement.locationInWindow(), None);
            let h = self.bounds().size.height;
            if let Some(outils) = self.ivars().outils.borrow().as_ref() {
                if p.y >= h-14.0 { outils.setHidden(false); self.boutons_fenetre(false); }
                else if p.y < outils.frame().origin.y - 8.0 {
                    let edition = self.window().and_then(|w| w.firstResponder()).is_some_and(|r| {
                        r.downcast_ref::<objc2_app_kit::NSTextView>().is_some_and(|t| t.isFieldEditor())
                    });
                    if !edition { outils.setHidden(true); self.boutons_fenetre(true); }
                }
            }
            if let Some(dossiers) = self.ivars().dossiers.borrow().as_ref() {
                if p.x <= 8.0 { dossiers.setHidden(false); }
                else if p.x > dossiers.frame().size.width + 16.0 { dossiers.setHidden(true); }
            }
        }
        #[unsafe(method(mouseExited:))]
        fn sortir(&self, _evenement: &NSEvent) { self.masquer(); }
    }
);
impl Racine {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(RacineIvars {
            suivi: RefCell::new(None),
            outils: RefCell::new(None),
            dossiers: RefCell::new(None),
        });
        unsafe { msg_send![super(this), init] }
    }
    pub fn panneaux(&self, outils: &NSView, dossiers: &NSView) {
        *self.ivars().outils.borrow_mut() = Some(outils.retain());
        *self.ivars().dossiers.borrow_mut() = Some(dossiers.retain());
        self.masquer();
    }
    pub fn matiere(&self, verre: bool) {
        for cellule in [&self.ivars().outils, &self.ivars().dossiers] {
            let ancienne = cellule.borrow_mut().take();
            if let Some(ancienne) = ancienne {
                let nouvelle = panneau(self.mtm(), ancienne.frame(), verre);
                nouvelle.setAutoresizingMask(ancienne.autoresizingMask());
                let contenu: Option<Retained<NSView>> = if est_verre(&ancienne) {
                    unsafe { msg_send![&*ancienne, contentView] }
                } else {
                    ancienne.subviews().firstObject()
                };
                if let Some(contenu) = contenu {
                    contenu.removeFromSuperview();
                    remplir(&nouvelle, &contenu);
                }
                nouvelle.setHidden(ancienne.isHidden());
                ancienne.removeFromSuperview();
                self.addSubview(&nouvelle);
                *cellule.borrow_mut() = Some(nouvelle);
            }
        }
    }
    pub fn reveler(&self) {
        self.boutons_fenetre(false);
        if let Some(v) = self.ivars().outils.borrow().as_ref() {
            v.setHidden(false);
        }
    }
    fn boutons_fenetre(&self, cacher: bool) {
        if let Some(fenetre) = self.window() {
            for genre in [
                NSWindowButton::CloseButton,
                NSWindowButton::MiniaturizeButton,
                NSWindowButton::ZoomButton,
            ] {
                if let Some(bouton) = fenetre.standardWindowButton(genre) {
                    bouton.setHidden(cacher);
                }
            }
        }
    }
    pub fn masquer(&self) {
        self.boutons_fenetre(true);
        for panneau in [&self.ivars().outils, &self.ivars().dossiers] {
            if let Some(v) = panneau.borrow().as_ref() {
                v.setHidden(true);
            }
        }
    }
}

/// NSGlassEffectView sur macOS 26 ; materiau AppKit de repli auparavant.
/// Le choix Flat emploie une surface opaque, conforme a l'apparence systeme.
pub fn panneau(mtm: MainThreadMarker, cadre: NSRect, verre: bool) -> Retained<NSView> {
    if verre {
        if let Some(classe) = AnyClass::get(c"NSGlassEffectView") {
            let vue: Retained<NSView> = unsafe {
                let objet: objc2::rc::Allocated<NSView> = msg_send![classe, alloc];
                msg_send![objet, initWithFrame: cadre]
            };
            return vue;
        }
        let effet = NSVisualEffectView::new(mtm);
        effet.setFrame(cadre);
        unsafe {
            effet.setMaterial(NSVisualEffectMaterial::Popover);
            effet.setBlendingMode(NSVisualEffectBlendingMode::WithinWindow);
            effet.setState(NSVisualEffectState::FollowsWindowActiveState);
        }
        return Retained::into_super(effet);
    }
    let vue = NSView::new(mtm);
    vue.setFrame(cadre);
    vue.setWantsLayer(true);
    if let Some(couche) = vue.layer() {
        couche.setBackgroundColor(Some(&NSColor::windowBackgroundColor().CGColor()));
        couche.setCornerRadius(10.0);
    }
    vue
}

fn est_verre(vue: &NSView) -> bool {
    AnyClass::get(c"NSGlassEffectView").is_some_and(|classe| vue.isKindOfClass(classe))
}
pub fn remplir(parent: &NSView, enfant: &NSView) {
    enfant.setFrame(parent.bounds());
    enfant.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    // Le contenu de NSGlassEffectView doit etre pose dans contentView.
    if est_verre(parent) {
        unsafe {
            let _: () = msg_send![parent, setContentView: enfant];
        }
    } else {
        parent.addSubview(enfant);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn couverture_sans_bandes_ni_deformation() {
        let cadre = NSRect::new(NSPoint::ZERO, NSSize::new(1200.0, 400.0));
        let image = NSSize::new(1600.0, 900.0);
        let r = cadrage(image, cadre, false);
        assert_eq!(r.size.width, 1200.0);
        assert_eq!(r.size.height, 675.0);
        assert_eq!(r.origin.y, -137.5);
        let entier = cadrage(image, cadre, true);
        assert_eq!(entier.size.height, 400.0);
        assert!(entier.origin.x > 0.0);
    }
}
