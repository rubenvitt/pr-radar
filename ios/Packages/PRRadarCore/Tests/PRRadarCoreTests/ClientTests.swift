import Foundation
import Testing

@testable import PRRadarCore

#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

/// Fehlerpfade des `GitHubClient`: HTTP-Status, Netz, Abbruch, unlesbare Antworten, GraphQL-Fehler.
@Suite struct ClientErrorTests {
    private func fetchError(_ transport: HTTPTransport, repos: [String] = ["a/b"]) async -> Error? {
        do {
            _ = try await GitHubClient(transport: transport, token: { "t" }).fetch(repos: repos)
            return nil
        } catch {
            return error
        }
    }

    @Test func http401IstAbgelehnterToken() async {
        let transport = ScriptedTransport(status: 401, body: #"{"message":"Bad credentials"}"#)
        let error = await fetchError(transport)
        #expect(error as? GitHubError == .unauthorized)
        #expect(error?.localizedDescription.contains("401") == true)
    }

    @Test(arguments: [404, 422, 500, 502, 503, 504])
    func httpStatusWirdZuHttpFehlerMitMeldung(status: Int) async {
        let transport = ScriptedTransport(status: status, body: #"{"message":"Nope"}"#)
        let error = await fetchError(transport)
        #expect(error as? GitHubError == .http(status, "Nope"))
    }

    @Test func httpTexteJeStatus() {
        #expect(describeHTTPError(404, message: "Not Found").contains("404"))
        #expect(describeHTTPError(404, message: "Not Found").contains("„Not Found“"))
        #expect(describeHTTPError(408, message: "").contains("nicht rechtzeitig"))
        #expect(describeHTTPError(504, message: "").contains("nicht rechtzeitig"))
        #expect(describeHTTPError(503, message: "x").contains("automatisch"))
        #expect(!describeHTTPError(503, message: "x").contains("„x“"), "Bei 5xx keine GitHub-Meldung anhängen")
        #expect(describeHTTPError(422, message: "  ").hasSuffix("abgelehnt."), "Leere Meldung ohne Anhang")
        // Lange Meldungen werden gekürzt
        let long = String(repeating: "x", count: 500)
        #expect(describeHTTPError(422, message: long).count < 300)
    }

    @Test func gitHubMessageNurAusJSON() {
        #expect(gitHubMessage(Data(#"{"message":"Bad"}"#.utf8)) == "Bad")
        #expect(gitHubMessage(Data("<html>".utf8)) == "")
        #expect(gitHubMessage(Data(#"{"documentation_url":"u"}"#.utf8)) == "")
    }

    @Test func netzfehlerWirdZuNichtErreichbar() async {
        let transport = ScriptedTransport { _ in throw URLError(.notConnectedToInternet) }
        let error = await fetchError(transport)
        guard case .unreachable? = error as? GitHubError else {
            Issue.record("unreachable erwartet, war \(String(describing: error))")
            return
        }
        #expect(error?.localizedDescription.hasPrefix("GitHub nicht erreichbar") == true)
    }

    @Test func abbruchBleibtAbbruch() async {
        let transport = ScriptedTransport { _ in throw CancellationError() }
        #expect(await fetchError(transport) is CancellationError, "kein Fehlerbanner beim Abbrechen")
    }

    @Test func unlesbareAntwort() async {
        let error = await fetchError(ScriptedTransport(status: 200, body: "<html>kaputt</html>"))
        guard case .unreadable? = error as? GitHubError else {
            Issue.record("unreadable erwartet, war \(String(describing: error))")
            return
        }
    }

    @Test func graphQLFehlerOhneDaten() async {
        let both = ScriptedTransport(
            status: 200, body: #"{"data":null,"errors":[{"message":"Eins"},{"message":"Zwei"}]}"#)
        #expect(await fetchError(both) as? GitHubError == .graphQL("Eins; Zwei"))

        let empty = ScriptedTransport(status: 200, body: "{}")
        #expect(await fetchError(empty) as? GitHubError == .graphQL("Leere Antwort von GitHub"))
    }

    @Test func einzelneReposScheiternOhneDenAbrufZuKippen() async throws {
        let body = #"""
            { "data": { "viewer": { "login": "me" }, "r0": null, "r1": 42, "r2": null },
              "errors": [{ "message": "Kein Zugriff auf c/d", "path": ["r2"] }] }
            """#
        let client = GitHubClient(transport: ScriptedTransport(status: 200, body: body), token: { "t" })
        let s = try await client.fetch(repos: ["a/b", "x/y", "c/d"]).snapshot
        #expect(s.viewer == "me")
        #expect(s.repos.map(\.fullName) == ["a/b", "x/y", "c/d"])
        #expect(s.repos[0].error == "Repository nicht gefunden oder kein Zugriff")
        #expect(s.repos[1].error?.hasPrefix("Antwort nicht lesbar") == true)
        #expect(s.repos[2].error == "Kein Zugriff auf c/d")
    }

    @Test func fehlerImErstenChunkZuerst() async {
        // 9 Repos → 2 Chunks; Chunk 0 nicht erreichbar, Chunk 1 HTTP 500 → Chunk 0 wird gemeldet
        let repos = (0..<9).map { "o/r\($0)" }
        let transport = ScriptedTransport { req in
            let query = req.graphQLBody["query"] as? String ?? ""
            if query.contains(#""r8""#) { return (Data(), 500, [:]) }
            throw URLError(.timedOut)
        }
        let error = await fetchError(transport, repos: repos)
        guard case .unreachable? = error as? GitHubError else {
            Issue.record("unreachable aus Chunk 0 erwartet, war \(String(describing: error))")
            return
        }
        #expect(transport.requests.count == 2)
    }

    @Test func drosselungInIrgendeinemChunkGewinnt() async {
        let repos = (0..<9).map { "o/r\($0)" }
        let transport = ScriptedTransport { req in
            let query = req.graphQLBody["query"] as? String ?? ""
            if query.contains(#""r8""#) { return (Data(), 429, ["retry-after": "60"]) }
            return (Data(), 500, [:])
        }
        let error = await fetchError(transport, repos: repos)
        guard case .rateLimited(retryAt: _?)? = error as? GitHubError else {
            Issue.record("rateLimited mit Zeit erwartet, war \(String(describing: error))")
            return
        }
    }

    @Test func keineReposKeinRequest() async throws {
        let transport = ScriptedTransport(status: 500, body: "")
        let fetched = try await GitHubClient(transport: transport, token: { "t" }).fetch(repos: [])
        #expect(fetched.snapshot.repos.isEmpty)
        #expect(transport.requests.isEmpty)
    }
}

/// Form der Requests und Mutationen.
@Suite struct ClientRequestTests {
    @Test func requestFormUndGetrimmterToken() async throws {
        let transport = ScriptedTransport(status: 200, body: #"{"data":{"viewer":{"login":"me"}}}"#)
        _ = try await GitHubClient(transport: transport, token: { "  t0k\n" }).fetch(repos: [])
        _ = try await GitHubClient(transport: transport, token: { "  t0k\n" }).fetch(repos: ["a/b"])
        let req = try #require(transport.requests.first)
        #expect(req.httpMethod == "POST")
        #expect(req.url?.absoluteString == "https://api.github.com/graphql")
        #expect(req.value(forHTTPHeaderField: "Authorization") == "bearer t0k")
        #expect(req.value(forHTTPHeaderField: "Content-Type") == "application/json")
        #expect(req.value(forHTTPHeaderField: "User-Agent") == "pr-radar-ios")
        #expect(req.timeoutInterval == 30)
        let query = try #require(req.graphQLBody["query"] as? String)
        #expect(query.contains(#"r0: repository(owner: "a", name: "b")"#))
    }

    @Test func tokenWirdJeRequestGelesen() async throws {
        let current = Locked<String?>(nil)
        let transport = ScriptedTransport(status: 200, body: #"{"data":{}}"#)
        let client = GitHubClient(transport: transport, token: { current.value })

        await #expect(throws: GitHubError.noToken) { _ = try await client.fetch(repos: ["a/b"]) }
        #expect(transport.requests.isEmpty, "ohne Token geht kein Request raus")

        current.value = "neu"
        _ = try await client.fetch(repos: ["a/b"])
        #expect(transport.requests.last?.value(forHTTPHeaderField: "Authorization") == "bearer neu")
    }

    @Test func mergeSchicktMethodeUndHead() async throws {
        let transport = ScriptedTransport(status: 200, body: #"{"data":{}}"#)
        try await GitHubClient(transport: transport, token: { "t" }).merge(prID: "PR_1", method: .squash, head: "abc")
        let body = try #require(transport.requests.first?.graphQLBody)
        #expect((body["query"] as? String)?.contains("mergePullRequest") == true)
        let vars = try #require(body["variables"] as? [String: Any])
        #expect(vars["id"] as? String == "PR_1")
        #expect(vars["method"] as? String == "SQUASH")
        #expect(vars["head"] as? String == "abc")
    }

    @Test func autoMergeOhneHeadSchicktNull() async throws {
        let transport = ScriptedTransport(status: 200, body: #"{"data":{}}"#)
        let client = GitHubClient(transport: transport, token: { "t" })
        try await client.setAutoMerge(prID: "PR_1", method: .rebase, head: nil)
        try await client.setAutoMerge(prID: "PR_1", method: nil, head: "abc")

        let enable = transport.requests[0].graphQLBody
        #expect((enable["query"] as? String)?.contains("enablePullRequestAutoMerge") == true)
        let vars = try #require(enable["variables"] as? [String: Any])
        #expect(vars["method"] as? String == "REBASE")
        #expect(vars.keys.contains("head"), "head muss als null mitgehen, nicht fehlen")
        #expect(vars["head"] is NSNull)

        let disable = transport.requests[1].graphQLBody
        #expect((disable["query"] as? String)?.contains("disablePullRequestAutoMerge") == true)
        #expect((disable["variables"] as? [String: Any])?.keys.sorted() == ["id"])
    }

    @Test func mutationMeldetGraphQLFehler() async {
        let transport = ScriptedTransport(
            status: 200,
            body: #"{"data":{"mergePullRequest":null},"errors":[{"message":"Base branch was modified"},{"message":"zweiter"}]}"#)
        let client = GitHubClient(transport: transport, token: { "t" })
        await #expect(throws: GitHubError.graphQL("Base branch was modified; zweiter")) {
            try await client.merge(prID: "PR_1", method: .merge, head: "abc")
        }
        await #expect(throws: GitHubError.graphQL("Base branch was modified; zweiter")) {
            try await client.setAutoMerge(prID: "PR_1", method: nil, head: nil)
        }
    }

    @Test func mutationMitHttpFehlern() async {
        let unauthorized = GitHubClient(transport: ScriptedTransport(status: 401, body: ""), token: { "t" })
        await #expect(throws: GitHubError.unauthorized) {
            try await unauthorized.merge(prID: "PR_1", method: .merge, head: "abc")
        }
        let forbidden = GitHubClient(
            transport: ScriptedTransport(status: 403, body: #"{"message":"Must have admin rights"}"#), token: { "t" })
        await #expect(throws: GitHubError.http(403, "Must have admin rights")) {
            try await forbidden.setAutoMerge(prID: "PR_1", method: .merge, head: "abc")
        }
        let noToken = GitHubClient(transport: ScriptedTransport(status: 200, body: "{}"), token: { nil })
        await #expect(throws: GitHubError.noToken) {
            try await noToken.merge(prID: "PR_1", method: .merge, head: "abc")
        }
    }
}

/// Branch-Regeln (Rulesets) über den Client: Abruf, Kodierung, Fehler, Cache.
@Suite struct ClientRulesTests {
    private let squashOnly = #"[{ "type": "pull_request", "parameters": { "allowed_merge_methods": ["squash"] } }]"#

    /// GraphQL aus der Fixture, Regeln über `rules`; protokolliert alle Requests.
    private func transport(
        baseRef: String = "main", rules: @escaping @Sendable (URLRequest) throws -> ScriptedTransport.Reply
    ) throws -> ScriptedTransport {
        var json = String(decoding: try fixture("dashboard"), as: UTF8.self)
        json = json.replacingOccurrences(of: #""baseRefName": "main""#, with: #""baseRefName": "\#(baseRef)""#)
        let graphQL = Data(json.utf8)
        return ScriptedTransport { req in
            req.isGraphQL ? (graphQL, 200, [:]) : try rules(req)
        }
    }

    private func methods(_ transport: ScriptedTransport) async throws -> [MergeMethod] {
        let client = GitHubClient(transport: transport, token: { "t" })
        return try await client.fetch(repos: ["rubeen/pr-radar", "rubeen/geheim"]).snapshot.open[0].mergeMethods
    }

    @Test func regelnWerdenJeBranchEinmalAbgerufen() async throws {
        let rules = Data(squashOnly.utf8)
        let t = try transport { _ in (rules, 200, [:]) }
        #expect(try await methods(t) == [.squash])

        let ruleRequests = t.requests.filter { !$0.isGraphQL }
        #expect(ruleRequests.count == 1, "zwei PRs auf main → ein Abruf")
        let req = try #require(ruleRequests.first)
        #expect(req.url?.absoluteString == "https://api.github.com/repos/rubeen/pr-radar/rules/branches/main")
        #expect(req.value(forHTTPHeaderField: "Accept") == "application/vnd.github+json")
        #expect(req.value(forHTTPHeaderField: "Authorization") == "bearer t")
    }

    @Test func branchMitSchraegstrichWirdKodiert() async throws {
        let t = try transport(baseRef: "release/1.0 ä") { _ in (Data("[]".utf8), 200, [:]) }
        _ = try await methods(t)
        let url = try #require(t.requests.first { !$0.isGraphQL }?.url?.absoluteString)
        #expect(url.hasSuffix("/rules/branches/release%2F1.0%20%C3%A4"))
    }

    @Test(arguments: [403, 404, 500])
    func abruffehlerFallenAufRepoMethodenZurueck(status: Int) async throws {
        let t = try transport { _ in (Data(#"{"message":"x"}"#.utf8), status, [:]) }
        #expect(try await methods(t) == [.squash, .merge], "Repo erlaubt Squash und Merge")
    }

    @Test func netzfehlerUndUnlesbareRegelnFallenZurueck() async throws {
        let offline = try transport { _ in throw URLError(.timedOut) }
        #expect(try await methods(offline) == [.squash, .merge])

        let garbage = try transport { _ in (Data("kaputt".utf8), 200, [:]) }
        #expect(try await methods(garbage) == [.squash, .merge])

        let notArray = try transport { _ in (Data(#"{"message":"ok"}"#.utf8), 200, [:]) }
        #expect(try await methods(notArray) == [.squash, .merge])
    }

    @Test func regelnWerdenZwischengespeichert() async throws {
        let rules = Data(squashOnly.utf8)
        let t = try transport { _ in (rules, 200, [:]) }
        let client = GitHubClient(transport: t, token: { "t" })
        _ = try await client.fetch(repos: ["rubeen/pr-radar"])
        let second = try await client.fetch(repos: ["rubeen/pr-radar"])
        #expect(second.snapshot.open[0].mergeMethods == [.squash])
        #expect(t.requests.filter { !$0.isGraphQL }.count == 1, "innerhalb von rulesTTL kein zweiter Abruf")
        #expect(t.requests.filter(\.isGraphQL).count == 2)
    }

    @Test func fehlgeschlageneAbrufeWerdenNichtGespeichert() async throws {
        let attempts = Locked(0)
        let rules = Data(squashOnly.utf8)
        let t = try transport { _ in
            attempts.value += 1
            return attempts.value == 1 ? (Data(), 500, [:]) : (rules, 200, [:])
        }
        let client = GitHubClient(transport: t, token: { "t" })
        #expect(try await client.fetch(repos: ["rubeen/pr-radar"]).snapshot.open[0].mergeMethods == [.squash, .merge])
        #expect(try await client.fetch(repos: ["rubeen/pr-radar"]).snapshot.open[0].mergeMethods == [.squash])
        #expect(attempts.value == 2, "nach einem Fehler wird beim nächsten Abruf erneut gefragt")
    }
}

/// Kleiner, threadsicherer Wert für Closures in Tests.
final class Locked<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private var stored: Value

    init(_ value: Value) { stored = value }

    var value: Value {
        get { lock.withLock { stored } }
        set { lock.withLock { stored = newValue } }
    }
}
