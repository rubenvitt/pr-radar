import Foundation
import Testing

@testable import PRRadarCore

private func rollup(_ json: String) throws -> RawRollup {
    try gitHubDecoder().decode(RawRollup.self, from: Data(json.utf8))
}

private func run(_ status: String, _ conclusion: String?, _ name: String) -> String {
    let c = conclusion.map { "\"\($0)\"" } ?? "null"
    return #"{ "__typename": "CheckRun", "name": "\#(name)", "status": "\#(status)", "conclusion": \#(c), "detailsUrl": null }"#
}

@Suite struct PipelineTests {
    @Test func noneOhneChecks() {
        #expect(buildPipeline(nil).state == .none)
    }

    @Test func rotSchlaegtLaufend() throws {
        let r = try rollup(
            #"{ "state": "PENDING", "contexts": { "nodes": [\#(run("IN_PROGRESS", nil, "a")), \#(run("COMPLETED", "FAILURE", "b"))] } }"#)
        let p = buildPipeline(r)
        #expect(p.state == .failed)
        #expect((p.failed, p.running, p.total) == (1, 1, 2))
        #expect(p.checks[0].name == "b", "Fehler zuerst")
    }

    @Test func laufendSolangeEtwasNichtFertigIst() throws {
        let r = try rollup(
            #"{ "state": "PENDING", "contexts": { "nodes": [\#(run("QUEUED", nil, "ci")), \#(run("COMPLETED", "SUCCESS", "ci"))] } }"#)
        #expect(buildPipeline(r).state == .running)
    }

    @Test func gruenMitUebersprungenenChecks() throws {
        let r = try rollup(
            #"{ "state": "SUCCESS", "contexts": { "nodes": [\#(run("COMPLETED", "SUCCESS", "ci")), \#(run("COMPLETED", "SKIPPED", "ci"))] } }"#)
        #expect(buildPipeline(r).state == .passed)
    }

    @Test func verstehtKlassischeCommitStatuses() throws {
        let r = try rollup(
            #"{ "state": "FAILURE", "contexts": { "nodes": [{ "__typename": "StatusContext", "context": "ci/jenkins", "state": "ERROR", "targetUrl": "x" }] } }"#)
        #expect(buildPipeline(r).state == .failed)
    }

    @Test func ignoriertUnbekannteKontexteUndNull() throws {
        let r = try rollup(#"{ "state": "SUCCESS", "contexts": { "nodes": [null, { "__typename": "Something" }] } }"#)
        let p = buildPipeline(r)
        #expect(p.state == .passed)
        #expect(p.total == 0)
    }

    @Test func mapptRollupStates() {
        #expect(rollupState("EXPECTED") == .running)
        #expect(rollupState(nil) == .none)
    }
}

@Suite struct RepoTests {
    private func rawRepo(_ flags: String) throws -> RawRepo {
        let json = #"{ "nameWithOwner": "a/b", "url": "u", "autoMergeAllowed": true, "viewerPermission": null, "defaultBranchRef": null \#(flags) }"#
        return try gitHubDecoder().decode(RawRepo.self, from: Data(json.utf8))
    }

    @Test func nurErlaubteMergeMethoden() throws {
        #expect(normalizeRepo(try rawRepo(#", "squashMergeAllowed": true"#)).mergeMethods == [.squash])
        #expect(
            normalizeRepo(try rawRepo(#", "mergeCommitAllowed": true, "rebaseMergeAllowed": true"#)).mergeMethods
                == [.merge, .rebase])
        #expect(normalizeRepo(try rawRepo("")).mergeMethods.isEmpty)
    }

    @Test func rulesetsSchraenkenMergeMethodenEin() throws {
        let all: [MergeMethod] = [.squash, .merge, .rebase]
        let decode = { (s: String) in try JSONDecoder().decode([RawRule].self, from: Data(s.utf8)) }
        let onlyMerge = try decode(#"[{ "type": "pull_request", "parameters": { "allowed_merge_methods": ["merge"] } }]"#)
        #expect(applyBranchRules(all, onlyMerge) == [.merge])
        let linear = try decode(#"[{ "type": "required_linear_history" }, { "type": "deletion" }]"#)
        #expect(applyBranchRules(all, linear) == [.squash, .rebase])
        #expect(applyBranchRules([.squash], onlyMerge) == [])
        #expect(applyBranchRules(all, []) == all)
    }

    @Test func normalisiertOffenenPR() throws {
        let json = #"""
            {
              "id": "PR_1", "number": 7, "title": "Feat", "url": "u", "isDraft": false,
              "createdAt": "2026-09-30T10:00:00Z", "updatedAt": "2026-09-30T11:00:00Z",
              "headRefName": "feat", "baseRefName": "main", "additions": 3, "deletions": 1,
              "reviewDecision": "APPROVED", "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN",
              "author": { "login": "rubeen", "avatarUrl": "a" },
              "autoMergeRequest": { "enabledAt": "2026-09-30T11:00:00Z", "mergeMethod": "SQUASH", "enabledBy": { "login": "rubeen" } },
              "labels": { "nodes": [{ "name": "bug", "color": "d73a4a" }] },
              "reviewRequests": { "nodes": [
                { "requestedReviewer": { "__typename": "User", "login": "x" } },
                { "requestedReviewer": { "__typename": "Team", "slug": "core" } }
              ] },
              "commits": { "nodes": [{ "commit": { "statusCheckRollup": null } }] }
            }
            """#
        let n = try gitHubDecoder().decode(RawOpen.self, from: Data(json.utf8))
        let pr = normalizeOpen(repo: "a/b", n)
        #expect(pr.reviewRequests == ["x", "@core"])
        #expect(pr.autoMerge?.method == .squash)
        #expect(pr.readyToMerge)
        #expect(pr.pipeline.state == .none)
    }
}

@Suite struct QueryTests {
    @Test func aliastReposUndEscapedNamen() {
        let q = Queries.dashboard(["a/b", "c/d"])
        #expect(q.contains(#"r0: repository(owner: "a", name: "b")"#))
        #expect(q.contains(#"r1: repository(owner: "c", name: "d")"#))
    }

    @Test func kodiertBranchNamenImPfad() {
        #expect(encodePath("feature/äb c") == "feature%2F%C3%A4b%20c")
        #expect(encodePath("release-1.0_x~") == "release-1.0_x~")
    }
}

@Suite struct FetchTests {
    @Test func liestDashboardMitRegelnUndFehlendemRepo() async throws {
        let rules = Data(#"[{ "type": "pull_request", "parameters": { "allowed_merge_methods": ["squash"] } }]"#.utf8)
        let transport = FakeTransport(
            graphQL: try fixture("dashboard"), rules: ["/repos/rubeen/pr-radar/rules/branches/main": (rules, 200)])
        let client = GitHubClient(transport: transport, token: { "t0k" })
        let fetched = try await client.fetch(repos: ["rubeen/pr-radar", "rubeen/geheim"])
        let s = fetched.snapshot

        #expect(s.viewer == "rubeen")
        #expect(fetched.rateLimit?.remaining == 4990)
        #expect(s.repos.map(\.fullName) == ["rubeen/pr-radar", "rubeen/geheim"])
        #expect(s.repos[0].mergeMethods == [.squash, .merge])
        #expect(s.repos[0].viewerCanMerge)
        #expect(s.repos[0].defaultBranchPipeline == .passed)
        #expect(s.repos[1].error?.contains("Could not resolve") == true)

        // neuester zuerst; Sekundenbruchteile werden gelesen
        #expect(s.open.map(\.id) == ["PR_new", "PR_old"])
        let pr = s.open[0]
        #expect(pr.mergeMethods == [.squash], "Ruleset erlaubt nur Squash")
        #expect(pr.pipeline.state == .passed)
        #expect(pr.pipeline.checks.map(\.group) == ["CI", nil])
        #expect(pr.labels == [PRLabel(name: "feature", color: "a2eeef")])
        #expect(pr.reviewRequests == ["@core"])
        #expect(pr.isBy("rubeen"))

        #expect(s.merged.map(\.id) == ["PR_m"], "ohne mergedAt verworfen")
        #expect(s.releases.map(\.name) == ["v1.0.0"], "Entwürfe verworfen, leerer Name → Tag")

        let auth = transport.requests.first?.value(forHTTPHeaderField: "Authorization")
        #expect(auth == "bearer t0k")
    }

    @Test func ohneRegelnGeltenRepoMethoden() async throws {
        let client = GitHubClient(transport: FakeTransport(graphQL: try fixture("dashboard")), token: { "t" })
        let s = try await client.fetch(repos: ["rubeen/pr-radar", "rubeen/geheim"]).snapshot
        #expect(s.open[0].mergeMethods == [.squash, .merge])
    }

    @Test func ohneTokenKlareFehlermeldung() async throws {
        let client = GitHubClient(transport: FakeTransport(graphQL: Data()), token: { "  " })
        await #expect(throws: GitHubError.noToken) {
            _ = try await client.fetch(repos: ["a/b"])
        }
    }

    @Test func liestRegelnTolerant() throws {
        let data = Data(#"""
            [{ "parameters": {} },
             { "type": "pull_request", "parameters": { "allowed_merge_methods": ["squash", null] } },
             { "type": "required_linear_history" }]
            """#.utf8)
        let rules = try #require(decodeRules(data))
        #expect(applyBranchRules([.squash, .merge, .rebase], rules) == [.squash])
        #expect(decodeRules(Data("{}".utf8)) == [], "kein Array → keine Regeln")
        #expect(decodeRules(Data("kaputt".utf8)) == nil)
    }

    @Test func rateLimitOhneLesbaresResetAt() throws {
        let json = #"{ "data": { "viewer": { "login": "x" }, "rateLimit": { "remaining": 7, "limit": 9, "resetAt": "morgen" } } }"#
        let res = try gitHubDecoder().decode(GQLResponse.self, from: Data(json.utf8))
        #expect(res.data?.rateLimit == RateLimit(remaining: 7, limit: 9, resetAt: nil))
    }

    @Test func regelnBleibenBeiAbruffehlerErhalten() async {
        let cache = RulesCache()
        let key = RulesCache.Key(repo: "a/b", branch: "main")
        let t0 = Date()
        let rules = [RawRule(type: "pull_request", parameters: .init(allowedMergeMethods: ["merge"]))]
        #expect(await cache.update(key, now: t0, fetched: rules) == rules)
        #expect(await cache.fresh(key, now: t0) == rules)
        // abgelaufen → neu abrufen; Abruf scheitert → alter Stand gilt weiter
        let later = t0 + rulesTTL + 1
        #expect(await cache.fresh(key, now: later) == nil)
        #expect(await cache.update(key, now: later, fetched: nil) == rules)
        // nie erfolgreich geladen → keine Regeln bekannt
        #expect(await cache.update(.init(repo: "x/y", branch: "main"), now: later, fetched: nil) == nil)
    }
}
