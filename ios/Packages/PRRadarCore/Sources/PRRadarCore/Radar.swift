// `Radar` besitzt Konfiguration, letzten Snapshot und Poll-Status, pollt GitHub adaptiv
// und führt Merges/Auto-Merge aus – inklusive Wiederholung bei vorübergehenden Ablehnungen.

import Foundation
import Observation

/// Normales Poll-Intervall
public let normalInterval: TimeInterval = 30
/// Intervall, solange irgendwo eine Pipeline läuft
public let activeInterval: TimeInterval = 10

/// So lange wird ein Merge bei vorübergehenden GitHub-Fehlern wiederholt.
let mergeDeadline: TimeInterval = 180
/// Wartezeit vor dem ersten erneuten Versuch; wächst bis `mergeRetryMax`.
let mergeRetryStart: TimeInterval = 2
let mergeRetryMax: TimeInterval = 10
/// Wie lange ein lokal angenommener Auto-Merge-Zustand gegen ältere GitHub-Antworten gewinnt
let optimisticTTL: TimeInterval = 30
/// Ein zwischengespeicherter Stand dient höchstens so lange als Vergleichsbasis für
/// Benachrichtigungen – sonst meldet der erste Abruf nach einer Nacht alles auf einmal.
let cacheBaselineMaxAge: TimeInterval = 2 * 3600
/// So lange bleibt eine geänderte Zeile hervorgehoben.
public let flashDuration: TimeInterval = 1.6

public enum PollState: Sendable, Equatable {
    case idle, polling, error
}

public struct Status: Sendable, Equatable {
    public var state: PollState = .idle
    public var lastSuccess: Date?
    public var interval: TimeInterval = normalInterval
    public var error: String?
    public var rateLimit: RateLimit?
}

public enum RadarEvent: Sendable, Equatable {
    /// Pipeline rot/grün oder PR gemergt – für Benachrichtigungen.
    case transitions([PRTransition])
    /// Eine Nutzeraktion (Merge, Auto-Merge) ist fehlgeschlagen.
    case actionFailed(String)
    /// „Alle mergen“ ist durch.
    case mergeAllFinished(merged: Int, autoMerge: Int, failed: [String])
}

public enum RadarError: LocalizedError, Equatable {
    case invalidRepo
    case duplicateRepo(String)
    case aborted(String)
    case mergeTimedOut(String)
    case notLoaded
    case notOpen
    case methodNotAllowed(MergeMethod, String)

    public var errorDescription: String? {
        switch self {
        case .invalidRepo: "Ungültiges Repo – erwartet owner/name oder eine GitHub-URL."
        case let .duplicateRepo(full): "\(full) ist schon in der Liste."
        case let .aborted(reason): reason
        case let .mergeTimedOut(detail):
            "GitHub lässt den Merge seit \(Int(mergeDeadline / 60)) min nicht zu: \(detail)"
        case .notLoaded: "Noch keine Daten geladen."
        case .notOpen: "Pull Request nicht mehr offen."
        case let .methodNotAllowed(m, repo): "Merge-Methode \(m.label) ist in \(repo) nicht erlaubt."
        }
    }
}

/// Uhr und Warten – austauschbar, damit Wiederholungen in Tests nicht wirklich warten.
public struct Timing: Sendable {
    public var now: @Sendable () -> Date
    public var sleep: @Sendable (TimeInterval) async throws -> Void

    public init(
        now: @escaping @Sendable () -> Date, sleep: @escaping @Sendable (TimeInterval) async throws -> Void
    ) {
        self.now = now
        self.sleep = sleep
    }

    public static let live = Timing(now: { Date() }, sleep: { try await Task.sleep(for: .seconds($0)) })
}

/// Letzter Snapshot auf der Platte – die App zeigt beim Start sofort den alten Stand.
public struct SnapshotCache: Sendable {
    public let url: URL

    public init(url: URL) {
        self.url = url
    }

    public static var standard: SnapshotCache {
        let base =
            FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSTemporaryDirectory())
        return SnapshotCache(url: base.appendingPathComponent("pr-radar/snapshot.json"))
    }

    public func load() -> Snapshot? {
        loadWithDate()?.snapshot
    }

    /// Stand samt Zeitpunkt, zu dem er gespeichert wurde.
    public func loadWithDate() -> (snapshot: Snapshot, savedAt: Date)? {
        guard let data = try? Data(contentsOf: url),
            let snapshot = try? JSONDecoder().decode(Snapshot.self, from: data)
        else { return nil }
        let attrs = try? FileManager.default.attributesOfItem(atPath: url.path)
        return (snapshot, attrs?[.modificationDate] as? Date ?? .distantPast)
    }

    public func save(_ snapshot: Snapshot) throws {
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try JSONEncoder().encode(snapshot).write(to: url, options: .atomic)
    }
}

