import Foundation
import Testing

@testable import PRRadarCore

@Suite struct ModelTests {
    @Test func diffMeldetRotGruenUndMerge() {
        let prev = Snapshot(open: [makePR("1", state: .running), makePR("2", state: .running), makePR("3", state: .passed)])
        let next = Snapshot(
            open: [makePR("1", state: .failed), makePR("2", state: .passed)],
            merged: [MergedPR(id: "3", repo: "a/b", number: 3, title: "t", url: "u", mergedAt: Date())])
        let kinds = diffSnapshots(prev, next).map { [$0.pr.id, $0.kind.rawValue] }
        #expect(kinds == [["1", "failed"], ["2", "passed"], ["3", "merged"]])
    }

    @Test func konfliktAuchBeiDirty() {
        let p = makePR("1", status: "DIRTY", mergeable: .unknown)
        #expect(p.hasConflict)
        #expect(!p.readyToMerge)
    }

    @Test func geaenderteZeilen() {
        let prev = Snapshot(open: [makePR("1", state: .running), makePR("2")])
        var next = prev
        next.open[0].pipeline.state = .passed
        #expect(changedPRs(prev, next) == ["1"])
    }

    @Test func snapshotUeberlebtCache() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let cache = SnapshotCache(url: dir.appendingPathComponent("s.json"))
        let s = Snapshot(viewer: "me", repos: [repoInfo()], open: [makePR("1")])
        try cache.save(s)
        #expect(cache.load() == s)
    }
}

@Suite struct ConfigTests {
    @Test func parstRepoAngaben() {
        let cases: [(String, String?)] = [
            ("rubeen/pr-radar", "rubeen/pr-radar"),
            ("https://github.com/rubeen/pr-radar", "rubeen/pr-radar"),
            ("https://github.com/rubeen/pr-radar/pulls", "rubeen/pr-radar"),
            ("git@github.com:rubeen/pr-radar.git", "rubeen/pr-radar"),
            ("  rubeen/pr-radar \n", "rubeen/pr-radar"),
            ("kaputt", nil),
            ("a/b c", nil),
            ("ä/b", nil),
        ]
        for (input, out) in cases {
            #expect(parseRepo(input) == out, "\(input)")
        }
    }

    @Test func liestMacOSKonfigurationMitUnbekanntenFeldern() throws {
        let json = #"{ "repos": ["a/b"], "prefs": { "tab": "merged", "quick": "failed", "view": "flow", "grouped": false, "repo_filter": ["a/b"], "notify": true, "sidebar_hidden": true } }"#
        let cfg = try JSONDecoder().decode(Config.self, from: Data(json.utf8))
        #expect(cfg.repos == ["a/b"])
        #expect(cfg.prefs.tab == .merged)
        #expect(cfg.prefs.quick == .failed)
        #expect(cfg.prefs.view == .flow)
        #expect(!cfg.prefs.grouped)
        #expect(cfg.prefs.repoFilter == ["a/b"])
        #expect(cfg.prefs.notify)
    }

    @Test func fehlendeFelderSindStandard() throws {
        let cfg = try JSONDecoder().decode(Config.self, from: Data(#"{ "prefs": { "quick": "gibtsnicht" } }"#.utf8))
        #expect(cfg == Config())
    }

    @Test func speichertUndLaedt() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + "/config.json")
        let store = ConfigStore(url: url)
        var cfg = Config(repos: ["x/y"])
        cfg.prefs.notify = true
        try store.save(cfg)
        #expect(store.load() == cfg)
    }
}

@Suite struct TimeTests {
    @Test func relativeAngaben() {
        let now = Date()
        #expect(GermanTime.ago(nil) == "–")
        #expect(GermanTime.ago(now, now: now) == "gerade eben")
        #expect(GermanTime.ago(now - 5 * 60, now: now) == "vor 5 Min.")
        #expect(GermanTime.ago(now - 3 * 3600, now: now) == "vor 3 Std.")
        #expect(GermanTime.ago(now - 30 * 3600, now: now) == "gestern")
        #expect(GermanTime.ago(now - 4 * 86_400, now: now) == "vor 4 Tagen")
        #expect(GermanTime.ago(now - 8 * 86_400, now: now) == "vor 1 Woche")
        #expect(GermanTime.ago(now - 400 * 86_400, now: now) == "vor 1 Jahr")
    }

    @Test func tageslabel() {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "Europe/Berlin")!
        let now = cal.date(from: DateComponents(year: 2026, month: 10, day: 15, hour: 12))!
        #expect(GermanTime.dayLabel(now - 3600, now: now, calendar: cal) == "Heute")
        #expect(GermanTime.dayLabel(now - 86_400, now: now, calendar: cal) == "Gestern")
        #expect(GermanTime.dayLabel(now - 2 * 86_400, now: now, calendar: cal) == "Dienstag, 13. Oktober")
        #expect(GermanTime.clock(now, calendar: cal) == "12:00")
        #expect(GermanTime.full(now, calendar: cal) == "15.10.2026, 12:00")
    }
}
