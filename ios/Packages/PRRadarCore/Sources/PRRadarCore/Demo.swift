// Demo-Modus: erfundene Repos und PRs, damit man die App ohne Token und Repos ausprobieren kann
// (App-Review, neue Nutzer, Screenshots). Nichts davon spricht GitHub an, nichts wird gespeichert.
//
// Die Attrappe spielt ein paar Abläufe nach: Laufende Pipelines werden nach einer Weile grün,
// Mergen und Auto-Merge wirken auf den Stand, und ein PR mit Auto-Merge wird gemergt, sobald
// er grün und freigegeben ist.

import Foundation

public enum DemoData {
    /// Angezeigter Login – „Dein Review“ und „Von dir“ beziehen sich darauf.
    public static let viewer = "alex"

    public static let repos = ["acme/web", "acme/api", "acme/mobile"]

    /// Links zeigen auf eine reservierte Beispiel-Domain, nicht auf echte GitHub-Konten.
    static func url(_ path: String) -> String {
        "https://example.com/\(path)"
    }

    /// Ein Stand, der möglichst viele Zustände der App zeigt: läuft, rot, grün, Konflikt,
    /// Draft, Auto-Merge, angefragtes Review, verschiedene Merge-Methoden.
    public static func snapshot(now: Date = Date()) -> Snapshot {
        let ago = { (minutes: Double) in now.addingTimeInterval(-minutes * 60) }
        let person = { (login: String) in Person(login: login) }

        let repos = [
            RepoInfo(
                fullName: "acme/web", url: url("acme/web"), defaultBranch: "main", defaultBranchPipeline: .passed,
                autoMergeAllowed: true, mergeMethods: [.squash, .merge], viewerCanMerge: true),
            RepoInfo(
                fullName: "acme/api", url: url("acme/api"), defaultBranch: "main", defaultBranchPipeline: .passed,
                autoMergeAllowed: true, mergeMethods: [.squash], viewerCanMerge: true),
            RepoInfo(
                fullName: "acme/mobile", url: url("acme/mobile"), defaultBranch: "develop",
                defaultBranchPipeline: .failed, autoMergeAllowed: false, mergeMethods: [.merge, .rebase],
                viewerCanMerge: true),
        ]

        let open = [
            OpenPR(
                id: "demo-web-128", repo: "acme/web", number: 128, title: "Add dark mode toggle to settings",
                url: url("acme/web/pull/128"), author: person(viewer), updatedAt: ago(4),
                headRef: "feat/dark-mode-toggle", headOID: "d3m0128", additions: 214, deletions: 37,
                reviewDecision: .approved, mergeable: .mergeable, mergeStateStatus: "BLOCKED",
                labels: [PRLabel(name: "enhancement", color: "a2eeef")],
                pipeline: pipeline([
                    Check(name: "build", state: .passed, url: url("acme/web/actions/1"), group: "CI"),
                    Check(name: "lint", state: .passed, url: url("acme/web/actions/2"), group: "CI"),
                    Check(name: "e2e", state: .running, url: url("acme/web/actions/3"), group: "CI"),
                ]),
                mergeMethods: [.squash, .merge]),
            OpenPR(
                id: "demo-web-127", repo: "acme/web", number: 127, title: "Fix layout shift on dashboard cards",
                url: url("acme/web/pull/127"), author: person("sam"), updatedAt: ago(35),
                headRef: "fix/card-layout-shift", headOID: "d3m0127", additions: 18, deletions: 9,
                reviewDecision: .approved, mergeable: .mergeable, mergeStateStatus: "CLEAN",
                labels: [PRLabel(name: "bug", color: "d73a4a")],
                pipeline: pipeline([
                    Check(name: "build", state: .passed, url: url("acme/web/actions/4"), group: "CI"),
                    Check(name: "lint", state: .passed, url: url("acme/web/actions/5"), group: "CI"),
                    Check(name: "e2e", state: .passed, url: url("acme/web/actions/6"), group: "CI"),
                    Check(name: "preview", state: .passed, url: url("acme/web/deployments/1"), group: "Deploy"),
                ]),
                mergeMethods: [.squash, .merge]),
            OpenPR(
                id: "demo-web-125", repo: "acme/web", number: 125, title: "Bump vite from 5.4.2 to 5.4.8",
                url: url("acme/web/pull/125"), author: person("dependabot[bot]"), updatedAt: ago(90),
                headRef: "dependabot/npm_and_yarn/vite-5.4.8", headOID: "d3m0125", additions: 41, deletions: 41,
                reviewDecision: .reviewRequired, mergeable: .mergeable, mergeStateStatus: "BLOCKED",
                autoMerge: AutoMerge(method: .squash, enabledBy: "sam"),
                labels: [PRLabel(name: "dependencies", color: "0366d6")], reviewRequests: [viewer],
                pipeline: pipeline([
                    Check(name: "build", state: .passed, url: url("acme/web/actions/7"), group: "CI"),
                    Check(name: "lint", state: .passed, url: url("acme/web/actions/8"), group: "CI"),
                    Check(name: "e2e", state: .passed, url: url("acme/web/actions/9"), group: "CI"),
                ]),
                mergeMethods: [.squash, .merge]),
            OpenPR(
                id: "demo-web-119", repo: "acme/web", number: 119, title: "New onboarding flow",
                url: url("acme/web/pull/119"), author: person("jordan"), isDraft: true, updatedAt: ago(60 * 26),
                headRef: "feat/onboarding-v2", headOID: "d3m0119", additions: 640, deletions: 212,
                mergeable: .mergeable, mergeStateStatus: "DRAFT",
                labels: [PRLabel(name: "wip", color: "fbca04")],
                mergeMethods: [.squash, .merge]),
            OpenPR(
                id: "demo-api-57", repo: "acme/api", number: 57, title: "Rate-limit public endpoints",
                url: url("acme/api/pull/57"), author: person("priya"), updatedAt: ago(12),
                headRef: "feat/rate-limit", headOID: "d3m0057", additions: 156, deletions: 12,
                reviewDecision: .changesRequested, mergeable: .mergeable, mergeStateStatus: "BLOCKED",
                labels: [PRLabel(name: "backend", color: "5319e7")],
                pipeline: pipeline([
                    Check(name: "unit", state: .failed, url: url("acme/api/actions/1"), group: "Tests"),
                    Check(name: "integration", state: .passed, url: url("acme/api/actions/2"), group: "Tests"),
                    Check(name: "CodeQL", state: .passed, url: url("acme/api/security/1"), group: "Security"),
                ]),
                mergeMethods: [.squash]),
            OpenPR(
                id: "demo-api-55", repo: "acme/api", number: 55, title: "Migrate sessions table to UUID keys",
                url: url("acme/api/pull/55"), author: person(viewer), updatedAt: ago(60 * 5),
                headRef: "chore/session-uuid", headOID: "d3m0055", additions: 88, deletions: 31,
                reviewDecision: .approved, mergeable: .conflicting, mergeStateStatus: "DIRTY",
                labels: [PRLabel(name: "database", color: "c5def5")],
                pipeline: pipeline([
                    Check(name: "unit", state: .passed, url: url("acme/api/actions/3"), group: "Tests"),
                    Check(name: "integration", state: .passed, url: url("acme/api/actions/4"), group: "Tests"),
                ]),
                mergeMethods: [.squash]),
            OpenPR(
                id: "demo-api-54", repo: "acme/api", number: 54, title: "Add OpenAPI docs for /v2/orders",
                url: url("acme/api/pull/54"), author: person("sam"), updatedAt: ago(2),
                headRef: "docs/openapi-orders", headOID: "d3m0054", additions: 302, deletions: 4,
                reviewDecision: .reviewRequired, mergeable: .mergeable, mergeStateStatus: "BLOCKED",
                reviewRequests: [viewer, "@backend"],
                pipeline: pipeline([
                    Check(name: "unit", state: .passed, url: url("acme/api/actions/5"), group: "Tests"),
                    Check(name: "integration", state: .running, url: url("acme/api/actions/6"), group: "Tests"),
                    Check(name: "docs-preview", state: .running, url: url("acme/api/actions/7"), group: "Docs"),
                ]),
                mergeMethods: [.squash]),
            OpenPR(
                id: "demo-mobile-312", repo: "acme/mobile", number: 312, title: "Offline cache for the feed",
                url: url("acme/mobile/pull/312"), author: person("jordan"), updatedAt: ago(50),
                headRef: "feat/offline-feed", headOID: "d3m0312", baseRef: "develop", additions: 420, deletions: 96,
                reviewDecision: .approved, mergeable: .mergeable, mergeStateStatus: "CLEAN",
                labels: [PRLabel(name: "feature", color: "0e8a16")],
                pipeline: pipeline([
                    Check(name: "iOS", state: .passed, url: url("acme/mobile/actions/1"), group: "Build"),
                    Check(name: "Android", state: .passed, url: url("acme/mobile/actions/2"), group: "Build"),
                    Check(name: "snapshot-tests", state: .passed, url: url("acme/mobile/actions/3"), group: "Tests"),
                ]),
                mergeMethods: [.merge, .rebase]),
            OpenPR(
                id: "demo-mobile-309", repo: "acme/mobile", number: 309, title: "Crash on launch with empty locale",
                url: url("acme/mobile/pull/309"), author: person("priya"), updatedAt: ago(20),
                headRef: "fix/empty-locale-crash", headOID: "d3m0309", baseRef: "develop", additions: 23,
                deletions: 5, reviewDecision: .reviewRequired, mergeable: .mergeable, mergeStateStatus: "BLOCKED",
                labels: [PRLabel(name: "bug", color: "d73a4a"), PRLabel(name: "crash", color: "b60205")],
                reviewRequests: [viewer],
                pipeline: pipeline([
                    Check(name: "iOS", state: .failed, url: url("acme/mobile/actions/4"), group: "Build"),
                    Check(name: "Android", state: .passed, url: url("acme/mobile/actions/5"), group: "Build"),
                    Check(name: "snapshot-tests", state: .skipped, group: "Tests"),
                ]),
                mergeMethods: [.merge, .rebase]),
        ]

        let merged = [
            MergedPR(
                id: "demo-web-126", repo: "acme/web", number: 126, title: "Cache avatar images", url: url("acme/web/pull/126"),
                author: person("sam"), mergedAt: ago(25), mergedBy: viewer),
            MergedPR(
                id: "demo-api-53", repo: "acme/api", number: 53, title: "Return 422 for invalid payloads",
                url: url("acme/api/pull/53"), author: person("priya"), mergedAt: ago(140), mergedBy: "priya"),
            MergedPR(
                id: "demo-mobile-308", repo: "acme/mobile", number: 308, title: "Pull to refresh on profile",
                url: url("acme/mobile/pull/308"), author: person("jordan"), mergedAt: ago(60 * 20), mergedBy: "jordan",
                baseRef: "develop"),
            MergedPR(
                id: "demo-web-124", repo: "acme/web", number: 124, title: "Update privacy page copy",
                url: url("acme/web/pull/124"), author: person(viewer), mergedAt: ago(60 * 26), mergedBy: viewer),
            MergedPR(
                id: "demo-api-52", repo: "acme/api", number: 52, title: "Bump postgres driver to 3.2", url: url("acme/api/pull/52"),
                author: person("dependabot[bot]"), mergedAt: ago(60 * 50), mergedBy: "sam"),
            MergedPR(
                id: "demo-web-121", repo: "acme/web", number: 121, title: "Keyboard shortcuts for search",
                url: url("acme/web/pull/121"), author: person("jordan"), mergedAt: ago(60 * 75), mergedBy: viewer),
        ]

        let releases = [
            Release(
                id: "demo-rel-web-240", repo: "acme/web", name: "v2.4.0", tagName: "v2.4.0",
                url: url("acme/web/releases/v2.4.0"), publishedAt: ago(60 * 30), isLatest: true, author: viewer,
                descriptionHTML: """
                    <h2>What's new</h2><ul><li>Keyboard shortcuts for search (#121)</li>\
                    <li>Updated privacy page (#124)</li></ul><h2>Fixes</h2><ul><li>Avatar images load faster (#126)</li></ul>
                    """),
            Release(
                id: "demo-rel-web-231", repo: "acme/web", name: "v2.3.1", tagName: "v2.3.1",
                url: url("acme/web/releases/v2.3.1"), publishedAt: ago(60 * 24 * 9), author: "sam",
                descriptionHTML: "<ul><li>Fix login redirect loop</li></ul>"),
            Release(
                id: "demo-rel-api-1120", repo: "acme/api", name: "v1.12.0", tagName: "v1.12.0",
                url: url("acme/api/releases/v1.12.0"), publishedAt: ago(60 * 48), isLatest: true, author: "priya",
                descriptionHTML: "<ul><li>422 for invalid payloads (#53)</li><li>Postgres driver 3.2 (#52)</li></ul>"),
            Release(
                id: "demo-rel-mobile-300b2", repo: "acme/mobile", name: "3.0.0 Beta 2", tagName: "3.0.0-beta.2",
                url: url("acme/mobile/releases/3.0.0-beta.2"), publishedAt: ago(60 * 19), isPrerelease: true,
                author: "jordan", descriptionHTML: "<p>Pull to refresh on the profile screen (#308).</p>"),
        ]

        return Snapshot(viewer: viewer, repos: repos, open: open, merged: merged, releases: releases)
    }

