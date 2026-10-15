import Foundation
import Testing

@testable import PRRadarCore

@MainActor
private func makeRadar(_ github: FakeGitHub, clock: FakeClock = FakeClock()) async -> Radar {
    let radar = Radar(github: github, store: nil, cache: nil, timing: clock.timing, initialConfig: Config(repos: ["a/b"]))
    await radar.pollOnce()
    return radar
}

/// Wartet, bis `pending` leer ist (die Mutationen laufen in eigenen Tasks).
@MainActor
private func settled(_ radar: Radar) async {
    for _ in 0..<1000 where !radar.pending.isEmpty {
        await Task.yield()
        try? await Task.sleep(for: .milliseconds(1))
    }
    radar.stop()
}

@Suite struct MergeLogicTests {
    @Test func erkenntVoruebergehendeMergeFehler() {
        #expect(isTransientMergeError("Base branch was modified. Review and try the merge again."))
        #expect(isTransientMergeError("Pull Request is not mergeable"))
        #expect(!isTransientMergeError("Repository rule violations found: Changes must be made through a pull request."))
        #expect(!isTransientMergeError("viewer does not have permission to merge"))
    }

    @Test func naechsteRundeBrichtBeiNeuemCommitUndKonfliktAb() {
        let snap = Snapshot(open: [makePR("1", status: "BLOCKED")])
        #expect(nextRound(snap, prID: "1", head: "abc") == .attempt(autoMergeAllowed: false))
        if case .abort = nextRound(snap, prID: "1", head: "alt") {} else { Issue.record("erwartet Abbruch") }
        #expect(nextRound(snap, prID: "weg", head: "abc") == .done(.merged))

        let conflict = Snapshot(open: [makePR("1", status: "DIRTY")])
        if case .abort = nextRound(conflict, prID: "1", head: "abc") {} else { Issue.record("erwartet Abbruch") }

        let auto = Snapshot(open: [makePR("1", auto: AutoMerge(method: .merge))])
        #expect(nextRound(auto, prID: "1", head: "abc") == .done(.autoMerge(.merge)))
    }

    @Test func optimistischerAutoMergeUeberlebtVeralteteAntwort() {
        let t0 = Date()
        let on = AutoMerge(method: .merge)
        var optimistic = ["1": OptimisticEntry(at: t0, state: on)]

        // GitHub hängt hinterher → lokaler Zustand bleibt sichtbar
        var stale = Snapshot(open: [makePR("1", status: "BLOCKED")])
        reconcileOptimistic(&optimistic, &stale, now: t0)
        #expect(stale.open[0].autoMerge == on)
        #expect(optimistic["1"] != nil)

        // GitHub bestätigt → Eintrag erledigt
        var fresh = Snapshot(open: [makePR("1", auto: on)])
        reconcileOptimistic(&optimistic, &fresh, now: t0)
        #expect(optimistic.isEmpty)

        // Nach Ablauf gewinnt GitHub
        optimistic = ["1": OptimisticEntry(at: t0, state: on)]
        stale = Snapshot(open: [makePR("1", status: "BLOCKED")])
        reconcileOptimistic(&optimistic, &stale, now: t0 + optimisticTTL + 1)
        #expect(stale.open[0].autoMerge == nil)
    }
}

@MainActor
@Suite struct RadarTests {
    @Test func direktMergenBindetAnHeadUndLaedtNeu() async {
        let github = FakeGitHub(snapshot: Snapshot(repos: [repoInfo()], open: [makePR("1")]))
        let radar = await makeRadar(github)
        radar.merge("1", method: .merge)
        #expect(radar.isPending("1"))
        await settled(radar)
        await radar.pollOnce()
        let merges = await github.merges
        #expect(merges.count == 1)
        #expect(merges.first?.2 == "abc")
        #expect(radar.snapshot?.open.isEmpty == true)
        #expect(!radar.isPending("1"))
    }

    @Test func voruebergehendeAblehnungAktiviertAutoMerge() async {
        let github = FakeGitHub(
            snapshot: Snapshot(repos: [repoInfo()], open: [makePR("1")]),
            mergeErrors: ["Base branch was modified. Review and try the merge again."])
        let radar = await makeRadar(github)
        radar.merge("1", method: .merge)
        await settled(radar)
        let autoMerges = await github.autoMerges
        #expect(autoMerges.count == 1)
        #expect(autoMerges.first?.2 == "abc", "Auto-Merge nur auf dem gesehenen Commit")
        #expect(radar.snapshot?.openPR("1")?.autoMerge?.method == .merge, "sofort lokal sichtbar")
    }

