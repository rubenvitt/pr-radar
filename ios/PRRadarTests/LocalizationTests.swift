// Die String Catalogs landen übersetzt in der App: Deutsch für App-Texte und für die Texte
// des Kerns (dessen Ressourcen-Bundle kompiliert nur Xcode, nicht `swift test`).

import Foundation
import Testing

@Suite struct LocalizationTests {
    /// Text aus der deutschen Übersetzung eines Bundles.
    private func german(_ key: String, in bundle: Bundle) -> String? {
        guard let path = bundle.path(forResource: "de", ofType: "lproj"), let de = Bundle(path: path) else {
            return nil
        }
        return de.localizedString(forKey: key, value: nil, table: nil)
    }

    @Test func appTexteSindAufDeutschDa() {
        #expect(german("Settings", in: .main) == "Einstellungen")
        #expect(german("No Repositories Yet", in: .main) == "Noch keine Repositories")
    }

    @Test func kernTexteSindAufDeutschDa() throws {
        let bundles = (try? FileManager.default.contentsOfDirectory(atPath: Bundle.main.bundlePath)) ?? []
        let name = try #require(bundles.first { $0.hasPrefix("PRRadarCore") && $0.hasSuffix(".bundle") })
        let core = try #require(Bundle(path: Bundle.main.bundleURL.appendingPathComponent(name).path))
        #expect(german("No checks", in: core) == "Keine Checks")
        #expect(german("Failed", in: core) == "Rot")
    }
}