    /// Zählt die Checks wie die Normalisierung echter GitHub-Daten (rot vor läuft vor grün).
    static func pipeline(_ checks: [Check]) -> Pipeline {
        let sorted = checks.sorted { a, b in
            a.state.rank != b.state.rank ? a.state.rank < b.state.rank : a.name < b.name
        }
        let count = { (s: CheckState) in checks.filter { $0.state == s }.count }
        let failed = count(.failed)
        let running = count(.running)
        let passed = count(.passed)
        let state: PipelineState =
            if failed > 0 { .failed } else if running > 0 { .running } else if !checks.isEmpty { .passed } else { .none }
        return Pipeline(
            state: state, total: checks.count, passed: passed, failed: failed, running: running, checks: sorted)
    }

    /// Nach so vielen Sekunden im Demo-Modus werden die laufenden Pipelines dieser PRs grün.
    static let finishes: [String: TimeInterval] = ["demo-web-128": 20, "demo-api-54": 60]
}

/// GitHub-Attrappe für den Demo-Modus. Antwortet sofort, ohne Netz.
public actor DemoGitHub: GitHubAPI {
    private var snapshot: Snapshot
    private let started: Date
    private let now: @Sendable () -> Date

    public init(now: @escaping @Sendable () -> Date = { Date() }) {
        self.now = now
        started = now()
        snapshot = DemoData.snapshot(now: started)
    }

    public func fetch(repos: [String]) async throws -> Fetched {
        advance()
        var s = snapshot
        // Entfernte Repos verschwinden wie bei GitHub; hinzugefügte gibt es in der Demo nicht.
        s.repos = repos.map { full in
            s.repo(full) ?? .unavailable(full, error: L10n.tr("Demo mode only includes the sample repos."))
        }
        s.open.removeAll { !repos.contains($0.repo) }
        s.merged.removeAll { !repos.contains($0.repo) }
        s.releases.removeAll { !repos.contains($0.repo) }
        return Fetched(snapshot: s, rateLimit: RateLimit(remaining: 4_986, limit: 5_000))
    }

    public func setAutoMerge(prID: String, method: MergeMethod?, head: String?) async throws {
        let i = try index(prID, head: head)
        snapshot.open[i].autoMerge = method.map { AutoMerge(method: $0, enabledBy: DemoData.viewer) }
    }

    public func merge(prID: String, method: MergeMethod, head: String) async throws {
        let i = try index(prID, head: head)
        let pr = snapshot.open[i]
        guard pr.readyToMerge else { throw GitHubError.graphQL("Pull Request is not mergeable") }
        mergeNow(at: i, by: DemoData.viewer)
    }

    private func index(_ prID: String, head: String?) throws -> Int {
        guard let i = snapshot.open.firstIndex(where: { $0.id == prID }) else {
            throw GitHubError.graphQL("Could not resolve to a PullRequest with the id of '\(prID)'.")
        }
        if let head, snapshot.open[i].headOID != head {
            throw GitHubError.graphQL("Head branch was modified. Review and try the merge again.")
        }
        return i
    }

    private func mergeNow(at i: Int, by login: String) {
        let pr = snapshot.open.remove(at: i)
        snapshot.merged.insert(
            MergedPR(
                id: pr.id, repo: pr.repo, number: pr.number, title: pr.title, url: pr.url, author: pr.author,
                mergedAt: now(), mergedBy: login, baseRef: pr.baseRef),
            at: 0)
    }

    /// Spielt die Zeit seit dem Start nach: Pipelines werden grün, Auto-Merge greift.
    private func advance() {
        let elapsed = now().timeIntervalSince(started)
        for (id, after) in DemoData.finishes where elapsed >= after {
            guard let i = snapshot.open.firstIndex(where: { $0.id == id }),
                snapshot.open[i].pipeline.state == .running
            else { continue }
            var checks = snapshot.open[i].pipeline.checks
            for c in checks.indices where checks[c].state == .running { checks[c].state = .passed }
            snapshot.open[i].pipeline = DemoData.pipeline(checks)
            snapshot.open[i].updatedAt = now()
            if snapshot.open[i].reviewDecision == .approved, !snapshot.open[i].hasConflict {
                snapshot.open[i].mergeStateStatus = "CLEAN"
            }
        }
        // GitHub mergt PRs mit Auto-Merge selbst, sobald sie grün und freigegeben sind.
        while let i = snapshot.open.firstIndex(where: { pr in
            pr.autoMerge != nil && pr.pipeline.state == .passed && pr.reviewDecision == .approved && !pr.hasConflict
        }) {
            mergeNow(at: i, by: snapshot.open[i].autoMerge?.enabledBy ?? DemoData.viewer)
        }
    }
}

extension Radar {
    /// Radar mit Beispieldaten: ohne Token, ohne Netz, ohne Konfiguration oder Cache auf der Platte.
    /// `prefs` übernimmt Ansichts-Einstellungen (Liste/Flow, gruppiert) aus dem echten Radar.
    public static func demo(prefs: Prefs = Prefs(), timing: Timing = .live) -> Radar {
        Radar(
            github: DemoGitHub(now: timing.now), store: nil, cache: nil, timing: timing,
            initialConfig: Config(repos: DemoData.repos, prefs: prefs))
    }
}
