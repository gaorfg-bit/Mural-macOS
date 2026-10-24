//! Exerce le vrai demarrage de Mural avec/sans le selecteur macOS 14.
//! Ne change aucun fond et n'execute pas la boucle evenementielle.
use objc2::{
    ClassType, DefinedClass, MainThreadMarker, define_class, msg_send, rc::Retained, runtime::Sel,
    sel,
};
use objc2_app_kit::NSApplication;
use objc2_foundation::NSObjectProtocol;
use std::cell::Cell;

struct ActivationIvars {
    moderne_disponible: bool,
    modernes: Cell<usize>,
    anciennes: Cell<usize>,
    ignore_autres: Cell<bool>,
}

define_class!(
    #[unsafe(super(NSApplication))]
    #[name = "MuralRecetteActivation"]
    #[ivars = ActivationIvars]
    struct ApplicationRecette;

    unsafe impl NSObjectProtocol for ApplicationRecette {}
    impl ApplicationRecette {
        #[unsafe(method_id(init))]
        fn initialiser(this: objc2::rc::Allocated<Self>) -> Retained<Self> {
            let this = this.set_ivars(ActivationIvars {
                moderne_disponible: std::env::args().nth(1).as_deref() == Some("moderne"),
                modernes: Cell::new(0),
                anciennes: Cell::new(0),
                ignore_autres: Cell::new(false),
            });
            unsafe { msg_send![super(this), init] }
        }
        #[unsafe(method(respondsToSelector:))]
        fn repond(&self, action: Sel) -> bool {
            if action == sel!(activate) {
                self.ivars().moderne_disponible
            } else {
                unsafe { msg_send![super(self), respondsToSelector: action] }
            }
        }
        #[unsafe(method(activate))]
        fn activer(&self) {
            self.ivars().modernes.set(self.ivars().modernes.get() + 1);
        }
        #[unsafe(method(activateIgnoringOtherApps:))]
        fn activer_ancienne(&self, ignorer: bool) {
            self.ivars().anciennes.set(self.ivars().anciennes.get() + 1);
            self.ivars().ignore_autres.set(ignorer);
        }
        #[unsafe(method(run))]
        fn verifier(&self) {
            let i = self.ivars();
            let attendus = if i.moderne_disponible { (1, 0) } else { (0, 1) };
            let obtenus = (i.modernes.get(), i.anciennes.get());
            // L'activation historique doit respecter les autres apps (false).
            let ok = obtenus == attendus && !i.ignore_autres.get();
            println!("{} activation {} : appels modernes/anciens {obtenus:?}, attendus {attendus:?}",
                if ok { "OK" } else { "ECHEC" },
                if i.moderne_disponible { "moderne" } else { "macOS 13 simule" });
            std::process::exit(if ok { 0 } else { 1 });
        }
    }
);

fn main() {
    let reglages = std::env::var_os("MURAL_REGLAGES").expect("Reglages isoles obligatoires");
    assert!(std::path::Path::new(&reglages).starts_with("/private/tmp"));
    assert!(
        matches!(
            std::env::args().nth(1).as_deref(),
            Some("moderne" | "ancien")
        ),
        "Usage : recette_activation moderne|ancien"
    );
    let mtm = MainThreadMarker::new().unwrap();
    // NSApplication doit etre cree par sharedApplication sur sa sous-classe.
    let application: Retained<ApplicationRecette> =
        unsafe { msg_send![ApplicationRecette::class(), sharedApplication] };
    let partagee = NSApplication::sharedApplication(mtm);
    assert!(std::ptr::eq::<NSApplication>(&**application, &*partagee));
    mural::app::lancer(mtm, "/private/tmp/mural-activation-images-absentes".into());
}
