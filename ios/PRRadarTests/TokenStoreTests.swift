// TokenStore gegen den echten Schlüsselbund des Simulators – je Test ein eigener Dienstname,
// damit der Token der App unberührt bleibt.

import Foundation
import Security
import Testing

@testable import PRRadar

@Suite struct TokenStoreTests {
    /// Eigener Schlüsselbund-Eintrag und feste Umgebung je Test.
    private func makeStore(
        service: String = "email.rubeen.prradar.tests.\(UUID().uuidString)", environment: [String: String] = [:]
    ) -> TokenStore {
        TokenStore(service: service, environment: { environment })
    }

    @Test func ohneTokenNichts() {
        let store = makeStore()
        #expect(store.token == nil)
        #expect(store.source == nil)
    }

    @Test func speichertGetrimmtImSchluesselbund() throws {
        let service = "email.rubeen.prradar.tests.\(UUID().uuidString)"
        let store = makeStore(service: service)
        defer { store.delete() }

        try store.save("  ghp_abc\n")
        #expect(store.token == "ghp_abc")
        #expect(store.source == .keychain)

        // Neue Instanz ohne Cache liest aus dem Schlüsselbund
        #expect(makeStore(service: service).token == "ghp_abc")
    }

    @Test func leererTokenWirdNichtGespeichert() throws {
        let store = makeStore()
        try store.save("   \n")
        #expect(store.token == nil)
        #expect(store.source == nil)
    }

    @Test func neuerTokenErsetztDenAlten() throws {
        let service = "email.rubeen.prradar.tests.\(UUID().uuidString)"
        let store = makeStore(service: service)
        defer { store.delete() }

        try store.save("alt")
        try store.save("neu")
        #expect(store.token == "neu")
        #expect(makeStore(service: service).token == "neu")
    }

    @Test func entfernenLoeschtAuchImSchluesselbund() throws {
        let service = "email.rubeen.prradar.tests.\(UUID().uuidString)"
        let store = makeStore(service: service)
        try store.save("weg")
        store.delete()
        #expect(store.token == nil)
        #expect(store.source == nil)
        #expect(makeStore(service: service).token == nil)
    }

    @Test func umgebungAlsRueckfall() throws {
        let env = makeStore(environment: ["GITHUB_TOKEN": "  aus-env \n"])
        #expect(env.token == "aus-env")
        #expect(env.source == .environment)

        let gh = makeStore(environment: ["GITHUB_TOKEN": "  ", "GH_TOKEN": "gh"])
        #expect(gh.token == "gh", "leeres GITHUB_TOKEN zählt nicht")

        let empty = makeStore(environment: ["GITHUB_TOKEN": "", "GH_TOKEN": " "])
        #expect(empty.token == nil)
        #expect(empty.source == nil)
    }

    @Test func schluesselbundVorUmgebung() throws {
        let store = makeStore(environment: ["GITHUB_TOKEN": "env"])
        defer { store.delete() }
        try store.save("keychain")
        #expect(store.token == "keychain")
        #expect(store.source == .keychain)

        store.delete()
        #expect(store.token == "env", "nach dem Entfernen greift wieder die Umgebung")
        #expect(store.source == .environment)
    }

    @Test func merktSichDieHerkunftAusDerAnmeldung() throws {
        let service = "email.rubeen.prradar.tests.\(UUID().uuidString)"
        let store = makeStore(service: service)
        defer { store.delete() }

        try store.save("gho_login", fromLogin: true)
        #expect(store.source == .login)
        #expect(makeStore(service: service).source == .login, "Herkunft überlebt einen Neustart")

        // Ein eingetragener PAT ersetzt die Anmeldung
        try store.save("ghp_pat")
        #expect(store.source == .keychain)

        try store.save("gho_login", fromLogin: true)
        store.delete()
        try store.save("ghp_neu")
        #expect(store.source == .keychain, "Entfernen vergisst die Herkunft")
    }

    @Test func fehlermeldungNenntDenSchluesselbund() {
        let text = KeychainError(status: errSecInteractionNotAllowed).localizedDescription
        #expect(text.lowercased().contains("keychain"))
    }
}
