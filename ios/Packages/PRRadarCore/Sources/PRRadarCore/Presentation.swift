// Ableitungen für die Oberfläche: Filter/Suche, Gruppen, Merge-Button-Zustand und Flow-Stufen.
// Bewusst ohne SwiftUI, damit alles testbar bleibt.

import Foundation

// MARK: - Filter & Suche

/// Was die Listen nach Repo-Filter, Suche und Schnellfilter zeigen.
public struct Derived: Sendable {
    public let snapshot: Snapshot
    public let prefs: Prefs
    /// Offene PRs nach Repo-Filter und Suche (Basis der Zähler)
    public let openBase: [OpenPR]
    /// … zusätzlich nach Schnellfilter
    public let openList: [OpenPR]
    public let merged: [MergedPR]
    public let releases: [Release]

    public init(snapshot: Snapshot, prefs: Prefs, query: String) {
        self.snapshot = snapshot
        self.prefs = prefs
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines)
        let repoOK = { (repo: String) in prefs.repoFilter.isEmpty || prefs.repoFilter.contains(repo) }
        // Groß-/Kleinschreibung und Akzente egal: „cafe“ findet „Café“
        let textOK = { (parts: [String]) in
            q.isEmpty || parts.contains { $0.range(of: q, options: [.caseInsensitive, .diacriticInsensitive]) != nil }
        }

        openBase = snapshot.open.filter { p in
            repoOK(p.repo) && textOK([p.title, p.repo, p.author?.login ?? "", p.headRef, "#\(p.number)"])
        }
        let viewer = snapshot.viewer
        openList = openBase.filter { prefs.quick.matches($0, viewer: viewer) }
        merged = snapshot.merged.filter { m in repoOK(m.repo) && textOK([m.title, m.repo, "#\(m.number)"]) }
        releases = snapshot.releases.filter { r in repoOK(r.repo) && textOK([r.name, r.tagName, r.repo]) }
    }

    public func count(_ quick: Quick) -> Int {
        openBase.filter { quick.matches($0, viewer: snapshot.viewer) }.count
    }

    /// Offene PRs je Repo, in der Reihenfolge des ersten Auftretens.
    public var openGroups: [(repo: String, prs: [OpenPR])] {
        group(openList, by: \.repo)
    }

    /// Gemergte PRs je Tag (neueste zuerst).
    public func mergedByDay(now: Date = Date(), calendar: Calendar = .current) -> [(day: String, prs: [MergedPR])] {
        var out: [(day: String, prs: [MergedPR])] = []
        for m in merged {
            let label = GermanTime.dayLabel(m.mergedAt, now: now, calendar: calendar)
            if out.last?.day == label {
                out[out.count - 1].prs.append(m)
            } else {
                out.append((label, [m]))
            }
        }
        return out
    }

    /// Release-IDs, die standardmäßig aufgeklappt sind: das neueste je Repo.
    public var releasesOpenByDefault: Set<String> {
        var seen = Set<String>()
        var out = Set<String>()
        for r in releases where seen.insert(r.repo).inserted {
            out.insert(r.id)
        }
        return out
    }
}

private func group<T, K: Equatable>(_ items: [T], by key: (T) -> K) -> [(K, [T])] {
    var out: [(K, [T])] = []
    for item in items {
        let k = key(item)
        if let i = out.firstIndex(where: { $0.0 == k }) {
            out[i].1.append(item)
        } else {
            out.append((k, [item]))
        }
    }
    return out
}

// MARK: - Merge-Button

/// Was der Merge-Button gerade tun kann – eine Quelle für Label, Zustand und Erklärung.
public enum MergeAction: Sendable, Equatable {
    /// Auto-Merge ist an; Tippen schaltet es aus.
    case disable(method: MergeMethod, by: String?)
    /// PR ist sofort mergebar.
    case mergeNow([MergeMethod])
    /// Auto-Merge kann aktiviert werden.
    case enable([MergeMethod])
    /// Nicht möglich – mit Grund.
    case unavailable(String)

