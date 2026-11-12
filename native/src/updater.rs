//! Auto-Updates per [Sparkle](https://sparkle-project.org). `scripts/bundle.sh` legt
//! `Sparkle.framework` nur in Release-Builds mit EdDSA-Schlüssel ins Bündel und trägt Feed
//! (`SUFeedURL`) und öffentlichen Schlüssel (`SUPublicEDKey`) in die Info.plist ein. Die App lädt
//! das Framework zur Laufzeit; ohne es (`cargo run`, lokale Bündel ohne Schlüssel) gibt es keine
//! Updates, der Menüpunkt bleibt ausgegraut. Sparkle fragt beim zweiten Start, ob es selbst nach
//! Updates suchen darf, und prüft danach einmal am Tag.

#[cfg(target_os = "macos")]
mod imp {
    use std::cell::RefCell;

    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::{MainThreadMarker, msg_send};
    use objc2_foundation::{NSBundle, NSString};

    thread_local! {
        /// `SPUStandardUpdaterController`; muss so lange leben wie die App.
        static CONTROLLER: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
    }

    /// Lädt Sparkle und startet den Updater, falls das Bündel ihn mitbringt.
    pub fn start(_mtm: MainThreadMarker) {
        let main = NSBundle::mainBundle();
        let Some(frameworks) = main.privateFrameworksPath() else {
            return;
        };
        let path = format!("{frameworks}/Sparkle.framework");
        if !std::path::Path::new(&path).exists() {
            return;
        }
        if main
            .objectForInfoDictionaryKey(&NSString::from_str("SUFeedURL"))
            .is_none()
        {
            log::warn!("Sparkle liegt im Bündel, aber SUFeedURL fehlt: keine Updates");
            return;
        }
        let Some(bundle) = NSBundle::bundleWithPath(&NSString::from_str(&path)) else {
            log::warn!("Sparkle.framework nicht lesbar: {path}");
            return;
        };
        // SAFETY: Lädt das mit der App signierte Framework; Initialisierer von Sparkle
        // haben keine Vorbedingungen außer dem Hauptthread (MainThreadMarker).
        if let Err(e) = unsafe { bundle.loadAndReturnError() } {
            log::warn!("Sparkle konnte nicht geladen werden: {e}");
            return;
        }
        let Some(class) = AnyClass::get(c"SPUStandardUpdaterController") else {
            log::warn!("Sparkle geladen, aber SPUStandardUpdaterController fehlt");
            return;
        };
        // SAFETY: `-initWithStartingUpdater:updaterDelegate:userDriverDelegate:` aus der
        // öffentlichen API von Sparkle 2; beide Delegates dürfen nil sein.
        let controller: Option<Retained<AnyObject>> = unsafe {
            let allocated: Allocated<AnyObject> = msg_send![class, alloc];
            msg_send![
                allocated,
                initWithStartingUpdater: true,
                updaterDelegate: None::<&AnyObject>,
                userDriverDelegate: None::<&AnyObject>
            ]
        };
        match controller {
            Some(c) => {
                log::info!("Sparkle gestartet");
                CONTROLLER.with(|slot| *slot.borrow_mut() = Some(c));
            }
            None => log::warn!("Sparkle-Updater ließ sich nicht anlegen"),
        }
    }

    pub fn available() -> bool {
        CONTROLLER.with(|slot| slot.borrow().is_some())
    }

    /// „Nach Updates suchen …“: Sparkle zeigt Fortschritt und Ergebnis in eigenen Fenstern.
    pub fn check_for_updates() {
        CONTROLLER.with(|slot| {
            if let Some(controller) = slot.borrow().as_ref() {
                // SAFETY: `-checkForUpdates:` ist eine IBAction, der Absender darf nil sein.
                let _: () =
                    unsafe { msg_send![&**controller, checkForUpdates: None::<&AnyObject>] };
            }
        });
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn available() -> bool {
        false
    }

    pub fn check_for_updates() {}
}

#[cfg(target_os = "macos")]
pub use imp::start;
pub use imp::{available, check_for_updates};
