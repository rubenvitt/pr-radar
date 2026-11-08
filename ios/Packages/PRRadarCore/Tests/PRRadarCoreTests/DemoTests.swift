import Foundation
import Testing

@testable import PRRadarCore

/// Uhr, die man von Hand vorstellt.
private final class ManualClock: @unchecked Sendable {
    private let lock = NSLock()
    private var current = Date(timeIntervalSince1970: 1_800_000_000)

    var now: @Sendable () -> Date { { self.lock.withLock { self.current } } }

    func advance(_ seconds: TimeInterval) {
        lock.withLock { current += seconds }
    }
}

@Suite struct DemoTests {
    @Test func beispieldatenZeigenAlleWichtigenZustaende() {
        let s = DemoData.snapshot()
        let states = Set(s.open.map(\.pipeline.state))
        #expect(states.isSuperset(of: [.running, .failed, .passed, .none]))
        #expect(s.open.contains { $0.hasConflict })
        #expect(s.open.contains { $0.isDraft })
        #expect(s.open.contains { $0.autoMerge != nil })
        #expect(s.open.contains { $0.readyToMerge })
        #expect(s.open.contains { $0.requestsReview(from: s.viewer) })
        #expect(s.open.contains { $0.isBy(s.viewer) })
        #expect(!s.merged.isEmpty)
        #expect(!s.releases.isEmpty)
        // Jeder PR gehört zu einem Demo-Repo und hat eine erlaubte Merge-Methode des Repos.
        for pr in s.open {
            let repo = try! #require(s.repo(pr.repo))
            #expect(!pr.mergeMethods.isEmpty)
            #expect(Set(pr.mergeMethods).isSubset(of: repo.mergeMethods))
        }
        #expect(Set(s.open.map(\.id)).count == s.open.count)
        #expect(Set(s.repos.map(\.fullName)) == Set(DemoData.repos))
    }

    @Test func linksZeigenNichtAufGitHub() {
        let s = DemoData.snapshot()
        let urls = s.open.map(\.url) + s.merged.map(\.url) + s.releases.map(\.url) + s.repos.map(\.url)
            + s.open.flatMap { $0.pipeline.checks.compactMap(\.url) }
        #expect(urls.allSatisfy { $0.hasPrefix("https://example.com/") })
    }

    @Test func laufendePipelineWirdNachEinerWeileGruen() async throws {
        let clock = ManualClock()
        let github = DemoGitHub(now: clock.now)
        let first = try await github.fetch(repos: DemoData.repos).snapshot
        #expect(first.openPR("demo-web-128")?.pipeline.state == .running)
        #expect(first.openPR("demo-web-128")?.readyToMerge == false)

        clock.advance(25)
        let later = try await github.fetch(repos: DemoData.repos).snapshot
        let pr = try #require(later.openPR("demo-web-128"))
        #expect(pr.pipeline.state == .passed)
        #expect(pr.readyToMerge)
        #expect(diffSnapshots(first, later).contains { $0.kind == .passed && $0.pr.id == "demo-web-128" })
    }

    @Test func mergenVerschiebtDenPRNachGemergt() async throws {
        let github = DemoGitHub()
        try await github.merge(prID: "demo-web-127", method: .squash, head: "d3m0127")
        let s = try await github.fetch(repos: DemoData.repos).snapshot
        #expect(s.openPR("demo-web-127") == nil)
        #expect(s.merged.first?.id == "demo-web-127")
        #expect(s.merged.first?.mergedBy == DemoData.viewer)
    }

    @Test func mergenLehntAbWieGitHub() async throws {
        let github = DemoGitHub()
        // Alter Commit
        await #expect(throws: GitHubError.self) {
            try await github.merge(prID: "demo-web-127", method: .squash, head: "alt")
        }
        // Noch nicht mergebar (Pipeline läuft)
        await #expect(throws: GitHubError.self) {
            try await github.merge(prID: "demo-web-128", method: .squash, head: "d3m0128")
        }
        let s = try await github.fetch(repos: DemoData.repos).snapshot
        #expect(s.openPR("demo-web-127") != nil)
        #expect(s.openPR("demo-web-128") != nil)
    }

    @Test func autoMergeGreiftSobaldDiePipelineGruenIst() async throws {
        let clock = ManualClock()
        let github = DemoGitHub(now: clock.now)
        try await github.setAutoMerge(prID: "demo-web-128", method: .squash, head: "d3m0128")
        var s = try await github.fetch(repos: DemoData.repos).snapshot
        #expect(s.openPR("demo-web-128")?.autoMerge?.method == .squash)

        clock.advance(25)
        s = try await github.fetch(repos: DemoData.repos).snapshot
        #expect(s.openPR("demo-web-128") == nil)
        #expect(s.merged.first?.id == "demo-web-128")
    }

    @Test func entfernteReposVerschwinden() async throws {
        let github = DemoGitHub()
        let s = try await github.fetch(repos: ["acme/web", "other/repo"]).snapshot
        #expect(s.repos.map(\.fullName) == ["acme/web", "other/repo"])
        #expect(s.repo("other/repo")?.error != nil)
        #expect(s.open.allSatisfy { $0.repo == "acme/web" })
        #expect(s.merged.allSatisfy { $0.repo == "acme/web" })
    }

    @MainActor
    @Test func demoRadarLaedtOhneTokenUndSpeichertNichts() async {
        var prefs = Prefs()
        prefs.view = .flow
        let radar = Radar.demo(prefs: prefs)
        #expect(radar.repos == DemoData.repos)
        #expect(radar.prefs.view == .flow)
        await radar.pollOnce()
        radar.stop()
        #expect(radar.status.error == nil)
        #expect(radar.snapshot?.viewer == DemoData.viewer)
        #expect(radar.snapshot?.open.isEmpty == false)
    }
}