    public static func of(_ pr: OpenPR, in snapshot: Snapshot) -> MergeAction {
        if let am = pr.autoMerge {
            return .disable(method: am.method, by: am.enabledBy)
        }
        guard let repo = snapshot.repo(pr.repo) else {
            return .unavailable("Repository nicht geladen")
        }
        let methods = pr.mergeMethods
        if methods.isEmpty {
            return .unavailable("Keine Merge-Methode erlaubt (Repo-Einstellungen und Rulesets)")
        } else if !repo.viewerCanMerge {
            return .unavailable("Keine Schreibrechte")
        } else if pr.isDraft {
            return .unavailable("Draft-PRs können nicht gemergt werden")
        } else if pr.readyToMerge {
            return .mergeNow(methods)
        } else if !repo.autoMergeAllowed {
            return .unavailable("Auto-Merge ist in diesem Repo nicht erlaubt (Settings → Allow auto-merge)")
        } else {
            return .enable(methods)
        }
    }
}

/// Kandidaten für „Alle mergen“: sichtbare, sofort mergebare PRs, die nicht schon laufen.
public struct MergeAllPlan: Sendable, Equatable {
    public var candidates: [(id: String, methods: [MergeMethod])]
    /// Alle angebotenen Methoden (Vereinigung, Reihenfolge des ersten Auftretens)
    public var offered: [MergeMethod]
    /// Wahl nur nötig, wenn ein PR mehrere Methoden erlaubt
    public var needsChoice: Bool

    public var count: Int { candidates.count }

    public init?(prs: [OpenPR], snapshot: Snapshot, isPending: (String) -> Bool) {
        candidates = prs.filter { !isPending($0.id) }.compactMap { pr in
            if case let .mergeNow(methods) = MergeAction.of(pr, in: snapshot) { return (pr.id, methods) }
            return nil
        }
        guard !candidates.isEmpty else { return nil }
        var offered: [MergeMethod] = []
        for (_, methods) in candidates {
            for m in methods where !offered.contains(m) { offered.append(m) }
        }
        self.offered = offered
        needsChoice = candidates.contains { $0.methods.count > 1 }
    }

    /// Je PR die bevorzugte Methode, sonst die erste erlaubte.
    public func items(preferred: MergeMethod?) -> [(String, MergeMethod)] {
        candidates.map { c in
            (c.id, preferred.flatMap { c.methods.contains($0) ? $0 : nil } ?? c.methods[0])
        }
    }

    public static func == (a: MergeAllPlan, b: MergeAllPlan) -> Bool {
        a.candidates.map(\.id) == b.candidates.map(\.id) && a.offered == b.offered
    }
}

// MARK: - Flow

public enum Tone: Sendable, Equatable {
    case pass, fail, run, accent, idle

    public init(_ state: PipelineState) {
        switch state {
        case .passed: self = .pass
        case .failed: self = .fail
        case .running: self = .run
        case .none: self = .idle
        }
    }
}

public struct CheckNode: Sendable, Equatable, Identifiable {
    public var id: String
    public var label: String
    public var sub: String
    public var state: PipelineState
    public var url: String?
}

public struct Stage: Sendable, Equatable {
    public var tone: Tone
    /// SF Symbol
    public var symbol: String
    public var title: String
    public var sub: String
}

/// Höchstens so viele Check-Knoten je PR; der Rest wird zusammengefasst.
public let maxCheckNodes = 4

func worst(_ states: [PipelineState]) -> PipelineState {
    states.min { $0.rank < $1.rank } ?? .none
}