/// Wie ein Merge-Auftrag ausgegangen ist.
enum MergeOutcome: Sendable, Equatable {
    case merged
    /// GitHub ließ den Merge noch nicht zu, Auto-Merge übernimmt.
    case autoMerge(MergeMethod)

    var settle: Settle {
        switch self {
        case .merged: .awaitRefresh
        case let .autoMerge(m): .autoMerge(m)
        }
    }
}

/// Was nach einer erfolgreichen Mutation passiert, bis GitHub den neuen Stand liefert.
enum Settle: Sendable, Equatable {
    /// Auto-Merge an/aus: sofort lokal übernehmen.
    case autoMerge(MergeMethod?)
    /// Direkt gemergt: Spinner bis zum nächsten Poll (dann verschwindet der PR).
    case awaitRefresh
}

enum SettleResult: Sendable {
    case ok(Settle)
    /// `nil`: schon anderweitig gemeldet
    case failed(String?)
}

@MainActor
@Observable
public final class Radar {
    public private(set) var config: Config
    public private(set) var snapshot: Snapshot?
    public private(set) var status = Status()
    /// PR-IDs, für die gerade eine Mutation läuft
    public private(set) var pending: Set<String> = []
    /// PRs, deren Merge GitHub gerade noch ablehnt und der gleich erneut versucht wird
    public private(set) var retrying: Set<String> = []
    /// Geänderte PRs (ID → Epoche) – die Oberfläche hebt sie kurz hervor.
    public private(set) var flashes: [String: Int] = [:]

    /// Gemergte PRs, die bis zum nächsten Poll noch als „läuft“ gelten
    private var awaitingRefresh: Set<String> = []
    /// Lokal vorweggenommener Auto-Merge-Zustand je PR, bis GitHub ihn bestätigt
    private var optimistic: [String: OptimisticEntry] = [:]
    private var flashEpoch = 0
    /// Gesetzt, solange `snapshot` aus dem Cache stammt: wann er gespeichert wurde
    private var cachedAt: Date?

    @ObservationIgnored public var onEvent: (@MainActor (RadarEvent) -> Void)?
    @ObservationIgnored private let github: GitHubAPI
    @ObservationIgnored private let store: ConfigStore?
    @ObservationIgnored private let cache: SnapshotCache?
    @ObservationIgnored private let timing: Timing
    @ObservationIgnored private var pollTask: Task<Void, Never>?

    public init(
        github: GitHubAPI, store: ConfigStore? = .standard, cache: SnapshotCache? = .standard,
        timing: Timing = .live, initialConfig: Config? = nil
    ) {
        self.github = github
        self.store = store
        self.cache = cache
        self.timing = timing
        config = initialConfig ?? store?.load() ?? Config()
        if let cached = cache?.loadWithDate() {
            snapshot = cached.snapshot
            cachedAt = cached.savedAt
        }
    }

    public var prefs: Prefs { config.prefs }
    public var repos: [String] { config.repos }

    public func isPending(_ id: String) -> Bool {
        pending.contains(id) || awaitingRefresh.contains(id)
    }

    public func isRetrying(_ id: String) -> Bool {
        retrying.contains(id)
    }

    public func updatePrefs(_ f: (inout Prefs) -> Void) {
        var prefs = config.prefs
        f(&prefs)
        guard prefs != config.prefs else { return }
        config.prefs = prefs
        // Einstellungen sind Komfort – ein Schreibfehler soll die Bedienung nicht stören.
        try? store?.save(config)
    }

    // MARK: Polling

    /// Sofort neu laden und den Poll-Zyklus neu starten.
    public func refresh() {
        startLoop(after: 0)
    }

    /// Polling anhalten (App im Hintergrund).
    public func stop() {
        pollTask?.cancel()
        pollTask = nil
        if status.state == .polling { status.state = .idle }
    }

    /// Für Pull-to-Refresh: lädt sofort und kehrt erst zurück, wenn die Daten da sind.
    public func refreshNow() async {
        pollTask?.cancel()
        let wait = await pollOnce()
        startLoop(after: wait)
    }

