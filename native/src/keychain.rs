//! Token aus „Mit GitHub anmelden“ im Schlüsselbund des Macs, damit die Anmeldung einen
//! Neustart übersteht. Lesen meldet Fehler nur im Protokoll (ohne Token); Speichern und Löschen
//! melden sie dem Aufrufer, damit die Oberfläche nichts Falsches verspricht. Ohne Schlüsselbund
//! gilt die Anmeldung wie früher bis zum Beenden. In Tests und auf anderen Systemen ohne Wirkung.

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

    pub fn save(token: &str) -> anyhow::Result<()> {
        set_generic_password(SERVICE, ACCOUNT, token.as_bytes()).map_err(|e| {
            anyhow::anyhow!(trf!(
                "Could not save the sign-in in the keychain: {e}",
                "Anmeldung konnte nicht im Schlüsselbund gespeichert werden: {e}"
            ))
        })
    }

    /// Fehlt der Eintrag schon, ist das kein Fehler.
    pub fn delete() -> anyhow::Result<()> {
        match delete_generic_password(SERVICE, ACCOUNT) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == NOT_FOUND => Ok(()),
            Err(e) => Err(anyhow::anyhow!(trf!(
                "Could not remove the sign-in from the keychain: {e}",
                "Anmeldung konnte nicht aus dem Schlüsselbund entfernt werden: {e}"
            ))),
        }
    }
}

#[cfg(not(all(target_os = "macos", not(test))))]
mod imp {
    pub fn load() -> Option<String> {
        None
    }

    pub fn save(_token: &str) -> anyhow::Result<()> {
        Ok(())
    }

    pub fn delete() -> anyhow::Result<()> {
        Ok(())
    }
}

pub use imp::{delete, load, save};
