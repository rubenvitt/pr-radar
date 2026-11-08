// Vollständigkeit der String Catalogs: jeder Text hat eine deutsche Übersetzung mit
// denselben Platzhaltern, und jeder im Kern benutzte Schlüssel steht im Catalog.

import Foundation
import Testing

@testable import PRRadarCore

@Suite struct L10nTests {
    /// Wurzel des Repos `ios/`, ermittelt aus dem Pfad dieser Datei.
    static let iosRoot = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()  // PRRadarCoreTests
        .deletingLastPathComponent()  // Tests
        .deletingLastPathComponent()  // PRRadarCore
        .deletingLastPathComponent()  // Packages
        .deletingLastPathComponent()  // ios

    static let coreCatalog = iosRoot.appendingPathComponent(
        "Packages/PRRadarCore/Sources/PRRadarCore/Resources/Localizable.xcstrings")
    static let appCatalog = iosRoot.appendingPathComponent("PRRadar/Localizable.xcstrings")

    /// Schlüssel → alle deutschen Werte (bei Plural-Varianten mehrere).
    static func german(_ url: URL) throws -> [String: [String]] {
        let json = try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any]
        let strings = json?["strings"] as? [String: Any] ?? [:]
        var out: [String: [String]] = [:]
        for (key, entry) in strings {
            let entry = entry as? [String: Any]
            // Reine Formatschlüssel wie „%@ %@“ sind bewusst nicht übersetzt.
            if entry?["shouldTranslate"] as? Bool == false { continue }
            let de = (entry?["localizations"] as? [String: Any])?["de"]
            out[key] = values(de)
        }
        return out
    }

    /// Alle `stringUnit.value` unterhalb eines Eintrags (auch in `variations`).
    static func values(_ node: Any?) -> [String] {
        guard let dict = node as? [String: Any] else { return [] }
        if let unit = dict["stringUnit"] as? [String: Any], let value = unit["value"] as? String {
            return [value]
        }
        return dict.values.flatMap { values($0) }
    }

    /// Platzhalter ohne Position, sortiert: „%1$@ … %2$lld“ und „%lld … %@“ gelten als gleich.
    static func specifiers(_ s: String) -> [String] {
        let pattern = try! NSRegularExpression(pattern: "%(?:\\d+\\$)?(@|lld|ld|d|lf|f)")
        let range = NSRange(s.startIndex..., in: s)
        return pattern.matches(in: s, range: range).map { m in
            String(s[Range(m.range(at: 1), in: s)!])
        }.sorted()
    }

    @Test(arguments: [coreCatalog, appCatalog])
    func jederTextHatDeutsch(catalog: URL) throws {
        guard FileManager.default.fileExists(atPath: catalog.path) else {
            Issue.record("Catalog fehlt: \(catalog.path)")
            return
        }
        let entries = try Self.german(catalog)
        #expect(!entries.isEmpty)
        for (key, values) in entries {
            #expect(!values.isEmpty, "keine deutsche Übersetzung für „\(key)“")
            for value in values {
                #expect(!value.isEmpty, "leere Übersetzung für „\(key)“")
                // Plural-Varianten und Ersetzungen (`%#@name@`) prüfen ihre Platzhalter selbst.
                if values.count == 1 && !value.contains("%#@") {
                    #expect(Self.specifiers(value) == Self.specifiers(key), "Platzhalter von „\(key)“ → „\(value)“")
                }
            }
        }
    }

    @Test func kernSchluesselStehenImCatalog() throws {
        let entries = try Self.german(Self.coreCatalog)
        let sources = Self.iosRoot.appendingPathComponent("Packages/PRRadarCore/Sources/PRRadarCore")
        let files = try FileManager.default.contentsOfDirectory(atPath: sources.path).filter { $0.hasSuffix(".swift") }
        let call = try NSRegularExpression(pattern: #"L10n\.tr\("((?:[^"\\]|\\.)*)""#)
        var used = Set<String>()
        for file in files {
            let text = try String(contentsOf: sources.appendingPathComponent(file), encoding: .utf8)
            for m in call.matches(in: text, range: NSRange(text.startIndex..., in: text)) {
                used.insert(String(text[Range(m.range(at: 1), in: text)!]))
            }
        }
        #expect(used.count > 30)
        for key in used.sorted() {
            #expect(entries[key] != nil, "„\(key)“ fehlt im String Catalog des Kerns")
        }
    }
}
