// Texte des Kerns aus dem String Catalog `Resources/Localizable.xcstrings`.
// Englisch ist Quellsprache und Schlüssel, Deutsch die Übersetzung. Unter Linux (nur Tests)
// kompiliert SwiftPM den Catalog nicht, dort gelten die englischen Schlüssel.

import Foundation
import Synchronization

enum L10n {
    /// Feste Sprache statt der Systemsprache; Tests setzen „en“, damit sie auf jedem Rechner
    /// dieselben Texte sehen.
    private static let fixed = Mutex<String?>(nil)

    static func setLanguage(_ language: String?) {
        fixed.withLock { $0 = language }
    }

    static func tr(_ key: String) -> String {
        tr(key, language: fixed.withLock { $0 })
    }

    /// Text in einer bestimmten Sprache (`nil` = wie das System sie auflöst).
    static func tr(_ key: String, language: String?) -> String {
        guard let language else { return NSLocalizedString(key, bundle: .module, comment: "") }
        // Englisch ist die Quellsprache: Der Schlüssel ist der Text.
        guard language != "en",
            let path = Bundle.module.path(forResource: language, ofType: "lproj"),
            let bundle = Bundle(path: path)
        else { return key }
        return bundle.localizedString(forKey: key, value: key, table: nil)
    }

    /// Gibt es diese Sprache übersetzt? Englisch immer; Deutsch nur, wenn der Catalog
    /// kompiliert wurde (Xcode, nicht `swift test`).
    static func isAvailable(_ language: String) -> Bool {
        language == "en" || Bundle.module.path(forResource: language, ofType: "lproj") != nil
    }

    /// Mit Platzhaltern (`%@`, `%lld`) wie bei `String(format:)`.
    static func tr(_ key: String, _ args: CVarArg...) -> String {
        String(format: tr(key), locale: locale, arguments: args)
    }

    /// Aufgelöste Sprache (en/de) mit den Formaten des Systems, sofern dessen Sprache passt,
    /// sonst mit dessen Region (z. B. „en_DE“: englische Texte, 24-Stunden-Uhr).
    static var locale: Locale {
        let language = fixed.withLock { $0 } ?? Bundle.module.preferredLocalizations.first ?? "en"
        let current = Locale.current
        if current.language.languageCode?.identifier == language { return current }
        guard let region = current.region?.identifier else { return Locale(identifier: language) }
        return Locale(identifier: "\(language)_\(region)")
    }
}
