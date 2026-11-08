//! Links auf die Produktseite (`site/` im Repo, ausgeliefert über Cloudflare Pages).
//!
//! Die Oberfläche ist derzeit deutsch, also zeigen die Links auf die deutschen
//! Seiten. Mit der englischen Lokalisierung (PRR-11) wählt `url` die Sprache.

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
    format!("{BASE_URL}{}", page.path(true))
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
