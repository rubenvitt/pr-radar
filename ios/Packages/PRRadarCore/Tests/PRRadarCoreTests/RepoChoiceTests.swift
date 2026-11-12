// Repo-Auswahl beim Einrichten: Seiten lesen und blättern (GitHubClient), Laden, Anhaken und
// verzögertes Neuladen (Radar).

import Foundation
import Testing

@testable import PRRadarCore

#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

/// Antwort von `viewer.repositories` mit diesen Knoten.
private func repoPage(_ nodes: [[String: Any?]], next: String?) -> Data {
    let page: [String: Any] = [
        "data": [
            "viewer": [
                "repositories": [
                    "pageInfo": ["hasNextPage": next != nil, "endCursor": next as Any? ?? NSNull()],
                    "nodes": nodes.map { $0.mapValues { $0 ?? NSNull() } },
                ]
            ]
        ]
    ]
    return try! JSONSerialization.data(withJSONObject: page)
}

private func node(_ full: String, private isPrivate: Bool = false, archived: Bool = false, description: String? = nil)
    -> [String: Any?]
{
    ["nameWithOwner": full, "isPrivate": isPrivate, "isArchived": archived, "description": description]
}

@Suite struct RepoChoiceClientTests {

    init() { L10n.setLanguage("en") }  // Texte unabhängig von der Systemsprache

    @Test func seiteOhneArchivierteUndMitGekuerztenBeschreibungen() throws {
        let data = repoPage(
            [
                node("me/app", private: true, description: " Die App "),
                node("me/alt", archived: true),
                node("org/lib", description: ""),
                node("org/leer", description: nil),
            ], next: "c1")
        let res = try gitHubDecoder().decode(GQLEnvelope<ViewerReposData>.self, from: data)
        let page = try parseRepoPage(res)
        #expect(
            page.choices == [
                RepoChoice(full: "me/app", isPrivate: true, description: "Die App"),
                RepoChoice(full: "org/lib"),
                RepoChoice(full: "org/leer"),
            ])
        #expect(page.next == "c1")
    }

    @Test func letzteSeiteHatKeinenCursor() throws {
        let json = #"{"data":{"viewer":{"repositories":{"pageInfo":{"hasNextPage":false,"endCursor":"x"},"nodes":[null]}}}}"#
        let res = try gitHubDecoder().decode(GQLEnvelope<ViewerReposData>.self, from: Data(json.utf8))
        let page = try parseRepoPage(res)
        #expect(page.choices.isEmpty)
        #expect(page.next == nil, "endCursor zählt nur, wenn es eine nächste Seite gibt")
    }

    @Test func blaettertMitCursorBisZurLetztenSeite() async throws {
        let transport = ScriptedTransport { req in
            let after = (req.graphQLBody["variables"] as? [String: Any])?["after"]
            switch after as? String {
            case nil:
                #expect(after is NSNull, "erste Seite fragt mit after: null")
                return (repoPage([node("me/app"), node("me/alt", archived: true)], next: "c1"), 200, [:])
            case "c1"?:
                return (repoPage([node("org/lib")], next: nil), 200, [:])
            default:
                Issue.record("unerwarteter Cursor \(String(describing: after))")
                return (Data(), 500, [:])
            }
        }
        let got = try await GitHubClient(transport: transport, token: { "t" }).listRepos()
        #expect(got.map(\.full) == ["me/app", "org/lib"])
        #expect(transport.requests.count == 2)
        let query = transport.requests.first?.graphQLBody["query"] as? String ?? ""
        #expect(query.contains("affiliations: [OWNER, COLLABORATOR, ORGANIZATION_MEMBER]"))
        #expect(query.contains("PUSHED_AT"))
    }

    @Test func hoertBeiFuenfhundertAuf() async throws {
        let transport = ScriptedTransport { req in
            let after = (req.graphQLBody["variables"] as? [String: Any])?["after"] as? String
            let page = Int(after ?? "0")!
            let nodes = (0..<100).map { node("o/r\(page * 100 + $0)") }
            return (repoPage(nodes, next: String(page + 1)), 200, [:])
        }
        let got = try await GitHubClient(transport: transport, token: { "t" }).listRepos()
        #expect(got.count == repoChoicesMax)
        #expect(transport.requests.count == 5, "nach fünf vollen Seiten nicht weiterblättern")
    }

    @Test func graphQLFehlerWirdGemeldet() async {
        let transport = ScriptedTransport(status: 200, body: #"{"errors":[{"message":"Resource not accessible"}]}"#)
        await #expect(throws: GitHubError.graphQL("Resource not accessible")) {
            try await GitHubClient(transport: transport, token: { "t" }).listRepos()
        }
    }

    @Test func ohneTokenKeinRequest() async {
        let transport = ScriptedTransport(status: 200, body: "{}")
        await #expect(throws: GitHubError.noToken) {
            try await GitHubClient(transport: transport, token: { nil }).listRepos()
        }
        #expect(transport.requests.isEmpty)
    }
}

