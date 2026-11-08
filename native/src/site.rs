//! Links auf die Produktseite (`site/` im Repo, ausgeliefert über Cloudflare Pages).
//!
//! Die Links folgen der Sprache der Oberfläche (`l10n`): Englisch oder Deutsch.

/// Basis-URL der Produktseite, ohne abschließenden Schrägstrich.
pub const BASE_URL: &str = "https://pr-radar.rubeen.dev";

/// Seiten, die die App verlinkt (Hilfemenü).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Support,
    Privacy,
    Terms,
    Imprint,
}

impl Page {
    #[cfg(test)]
    const ALL: [Page; 5] = [
        Page::Home,
        Page::Support,
        Page::Privacy,
        Page::Terms,
        Page::Imprint,
    ];

    /// Pfad der Seite; das Impressum ist zweisprachig und hat nur einen Pfad.
    pub fn path(self, german: bool) -> &'static str {
        match (self, german) {
            (Page::Home, false) => "/",
            (Page::Home, true) => "/de/",
            (Page::Support, false) => "/support/",
            (Page::Support, true) => "/de/support/",
            (Page::Privacy, false) => "/privacy/",
            (Page::Privacy, true) => "/de/datenschutz/",
            (Page::Terms, false) => "/terms/",
            (Page::Terms, true) => "/de/eula/",
            (Page::Imprint, _) => "/imprint/",
        }
    }
}

/// Vollständige URL einer Seite in der Sprache der Oberfläche.
pub fn url(page: Page) -> String {
    let german = crate::l10n::lang() == crate::l10n::Lang::De;
    format!("{BASE_URL}{}", page.path(german))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn urls_are_absolute_https() {
        for page in Page::ALL {
            let u = url(page);
            assert!(u.starts_with("https://"), "{u}");
            assert!(u.ends_with('/'), "{u}");
            assert!(!u["https://".len()..].contains("//"), "{u}");
        }
    }

    #[test]
    fn url_folgt_der_sprache() {
        use crate::l10n::{Lang, with_lang};
        let privacy = |lang| with_lang(lang, || url(Page::Privacy));
        assert_eq!(privacy(Lang::En), format!("{BASE_URL}/privacy/"));
        assert_eq!(privacy(Lang::De), format!("{BASE_URL}/de/datenschutz/"));
    }

    /// Jeder Link der App zeigt auf eine Seite, die es in `site/public` gibt.
    #[test]
    fn every_page_exists_in_site() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../site/public");
        for page in Page::ALL {
            for german in [false, true] {
                let path = page.path(german).trim_start_matches('/');
                let file = root.join(path).join("index.html");
                assert!(file.is_file(), "fehlt: {}", file.display());
            }
        }
    }
}
