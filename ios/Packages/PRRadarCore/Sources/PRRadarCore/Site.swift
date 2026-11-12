// Links auf die Produktseite (`site/` im Repo, ausgeliefert über Cloudflare).
// Gleiche Pfade wie `native/src/site.rs`.

import Foundation

public enum Site {
    /// Basis-URL der Produktseite, ohne abschließenden Schrägstrich.
    public static let baseURL = "https://pr-radar.rubeen.dev"
    /// Support läuft über GitHub Issues; die Vorlagen-Auswahl fragt App und Version ab.
    public static let reportIssueURL = URL(string: "https://github.com/rubenvitt/pr-radar/issues/new/choose")!

    public enum Page: CaseIterable, Sendable {
        case home, support, privacy

        /// Pfad der Seite in der jeweiligen Sprache.
        public func path(german: Bool) -> String {
            switch (self, german) {
            case (.home, false): "/"
            case (.home, true): "/de/"
            case (.support, false): "/support/"
            case (.support, true): "/de/support/"
            case (.privacy, false): "/privacy/"
            case (.privacy, true): "/de/datenschutz/"
            }
        }
    }

    /// URL einer Seite. Ohne Angabe in der Sprache, in der die App gerade läuft.
    public static func url(_ page: Page, german: Bool = Site.appIsGerman) -> URL {
        URL(string: baseURL + page.path(german: german))!
    }

    /// Sprache der App-Oberfläche (nicht nur die des Systems), damit Seite und App zusammenpassen.
    public static var appIsGerman: Bool {
        (Bundle.main.preferredLocalizations.first ?? "de").hasPrefix("de")
    }
}
