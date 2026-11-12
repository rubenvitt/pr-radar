//! Links auf die Produktseite (`site/` im Repo, ausgeliefert über Cloudflare).
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
}

impl Page {
    #[cfg(test)]
    const ALL: [Page; 3] = [Page::Home, Page::Support, Page::Privacy];

    /// Pfad der Seite in der jeweiligen Sprache.
    pub fn path(self, german: bool) -> &'static str {
        match (self, german) {
            (Page::Home, false) => "/",
            (Page::Home, true) => "/de/",
            (Page::Support, false) => "/support/",
            (Page::Support, true) => "/de/support/",
            (Page::Privacy, false) => "/privacy/",
            (Page::Privacy, true) => "/de/datenschutz/",
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