    @Test func wiederholtBisGitHubMergtWennAutoMergeNichtGeht() async {
        let clock = FakeClock()
        let github = FakeGitHub(
            snapshot: Snapshot(repos: [repoInfo(autoMerge: false)], open: [makePR("1")]),
            mergeErrors: ["Pull Request is not mergeable", "Base branch was modified", nil])
        let radar = await makeRadar(github, clock: clock)
        radar.merge("1", method: .merge)
        await settled(radar)
        #expect(await github.merges.count == 3)
        #expect(clock.sleeps.prefix(2) == [2, 3], "Wartezeit wächst")
    }

    @Test func gibtNachDeadlineAuf() async {
        let clock = FakeClock()
        var events: [RadarEvent] = []
        let github = FakeGitHub(
            snapshot: Snapshot(repos: [repoInfo(autoMerge: false)], open: [makePR("1")]),
            mergeErrors: Array(repeating: "Pull Request is not mergeable", count: 100))
        let radar = await makeRadar(github, clock: clock)
        radar.onEvent = { events.append($0) }
        radar.merge("1", method: .merge)
        await settled(radar)
        guard case let .actionFailed(message)? = events.last else {
            Issue.record("erwartet Fehlermeldung, bekam \(events)")
            return
        }
        #expect(message.contains("seit 3 min"))
        #expect(await github.merges.count < 30)
    }

    @Test func dauerhafterFehlerWirdGemeldet() async {
        var events: [RadarEvent] = []
        let github = FakeGitHub(
            snapshot: Snapshot(repos: [repoInfo()], open: [makePR("1")]),
            mergeErrors: ["viewer does not have permission to merge"])
        let radar = await makeRadar(github)
        radar.onEvent = { events.append($0) }
        radar.merge("1", method: .merge)
        await settled(radar)
        #expect(events == [.actionFailed("viewer does not have permission to merge")])
        #expect(await github.autoMerges.isEmpty)
    }

    @Test func unerlaubteMethodeWirdAbgelehnt() async {
        var events: [RadarEvent] = []
        let github = FakeGitHub(snapshot: Snapshot(repos: [repoInfo()], open: [makePR("1", methods: [.squash])]))
        let radar = await makeRadar(github)
        radar.onEvent = { events.append($0) }
        radar.merge("1", method: .merge)
        #expect(events == [.actionFailed("Merge-Methode Merge ist in a/b nicht erlaubt.")])
        #expect(await github.merges.isEmpty)
    }

    @Test func alleMergenFasstZusammen() async {
        var events: [RadarEvent] = []
        let github = FakeGitHub(
            snapshot: Snapshot(repos: [repoInfo(), repoInfo("c/d")], open: [
                makePR("1"), makePR("2"), makePR("3", repo: "c/d"),
            ]),
            mergeErrors: [nil, "Base branch was modified", nil])
        let radar = await makeRadar(github)
        radar.onEvent = { events.append($0) }
        radar.mergeAll([("1", .merge), ("2", .merge), ("3", .merge)])
        for _ in 0..<1000 where !events.contains(where: { if case .mergeAllFinished = $0 { true } else { false } }) {
            try? await Task.sleep(for: .milliseconds(1))
        }
        radar.stop()
        let summary = events.compactMap { e -> (Int, Int, Int)? in
            if case let .mergeAllFinished(m, a, f) = e { return (m, a, f.count) }
            return nil
        }
        #expect(summary.count == 1)
        #expect(summary.first.map { $0.0 + $0.1 } == 3)
        #expect(summary.first?.2 == 0)
    }

    @Test func meldetPipelineWechsel() async {
        var events: [RadarEvent] = []
        let github = FakeGitHub(snapshot: Snapshot(repos: [repoInfo()], open: [makePR("1", state: .running)]))
        let radar = await makeRadar(github)
        radar.onEvent = { events.append($0) }
        #expect(radar.status.interval == activeInterval, "läuft → schneller pollen")
        await github.setSnapshot(Snapshot(repos: [repoInfo()], open: [makePR("1", state: .failed)]))
        await radar.pollOnce()
        guard case let .transitions(t)? = events.first else {
            Issue.record("keine Transition")
            return
        }
        #expect(t.map(\.kind) == [.failed])
        #expect(radar.flashes["1"] != nil)
        #expect(radar.status.interval == normalInterval)
    }

    @Test func reposVerwalten() throws {
        let radar = Radar(github: FakeGitHub(snapshot: Snapshot()), store: nil, cache: nil, timing: FakeClock().timing)
        try radar.addRepo("https://github.com/o/a")
        try radar.addRepo("o/b")
        #expect(throws: RadarError.duplicateRepo("O/A")) { try radar.addRepo("O/A") }
        #expect(throws: RadarError.invalidRepo) { try radar.addRepo("quatsch") }
        radar.updatePrefs { $0.repoFilter = ["o/a"] }
        radar.moveRepos(fromOffsets: [1], toOffset: 0)
        #expect(radar.repos == ["o/b", "o/a"])
        try radar.removeRepo("o/a")
        #expect(radar.repos == ["o/b"])
        #expect(radar.prefs.repoFilter.isEmpty)
        radar.stop()
    }
}