    /// Ein einzelner Abruf ohne Poll-Zyklus – für Hintergrund-Aktualisierung.
    @discardableResult
    public func pollOnce() async -> TimeInterval {
        status.state = .polling
        let repos = config.repos
        let result: Result<Fetched, Error>
        do {
            result = .success(try await github.fetch(repos: repos))
        } catch {
            result = .failure(error)
        }
        // Abgelöst (neuer Refresh, Repos geändert): Ergebnis verwerfen.
        if Task.isCancelled { return 0 }
        return apply(result)
    }

    private func startLoop(after initialDelay: TimeInterval) {
        pollTask?.cancel()
        pollTask = Task { [weak self, timing] in
            var wait = initialDelay
            while !Task.isCancelled {
                if wait > 0 {
                    do { try await timing.sleep(wait) } catch { return }
                }
                guard let self, !Task.isCancelled else { return }
                wait = await self.pollOnce()
            }
        }
    }

    /// Übernimmt ein Poll-Ergebnis und liefert die Wartezeit bis zum nächsten Poll.
    func apply(_ result: Result<Fetched, Error>) -> TimeInterval {
        switch result {
        case let .success(fetched):
            var next = fetched.snapshot
            let baselineFresh = cachedAt.map { timing.now().timeIntervalSince($0) <= cacheBaselineMaxAge } ?? true
            cachedAt = nil
            if let prev = snapshot, baselineFresh {
                let transitions = diffSnapshots(prev, next)
                if !transitions.isEmpty { onEvent?(.transitions(transitions)) }
                flash(changedPRs(prev, next))
            }
            reconcileOptimistic(&optimistic, &next, now: timing.now())
            snapshot = next
            awaitingRefresh.removeAll()
            status.rateLimit = fetched.rateLimit ?? status.rateLimit
            status.state = .idle
            status.error = nil
            status.lastSuccess = timing.now()
            if let cache {
                Task.detached(priority: .utility) { try? cache.save(next) }
            }
        case let .failure(error):
            status.state = .error
            status.error = error.localizedDescription
        }
        status.interval = (snapshot?.anyRunning ?? false) ? activeInterval : normalInterval
        return status.state == .error ? max(status.interval, normalInterval) : status.interval
    }

    private func flash(_ ids: [String]) {
        guard !ids.isEmpty else { return }
        flashEpoch += 1
        let epoch = flashEpoch
        for id in ids { flashes[id] = epoch }
        Task { [weak self, timing] in
            try? await timing.sleep(flashDuration + 0.2)
            self?.flashes = self?.flashes.filter { $0.value != epoch } ?? [:]
        }
    }

    // MARK: Repos

    public func addRepo(_ input: String) throws {
        guard let full = parseRepo(input) else { throw RadarError.invalidRepo }
        if config.repos.contains(where: { $0.caseInsensitiveCompare(full) == .orderedSame }) {
            throw RadarError.duplicateRepo(full)
        }
        config.repos.append(full)
        try store?.save(config)
        refresh()
    }

    public func removeRepo(_ full: String) throws {
        config.repos.removeAll { $0 == full }
        config.prefs.repoFilter.removeAll { $0 == full }
        try store?.save(config)
        refresh()
    }

    public func moveRepos(fromOffsets source: IndexSet, toOffset destination: Int) {
        var repos = config.repos
        let moving = source.sorted().map { repos[$0] }
        for i in source.sorted(by: >) { repos.remove(at: i) }
        let target = destination - source.filter { $0 < destination }.count
        repos.insert(contentsOf: moving, at: max(0, min(target, repos.count)))
        config.repos = repos
        try? store?.save(config)
    }

    // MARK: Merge & Auto-Merge

    /// Auto-Merge an (`method`) oder aus (`nil`).
    public func setAutoMerge(_ prID: String, method: MergeMethod?) {
        if let method, let error = checkMethod(prID, method) {
            onEvent?(.actionFailed(error.localizedDescription))
            return
        }
        guard pending.insert(prID).inserted else { return }  // läuft schon – keine Doppel-Submission
        Task { [github] in
            let result: SettleResult
            do {
                try await github.setAutoMerge(prID: prID, method: method, head: nil)
                result = .ok(.autoMerge(method))
            } catch {
                result = .failed(error.localizedDescription)
            }
            settle(prID, result)
        }
    }

