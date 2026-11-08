import Foundation
import Testing

@testable import PRRadarCore

/// Persistenz der Konfiguration: Datei fehlt, ist kaputt, wird angelegt, bleibt mit der macOS-App kompatibel.
@Suite struct ConfigStoreTests {

    init() { L10n.setLanguage("en") }  // Texte unabhängig von der Systemsprache
    /// Eigener, noch nicht existierender Ordner je Test: `<tmp>/prradar-<uuid>/nested/pr-radar/config.json`.
    private func tempStore() -> ConfigStore {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("prradar-\(UUID().uuidString)")
        return ConfigStore(url: dir.appendingPathComponent("nested/pr-radar/config.json"))
    }

    private func cleanup(_ store: ConfigStore) {
        let root = store.url.deletingLastPathComponent()  // …/pr-radar
            .deletingLastPathComponent()  // …/nested
            .deletingLastPathComponent()  // …/prradar-<uuid>
        try? FileManager.default.removeItem(at: root)
    }

    @Test func fehlendeDateiErgibtStandard() {
        let store = tempStore()
        #expect(!FileManager.default.fileExists(atPath: store.url.path))
        #expect(store.load() == Config())
    }

    @Test func kaputteDateiErgibtStandard() throws {
        let store = tempStore()
        defer { cleanup(store) }
        try FileManager.default.createDirectory(
            at: store.url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("{ kein json".utf8).write(to: store.url)
        #expect(store.load() == Config())

        try Data(#"["a/b"]"#.utf8).write(to: store.url)
        #expect(store.load() == Config(), "falscher Wurzeltyp")
    }

    @Test func speichernLegtOrdnerAnUndUeberschreibt() throws {
        let store = tempStore()
        defer { cleanup(store) }
        try store.save(Config(repos: ["a/b"]))
        #expect(store.load().repos == ["a/b"])

        var prefs = Prefs()
        prefs.tab = .releases
        prefs.quick = .conflict
        prefs.view = .flow
        prefs.grouped = false
        prefs.repoFilter = ["c/d"]
        prefs.notify = true
        let full = Config(repos: ["c/d", "e/f"], prefs: prefs)
        try store.save(full)
        #expect(store.load() == full)
    }

    @Test func schreibtFormatDerMacOSApp() throws {
        let store = tempStore()
        defer { cleanup(store) }
        var cfg = Config(repos: ["a/b"])
        cfg.prefs.repoFilter = ["a/b"]
        try store.save(cfg)

        let text = try String(contentsOf: store.url, encoding: .utf8)
        #expect(text.contains(#""repo_filter""#), "snake_case wie in native/")
        #expect(!text.contains("repoFilter"))
        #expect(text.contains("\n"), "lesbar formatiert")

        let json = try #require(try JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
        #expect(Set(json.keys) == ["repos", "prefs"])
        let prefs = try #require(json["prefs"] as? [String: Any])
        #expect(Set(prefs.keys) == ["tab", "quick", "view", "grouped", "repo_filter", "notify"])
        #expect(prefs["tab"] as? String == "open")
        #expect(prefs["view"] as? String == "list")
    }

    @Test func einzelneUngueltigeFelderKippenNichtDenRest() throws {
        let json = #"""
            { "repos": "nicht-liste",
              "prefs": { "tab": "merged", "quick": 3, "view": "gibtsnicht", "grouped": "ja", "notify": true } }
            """#
        let cfg = try JSONDecoder().decode(Config.self, from: Data(json.utf8))
        #expect(cfg.repos == [])
        #expect(cfg.prefs.tab == .merged)
        #expect(cfg.prefs.quick == .all)
        #expect(cfg.prefs.view == .list)
        #expect(cfg.prefs.grouped)
        #expect(cfg.prefs.notify)

        let badPrefs = try JSONDecoder().decode(Config.self, from: Data(#"{ "repos": ["a/b"], "prefs": 1 }"#.utf8))
        #expect(badPrefs == Config(repos: ["a/b"]))
    }

    @Test(arguments: RadarTab.allCases)
    func alleTabsUeberlebenDieDatei(tab: RadarTab) throws {
        var prefs = Prefs()
        prefs.tab = tab
        let data = try JSONEncoder().encode(prefs)
        #expect(try JSONDecoder().decode(Prefs.self, from: data).tab == tab)
    }

    @Test(arguments: Quick.allCases)
    func alleSchnellfilterUeberlebenDieDatei(quick: Quick) throws {
        var prefs = Prefs()
        prefs.quick = quick
        let data = try JSONEncoder().encode(prefs)
        #expect(try JSONDecoder().decode(Prefs.self, from: data).quick == quick)
    }

    @Test func standardPfad() {
        #expect(ConfigStore.standard.url.path.hasSuffix("pr-radar/config.json"))
    }
}