/// Checks nach Workflow bündeln – ein Knoten pro Workflow, Rotes zuerst, Rest zusammengefasst.
public func checkNodes(_ pr: OpenPR) -> [CheckNode] {
    let groups = group(pr.pipeline.checks) { $0.group ?? $0.name }
    if groups.isEmpty {
        return [CheckNode(id: "none", label: "Keine Checks", sub: "letzter Commit", state: .none, url: nil)]
    }
    var nodes: [CheckNode] = groups.map { label, checks in
        let state = worst(checks.map(\.state.pipeline))
        let passed = checks.filter { $0.state.pipeline == .passed }.count
        let sub =
            if checks.count > 1 { "\(passed)/\(checks.count) grün" }
            else if checks[0].group != nil { checks[0].name }
            else { state.label }
        let url = (checks.first { $0.state.pipeline == state } ?? checks[0]).url
        return CheckNode(id: label, label: label, sub: sub, state: state, url: url)
    }
    // stabil nach Rang sortieren
    nodes = nodes.enumerated().sorted { a, b in
        a.element.state.rank != b.element.state.rank ? a.element.state.rank < b.element.state.rank : a.offset < b.offset
    }.map(\.element)
    guard nodes.count > maxCheckNodes else { return nodes }
    let rest = nodes[(maxCheckNodes - 1)...]
    let state = worst(rest.map(\.state))
    nodes = Array(nodes.prefix(maxCheckNodes - 1))
    nodes.append(CheckNode(id: "more", label: "+\(rest.count) weitere", sub: state.label, state: state, url: pr.url))
    return nodes
}

public func reviewStage(_ pr: OpenPR, viewer: String?) -> Stage {
    if pr.requestsReview(from: viewer) && pr.reviewDecision != .approved {
        return Stage(tone: .accent, symbol: "person.badge.clock", title: "Dein Review", sub: "angefragt")
    }
    switch pr.reviewDecision {
    case .approved:
        return Stage(tone: .pass, symbol: "checkmark.shield", title: "Approved", sub: "Review ok")
    case .changesRequested:
        return Stage(tone: .fail, symbol: "exclamationmark.bubble", title: "Änderungen", sub: "angefordert")
    case .reviewRequired:
        let who = pr.reviewRequests.isEmpty ? "ausstehend" : pr.reviewRequests.joined(separator: ", ")
        return Stage(tone: .idle, symbol: "eye", title: "Review offen", sub: who)
    case nil:
        return Stage(tone: .idle, symbol: "eye", title: "Review", sub: "nicht erforderlich")
    }
}

public func mergeStage(_ pr: OpenPR) -> Stage {
    if pr.hasConflict {
        return Stage(tone: .fail, symbol: "exclamationmark.triangle", title: "Konflikt", sub: "nicht mergebar")
    } else if pr.isDraft {
        return Stage(tone: .idle, symbol: "pencil.circle", title: "Draft", sub: "noch nicht bereit")
    } else if let am = pr.autoMerge {
        return Stage(tone: .accent, symbol: "bolt.fill", title: "Auto-Merge", sub: "\(am.method.label) · wenn grün")
    } else if pr.readyToMerge {
        return Stage(tone: .pass, symbol: "arrow.triangle.merge", title: "Bereit", sub: "kann gemergt werden")
    } else if pr.mergeStateStatus == "BEHIND" {
        return Stage(tone: .run, symbol: "arrow.triangle.merge", title: "Veraltet", sub: "Base ist weiter")
    } else if pr.mergeStateStatus == "BLOCKED" {
        return Stage(tone: .idle, symbol: "arrow.triangle.merge", title: "Blockiert", sub: "wartet auf Regeln")
    } else {
        return Stage(tone: .idle, symbol: "arrow.triangle.merge", title: "Merge", sub: pr.mergeStateStatus.lowercased())
    }
}

/// Zustand des Ziel-Branches – nur bekannt, wenn er der Default-Branch des Repos ist.
public func baseState(_ pr: OpenPR, in snapshot: Snapshot) -> PipelineState {
    guard let repo = snapshot.repo(pr.repo), repo.defaultBranch == pr.baseRef else { return .none }
    return repo.defaultBranchPipeline
}
