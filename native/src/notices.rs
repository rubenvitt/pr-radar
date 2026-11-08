//! Drittanbieter-Hinweise: Lizenzen der Rust-Abhängigkeiten und der Icons.
//!
//! `scripts/bundle.sh` erzeugt sie mit `cargo about` (Vorlage `about.hbs`) und legt
//! sie als `Contents/Resources/ThirdPartyNotices.html` ins App-Bundle. Unter
//! `cargo run` gibt es die Datei nicht; dann öffnet das Menü den Lizenzabschnitt
//! im README.

use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "ThirdPartyNotices.html";

/// Ort der Hinweise im App-Bundle: aus `…/Contents/MacOS/pr-radar` wird
/// `…/Contents/Resources/ThirdPartyNotices.html`.
pub fn bundled_path(exe: &Path) -> Option<PathBuf> {
    let contents = exe.parent()?.parent()?;
    Some(contents.join("Resources").join(FILE_NAME))
}

/// Die Hinweise der laufenden App, falls sie aus einem Bundle mit Hinweisen startet.
pub fn locate() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    bundled_path(&exe).filter(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn findet_hinweise_neben_der_programmdatei_im_bundle() {
        let exe = Path::new("/Applications/PR Radar.app/Contents/MacOS/pr-radar");
        assert_eq!(
            bundled_path(exe),
            Some(PathBuf::from(
                "/Applications/PR Radar.app/Contents/Resources/ThirdPartyNotices.html"
            ))
        );
    }

    #[test]
    fn ohne_elternordner_kein_pfad() {
        assert_eq!(bundled_path(Path::new("pr-radar")), None);
    }

    #[test]
    fn ausserhalb_eines_bundles_nichts_gefunden() {
        // Unter `cargo test` liegt die Programmdatei in target/, ohne Resources-Ordner.
        assert_eq!(locate(), None);
    }
}
