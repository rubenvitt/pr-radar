// Links auf die Produktseite (`site/` im Repo, ausgeliefert über Cloudflare Pages).
// Gleiche Pfade wie `native/src/site.rs`.

import Foundation

public enum Site {
    /// Basis-URL der Produktseite, ohne abschließenden Schrägstrich.
    public static let baseURL = "https://pr-radar.rubeen.dev"
    public static let supportEmail = "pr-radar@rubeen.dev"

    public enum Page: CaseIterable, Sendable {
        case home, support, privacy, terms, imprint

        /// Pfad der Seite; das Impressum ist zweisprachig und hat nur einen Pfad.
        public func path(german: Bool) -> String {
            switch (self, german) {
            case (.home, false): "/"
            case (.home, true): "/de/"
            case (.support, false): "/support/"
            case (.support, true): "/de/support/"
            case (.privacy, false): "/privacy/"
            case (.privacy, true): "/de/datenschutz/"
            case (.terms, false): "/terms/"
            case (.terms, true): "/de/eula/"
            case (.imprint, _): "/imprint/"
            }
        }
    }

    /// URL einer Seite. Ohne Angabe in der Sprache, in der die App gerade läuft.
    public static func url(_ page: Page, german: Bool = Site.appIsGerman) -> URL {
        URL(string: baseURL + page.path(german: german))!
    }

    public static var supportMail: URL {
        URL(string: "mailto:\(supportEmail)?subject=PR%20Radar")!
    }

    /// Sprache der App-Oberfläche (nicht nur die des Systems), damit Seite und App zusammenpassen.
    public static var appIsGerman: Bool {
        (Bundle.main.preferredLocalizations.first ?? "de").hasPrefix("de")
    }
}