    /// Jetzt mergen. Lehnt GitHub vorübergehend ab (z. B. weil gerade ein anderer PR in den
    /// Ziel-Branch gemergt wurde), wird erneut versucht bzw. Auto-Merge aktiviert.
    public func merge(_ prID: String, method: MergeMethod) {
        if let error = checkMethod(prID, method) {
            onEvent?(.actionFailed(error.localizedDescription))
            return
        }
        guard let head = snapshot?.openPR(prID)?.headOID else { return }
        guard pending.insert(prID).inserted else { return }
        Task {
            let result = await mergeUntilDone(prID, method: method, head: head)
            switch result {
            case let .success(outcome): settle(prID, .ok(outcome.settle))
            case let .failure(error): settle(prID, .failed(error.localizedDescription))
            }
        }
    }

    /// Mergt mehrere PRs: je Repo und Ziel-Branch nacheinander (parallele Merges in denselben
    /// Branch lehnt GitHub ab), verschiedene Ziele parallel. Ein Fehlschlag stoppt die anderen nicht.
    public func mergeAll(_ items: [(String, MergeMethod)]) {
        guard let snapshot else { return }
        var groups: [(key: String, jobs: [MergeJob])] = []
        for (id, method) in items {
            guard let pr = snapshot.openPR(id), checkMethod(id, method) == nil, pending.insert(id).inserted else {
                continue
            }
            let key = "\(pr.repo)\u{0}\(pr.baseRef)"
            let job = MergeJob(id: id, label: "\(pr.repo)#\(pr.number)", method: method, head: pr.headOID)
            if let i = groups.firstIndex(where: { $0.key == key }) {
                groups[i].jobs.append(job)
            } else {
                groups.append((key, [job]))
            }
        }
        guard !groups.isEmpty else { return }

        let jobLists = groups.map(\.jobs)
        Task {
            // Ergebnisse in Gruppenreihenfolge, egal welche Gruppe zuerst fertig ist
            var parts = [[JobResult]](repeating: [], count: jobLists.count)
            await withTaskGroup(of: (Int, [JobResult]).self) { group in
                for (i, jobs) in jobLists.enumerated() {
                    group.addTask { (i, await self.runSequentially(jobs)) }
                }
                for await (i, part) in group { parts[i] = part }
            }
            let results = parts.flatMap { $0 }
            onEvent?(
                .mergeAllFinished(
                    merged: results.filter { $0.outcome == .merged }.count,
                    autoMerge: results.filter { if case .autoMerge = $0.outcome { true } else { false } }.count,
                    failed: results.compactMap(\.failure)))
        }
    }

    private func runSequentially(_ jobs: [MergeJob]) async -> [JobResult] {
        var out: [JobResult] = []
        for job in jobs {
            switch await mergeUntilDone(job.id, method: job.method, head: job.head) {
            case let .success(outcome):
                settle(job.id, .ok(outcome.settle))
                out.append(JobResult(outcome: outcome, failure: nil))
            case let .failure(error):
                settle(job.id, .failed(nil))
                out.append(JobResult(outcome: nil, failure: "\(job.label): \(error.localizedDescription)"))
            }
        }
        return out
    }

    /// Mergt einen PR und wiederholt bei vorübergehenden Ablehnungen bis `mergeDeadline`.
    /// Zwischendurch wird Auto-Merge versucht – GitHub nimmt ihn nur an, solange der PR nicht
    /// direkt mergebar ist, und mergt dann selbst, sobald es geht. Beides nur auf dem Commit
    /// `head`, der beim Tippen zu sehen war.
    func mergeUntilDone(_ prID: String, method: MergeMethod, head: String) async -> Result<MergeOutcome, Error> {
        let started = timing.now()
        var delay = mergeRetryStart
        while true {
            let round = snapshot.map { nextRound($0, prID: prID, head: head) } ?? .done(.merged)
            let autoMergeAllowed: Bool
            switch round {
            case let .attempt(allowed): autoMergeAllowed = allowed
            case let .done(outcome): return .success(outcome)
            case let .abort(reason): return .failure(RadarError.aborted(reason))
            }

            let message: String
            do {
                try await github.merge(prID: prID, method: method, head: head)
                return .success(.merged)
            } catch {
                message = error.localizedDescription
                if !isTransientMergeError(message) { return .failure(error) }
            }

            if autoMergeAllowed {
                do {
                    try await github.setAutoMerge(prID: prID, method: method, head: head)
                    return .success(.autoMerge(method))
                } catch {
                    // Auto-Merge geht (noch) nicht – weiter mit erneutem Merge-Versuch.
                }
            }

            if timing.now().timeIntervalSince(started) + delay > mergeDeadline {
                return .failure(RadarError.mergeTimedOut(message))
            }
            retrying.insert(prID)
            try? await timing.sleep(delay)
            delay = min(delay * 1.5, mergeRetryMax)
        }
    }