/// Wartet, bis die Auswahl nicht mehr lädt (das Laden läuft in einem eigenen Task).
@MainActor
private func loaded(_ radar: Radar) async {
    for _ in 0..<1000 where radar.repoChoices == .loading {
        await Task.yield()
        try? await Task.sleep(for: .milliseconds(1))
    }
}

@MainActor
@Suite struct RepoChoiceRadarTests {

    init() { L10n.setLanguage("en") }  // Texte unabhängig von der Systemsprache

    @Test func laedtEinmalUndNachFehlerErneut() async {
        let github = FakeGitHub(snapshot: Snapshot())
        await github.setChoices(.failure(.graphQL("kaputt")))
        let radar = Radar(github: github, store: nil, cache: nil, timing: FakeClock().timing)

        radar.ensureRepoChoices()
        #expect(radar.repoChoices == .loading)
        await loaded(radar)
        #expect(radar.repoChoices == .failed("kaputt"))

        await github.setChoices(.success([RepoChoice(full: "me/app")]))
        radar.ensureRepoChoices()
        await loaded(radar)
        #expect(radar.repoChoices == .loaded([RepoChoice(full: "me/app")]))

        radar.ensureRepoChoices()
        #expect(radar.repoChoices == .loaded([RepoChoice(full: "me/app")]), "geladen → nicht erneut fragen")
        #expect(await github.listCalls == 2)

        radar.resetRepoChoices()
        #expect(radar.repoChoices == .idle)
    }

    @Test func anhakenSpeichertSofortUndIgnoriertGrossKlein() throws {
        let radar = Radar(
            github: FakeGitHub(snapshot: Snapshot()), store: nil, cache: nil, timing: FakeClock().timing,
            initialConfig: Config(repos: ["Me/App"]))
        radar.updatePrefs { $0.repoFilter = ["Me/App"] }

        #expect(radar.isWatched("me/app"))
        try radar.setWatched("me/app", true)
        #expect(radar.repos == ["Me/App"], "schon beobachtet → nichts doppelt")

        try radar.setWatched("org/lib", true)
        #expect(radar.repos == ["Me/App", "org/lib"])

        try radar.setWatched("me/app", false)
        #expect(radar.repos == ["org/lib"])
        #expect(radar.prefs.repoFilter.isEmpty, "Filter auf ein entferntes Repo fällt weg")
        radar.stop()
    }

    @Test func mehrereHaekchenLoesenEinenAbrufAus() async throws {
        let github = FakeGitHub(snapshot: Snapshot())
        let clock = FakeClock()
        let radar = Radar(github: github, store: nil, cache: nil, timing: clock.timing)

        try radar.setWatched("o/a", true)
        try radar.setWatched("o/b", true)
        try radar.setWatched("o/c", true)
        try radar.setWatched("o/b", false)
        #expect(await github.fetches.isEmpty, "Anhaken lädt nicht sofort")

        await radar.settleTask?.value
        // Der Poll-Zyklus läuft jetzt; auf den ersten Abruf warten und dann anhalten.
        for _ in 0..<1000 where await github.fetches.isEmpty {
            await Task.yield()
        }
        radar.stop()
        let fetches = await github.fetches
        #expect(fetches.first == ["o/a", "o/c"])
        #expect(clock.sleeps.filter { $0 == selectionSettle }.count == 1, "nur das letzte Häkchen wartet zu Ende")
    }
}
