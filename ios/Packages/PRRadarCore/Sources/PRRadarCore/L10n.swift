// Texte des Kerns aus dem String Catalog `Resources/Localizable.xcstrings`.
// Englisch ist Quellsprache und Schlüssel, Deutsch die Übersetzung. Unter Linux (nur Tests)
// kompiliert SwiftPM den Catalog nicht, dort gelten die englischen Schlüssel.

import Foundation

enum L10n {
    static func tr(_ key: String) -> String {
        NSLocalizedString(key, bundle: .module, comment: "")
    }

    /// Mit Platzhaltern (`%@`, `%lld`) wie bei `String(format:)`.
    static func tr(_ key: String, _ args: CVarArg...) -> String {
        String(format: tr(key), locale: locale, arguments: args)
    }

    /// Aufgelöste Sprache (en/de) mit den Formaten des Systems, sofern dessen Sprache passt,
    /// sonst mit dessen Region (z. B. „en_DE“: englische Texte, 24-Stunden-Uhr).
    static var locale: Locale {
        let language = Bundle.module.preferredLocalizations.first ?? "en"
        let current = Locale.current
        if current.language.languageCode?.identifier == language { return current }
        guard let region = current.region?.identifier else { return Locale(identifier: language) }
        return Locale(identifier: "\(language)_\(region)")
    }
}
