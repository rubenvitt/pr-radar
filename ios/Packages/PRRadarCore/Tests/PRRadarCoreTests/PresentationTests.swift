import Foundation
import Testing

@testable import PRRadarCore

@Suite struct PresentationTests {
    @Test func filterSucheUndZaehler() {
        var a = makePR("1", state: .failed, repo: "o/a")
        a.title = "Login reparieren"
        var b = makePR("2", state: .passed, repo: "o/b")
        b.author = Person(login: "me")
        let s = Snapshot(viewer: "me", open: [a, b])
        var prefs = Prefs()

        var d = Derived(snapshot: s, prefs: prefs, query: "")
        #expect(d.count(.failed) == 1)
        #expect(d.count(.mine) == 1)
        #expect(d.openGroups.map(\.repo) == ["o/a", "o/b"])

        d = Derived(snapshot: s, prefs: prefs, query: " LOGIN ")
        #expect(d.openList.map(\.id) == ["1"])
        d = Derived(snapshot: s, prefs: prefs, query: "login réparieren")
        #expect(d.openList.map(\.id) == ["1"], "Akzente egal")
        d = Derived(snapshot: s, prefs: prefs, query: "#2")
        #expect(d.openList.map(\.id) == ["2"])

        prefs.repoFilter = ["o/b"]
        prefs.quick = .failed
        d = Derived(snapshot: s, prefs: prefs, query: "")
        #expect(d.openBase.map(\.id) == ["2"])
        #expect(d.openList.isEmpty)
    }

    @Test func mergeButtonZustaende() {
        let s = Snapshot(repos: [repoInfo(autoMerge: false)])
        #expect(MergeAction.of(makePR("1"), in: s) == .mergeNow([.merge]))
        #expect(MergeAction.of(makePR("1", status: "BLOCKED"), in: s) == .unavailable("Auto-Merge ist in diesem Repo nicht erlaubt (Settings → Allow auto-merge)"))
        #expect(MergeAction.of(makePR("1", status: "BLOCKED"), in: Snapshot(repos: [repoInfo()])) == .enable([.merge]))
        #expect(MergeAction.of(makePR("1", methods: []), in: s) == .unavailable("Keine Merge-Methode erlaubt (Repo-Einstellungen und Rulesets)"))
        #expect(MergeAction.of(makePR("1"), in: Snapshot(repos: [repoInfo(canMerge: false)])) == .unavailable("Keine Schreibrechte"))
        #expect(MergeAction.of(makePR("1", auto: AutoMerge(method: .squash, enabledBy: "x")), in: s) == .disable(method: .squash, by: "x"))
        #expect(MergeAction.of(makePR("1"), in: Snapshot()) == .unavailable("Repository nicht geladen"))
    }

    @Test func alleMergenNurBereite() throws {
        let s = Snapshot(repos: [repoInfo()], open: [
            makePR("1", methods: [.merge]),
            makePR("2", methods: [.squash, .merge]),
            makePR("3", status: "BLOCKED"),
            makePR("4"),
        ])
        let plan = try #require(MergeAllPlan(prs: s.open, snapshot: s, isPending: { $0 == "4" }))
        #expect(plan.count == 2)
        #expect(plan.offered == [.merge, .squash])
        #expect(plan.needsChoice)
        #expect(plan.items(preferred: .squash).map(\.1) == [.merge, .squash])
        #expect(plan.items(preferred: nil).map(\.1) == [.merge, .squash])
        #expect(MergeAllPlan(prs: [makePR("3", status: "BLOCKED")], snapshot: s, isPending: { _ in false }) == nil)
    }

    @Test func checkKnotenJeWorkflowRotesZuerstRestZusammengefasst() {
        var pr = makePR("1")
        pr.pipeline.checks = [
            Check(name: "lint", state: .passed, group: "CI"),
            Check(name: "test", state: .failed, url: "f", group: "CI"),
            Check(name: "a", state: .passed),
            Check(name: "b", state: .running),
            Check(name: "c", state: .passed),
            Check(name: "d", state: .skipped),
        ]
        let nodes = checkNodes(pr)
        #expect(nodes.map(\.label) == ["CI", "b", "a", "+2 weitere"])
        #expect(nodes[0].sub == "1/2 grün")
        #expect(nodes[0].url == "f")
        #expect(nodes[3].state == .passed)
        #expect(checkNodes(makePR("2")).map(\.label) == ["Keine Checks"])
    }

    @Test func flowStufen() {
        var pr = makePR("1", status: "BEHIND", mergeable: .mergeable)
        #expect(mergeStage(pr).title == "Veraltet")
        pr.reviewRequests = ["me"]
        pr.reviewDecision = .reviewRequired
        #expect(reviewStage(pr, viewer: "me").title == "Dein Review")
        #expect(reviewStage(pr, viewer: "other").sub == "me")
        pr.autoMerge = AutoMerge(method: .squash)
        #expect(mergeStage(pr).sub == "Squash · wenn grün")
    }

    @Test func standardmaessigOffeneReleases() {
        let s = Snapshot(releases: [
            Release(id: "1", repo: "a", name: "v2", tagName: "v2", url: "u"),
            Release(id: "2", repo: "a", name: "v1", tagName: "v1", url: "u"),
            Release(id: "3", repo: "b", name: "v1", tagName: "v1", url: "u"),
        ])
        #expect(Derived(snapshot: s, prefs: Prefs(), query: "").releasesOpenByDefault == ["1", "3"])
    }
}