    /// Nur Methoden zulassen, die Repo und Rulesets für diesen PR erlauben.
    private func checkMethod(_ prID: String, _ method: MergeMethod) -> RadarError? {
        guard let snapshot else { return .notLoaded }
        guard let pr = snapshot.openPR(prID) else { return .notOpen }
        return pr.mergeMethods.contains(method) ? nil : .methodNotAllowed(method, pr.repo)
    }

    /// Schließt eine Mutation ab: Spinner lösen, Ergebnis melden, neu laden.
    func settle(_ prID: String, _ result: SettleResult) {
        pending.remove(prID)
        retrying.remove(prID)
        switch result {
        case let .failed(message?): onEvent?(.actionFailed(message))
        case .failed(nil): break
        case let .ok(.autoMerge(method)): applyAutoMerge(prID, method: method)
        case .ok(.awaitRefresh): awaitingRefresh.insert(prID)
        }
        refresh()
    }

    /// Zeigt einen erfolgreich geschalteten Auto-Merge sofort an, statt auf den nächsten Poll zu warten.
    private func applyAutoMerge(_ prID: String, method: MergeMethod?) {
        guard var s = snapshot, let i = s.open.firstIndex(where: { $0.id == prID }) else { return }
        let state = method.map { AutoMerge(method: $0, enabledBy: s.viewer) }
        s.open[i].autoMerge = state
        snapshot = s
        optimistic[prID] = OptimisticEntry(at: timing.now(), state: state)
        flash([prID])
    }
}

struct MergeJob: Sendable {
    var id: String
    var label: String
    var method: MergeMethod
    var head: String
}

struct JobResult: Sendable {
    var outcome: MergeOutcome?
    var failure: String?
}

struct OptimisticEntry: Sendable, Equatable {
    var at: Date
    var state: AutoMerge?
}

/// Was vor einem (erneuten) Merge-Versuch laut aktuellem Snapshot gilt.
enum Round: Equatable {
    /// Versuchen; Wert: Auto-Merge als Ausweg erlaubt
    case attempt(autoMergeAllowed: Bool)
    /// Nichts mehr zu tun
    case done(MergeOutcome)
    /// Abbrechen
    case abort(String)
}

func nextRound(_ snapshot: Snapshot, prID: String, head: String) -> Round {
    // PR inzwischen weg (gemergt/geschlossen)? Dann ist nichts mehr zu tun.
    guard let pr = snapshot.openPR(prID) else { return .done(.merged) }
    if pr.headOID != head {
        return .abort("Seit dem Tippen wurden neue Commits gepusht – Merge abgebrochen.")
    }
    if let am = pr.autoMerge { return .done(.autoMerge(am.method)) }
    if pr.hasConflict { return .abort("Merge-Konflikt – Branch muss erst aktualisiert werden.") }
    return .attempt(autoMergeAllowed: snapshot.repo(pr.repo)?.autoMergeAllowed ?? false)
}

/// Ablehnungen, die sich von selbst erledigen – etwa direkt nachdem ein anderer PR in denselben
/// Branch gemergt wurde und GitHub die Mergebarkeit neu berechnet.
func isTransientMergeError(_ message: String) -> Bool {
    let m = message.lowercased()
    return ["base branch was modified", "try the merge again", "not mergeable", "merge already in progress"]
        .contains { m.contains($0) }
}

/// GitHub liefert einen neuen Auto-Merge-Zustand manchmal erst verzögert. Bis er ankommt
/// (max. `optimisticTTL`), gilt der lokal angenommene, damit der Button nicht zurückspringt.
func reconcileOptimistic(_ optimistic: inout [String: OptimisticEntry], _ snapshot: inout Snapshot, now: Date) {
    optimistic = optimistic.filter { id, entry in
        guard let i = snapshot.open.firstIndex(where: { $0.id == id }) else { return false }
        if (snapshot.open[i].autoMerge != nil) == (entry.state != nil) || now.timeIntervalSince(entry.at) > optimisticTTL {
            return false
        }
        snapshot.open[i].autoMerge = entry.state
        return true
    }
}
