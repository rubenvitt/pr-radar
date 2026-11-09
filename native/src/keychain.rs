//! Token aus „Mit GitHub anmelden“ im Schlüsselbund des Macs, damit die Anmeldung einen
//! Neustart übersteht. Fehler landen nur im Protokoll (ohne Token): Ohne Schlüsselbund gilt die
//! Anmeldung wie früher bis zum Beenden. In Tests und auf anderen Systemen ohne Wirkung.

#[cfg(all(target_os = "macos", not(test)))]
mod imp {
    use security_framework::passwords::{
        delete_generic_password, get_generic_password, set_generic_password,
    };

    const SERVICE: &str = "dev.rubeen.pr-radar";
    const ACCOUNT: &str = "github-sign-in";
    /// `errSecItemNotFound`: kein Eintrag – der Normalfall ohne Anmeldung.
    const NOT_FOUND: i32 = -25300;

    pub fn load() -> Option<String> {
        match get_generic_password(SERVICE, ACCOUNT) {
            Ok(bytes) => String::from_utf8(bytes)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            Err(e) => {
                if e.code() != NOT_FOUND {
                    log::warn!("Anmeldung aus dem Schlüsselbund nicht lesbar: {e}");
                }
                None
            }
        }
    }

    pub fn save(token: &str) {
        if let Err(e) = set_generic_password(SERVICE, ACCOUNT, token.as_bytes()) {
            log::warn!("Anmeldung nicht im Schlüsselbund gespeichert: {e}");
        }
    }

    pub fn delete() {
        match delete_generic_password(SERVICE, ACCOUNT) {
            Ok(()) => {}
            Err(e) if e.code() == NOT_FOUND => {}
            Err(e) => log::warn!("Anmeldung nicht aus dem Schlüsselbund entfernt: {e}"),
        }
    }
}

#[cfg(not(all(target_os = "macos", not(test))))]
mod imp {
    pub fn load() -> Option<String> {
        None
    }

    pub fn save(_token: &str) {}

    pub fn delete() {}
}

pub use imp::{delete, load, save};
