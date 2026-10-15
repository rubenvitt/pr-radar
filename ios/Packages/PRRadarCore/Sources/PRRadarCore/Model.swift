// Domänenmodell: der normalisierte Stand aller beobachteten Repos.

import Foundation

public enum PipelineState: String, Sendable, Codable, Hashable, CaseIterable {
    case running, failed, passed, none

    public var label: String {
        switch self {
        case .running: "läuft"
        case .failed: "rot"
        case .passed: "grün"
        case .none: "ohne Ergebnis"
        }
    }

    /// Sortierreihenfolge: Rotes und Laufendes zuerst.
    public var rank: Int {
        switch self {
        case .failed: 0
        case .running: 1
        case .passed: 2
        case .none: 3
        }
    }
}

public enum CheckState: String, Sendable, Codable, Hashable {
    case running, failed, passed, skipped, neutral

    public var rank: Int {
        switch self {
        case .failed: 0
        case .running: 1
        case .passed: 2
        case .neutral: 3
        case .skipped: 4
        }
    }

    public var pipeline: PipelineState {
        switch self {
        case .failed: .failed
        case .running: .running
        case .passed: .passed
        case .skipped, .neutral: .none
        }
    }
}

public enum MergeMethod: String, Sendable, Codable, Hashable, CaseIterable, Identifiable {
    case squash = "SQUASH"
    case merge = "MERGE"
    case rebase = "REBASE"

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .squash: "Squash"
        case .merge: "Merge"
        case .rebase: "Rebase"
        }
    }

    public var graphQL: String { rawValue }

    public init?(graphQL: String) {
        self.init(rawValue: graphQL)
    }
}

public enum ReviewDecision: String, Sendable, Codable, Hashable {
    case approved, changesRequested, reviewRequired
}

/// Spiegelt GitHubs `MergeableState`.
public enum Mergeable: String, Sendable, Codable, Hashable {
    case mergeable, conflicting, unknown
}

public struct Check: Sendable, Codable, Hashable {
    public var name: String
    public var state: CheckState
    public var url: String?
    /// Workflow-Name bei GitHub Actions
    public var group: String?

    public init(name: String, state: CheckState, url: String? = nil, group: String? = nil) {
        self.name = name
        self.state = state
        self.url = url
        self.group = group
    }
}

public struct Pipeline: Sendable, Codable, Hashable {
    public var state: PipelineState
    public var total: Int
    public var passed: Int
    public var failed: Int
    public var running: Int
    public var checks: [Check]

    public init(
        state: PipelineState, total: Int = 0, passed: Int = 0, failed: Int = 0, running: Int = 0,
        checks: [Check] = []
    ) {
        self.state = state
        self.total = total
        self.passed = passed
        self.failed = failed
        self.running = running
        self.checks = checks
    }
}

public struct Person: Sendable, Codable, Hashable {
    public var login: String
    public var avatarURL: String

    public init(login: String, avatarURL: String = "") {
        self.login = login
        self.avatarURL = avatarURL
    }
}

public struct PRLabel: Sendable, Codable, Hashable {
    public var name: String
    /// Hex ohne `#`, so wie GitHub ihn liefert
    public var color: String

    public init(name: String, color: String) {
        self.name = name
        self.color = color
    }
}

public struct AutoMerge: Sendable, Codable, Hashable {
    public var method: MergeMethod
    public var enabledBy: String?

    public init(method: MergeMethod, enabledBy: String? = nil) {
        self.method = method
        self.enabledBy = enabledBy
    }
}

/// GitHub-Zustände, in denen ein PR sofort gemergt werden kann (Auto-Merge lehnt GitHub dann ab).
private let directMergeStates: Set<String> = ["CLEAN", "HAS_HOOKS", "UNSTABLE"]

public struct OpenPR: Sendable, Codable, Hashable, Identifiable {
    public var id: String
    public var repo: String
    public var number: Int
    public var title: String
    public var url: String
    public var author: Person?
    public var isDraft: Bool
    public var updatedAt: Date
    public var headRef: String
    /// Commit, auf dem der Branch gerade steht – Merges werden daran gebunden
    public var headOID: String
    public var baseRef: String
    public var additions: Int
    public var deletions: Int
    public var reviewDecision: ReviewDecision?
    public var mergeable: Mergeable
    public var mergeStateStatus: String
    public var autoMerge: AutoMerge?
    public var labels: [PRLabel]
    public var reviewRequests: [String]
    public var pipeline: Pipeline
    /// Effektiv erlaubte Merge-Methoden: Repo-Einstellungen ∩ Rulesets des Ziel-Branches
    public var mergeMethods: [MergeMethod]

    public init(
        id: String, repo: String, number: Int, title: String, url: String, author: Person? = nil,
        isDraft: Bool = false, updatedAt: Date = Date(), headRef: String = "", headOID: String = "",
        baseRef: String = "main", additions: Int = 0, deletions: Int = 0,
        reviewDecision: ReviewDecision? = nil, mergeable: Mergeable = .unknown,
        mergeStateStatus: String = "UNKNOWN", autoMerge: AutoMerge? = nil, labels: [PRLabel] = [],
        reviewRequests: [String] = [], pipeline: Pipeline = Pipeline(state: .none),
        mergeMethods: [MergeMethod] = []
    ) {
        self.id = id
        self.repo = repo
        self.number = number
        self.title = title
        self.url = url
        self.author = author
        self.isDraft = isDraft
        self.updatedAt = updatedAt
        self.headRef = headRef
        self.headOID = headOID
        self.baseRef = baseRef
        self.additions = additions
        self.deletions = deletions
        self.reviewDecision = reviewDecision
        self.mergeable = mergeable
        self.mergeStateStatus = mergeStateStatus
        self.autoMerge = autoMerge
        self.labels = labels
        self.reviewRequests = reviewRequests
        self.pipeline = pipeline
        self.mergeMethods = mergeMethods
    }

    /// Merge-Konflikt – `DIRTY` greift auch, solange GitHub `mergeable` noch berechnet.
    public var hasConflict: Bool {
        mergeable == .conflicting || mergeStateStatus == "DIRTY"
    }

    public var readyToMerge: Bool {
        mergeable == .mergeable && directMergeStates.contains(mergeStateStatus)
    }

    public func isBy(_ login: String?) -> Bool {
        guard let login, let author else { return false }
        return author.login == login
    }

    public func requestsReview(from login: String?) -> Bool {
        guard let login else { return false }
        return reviewRequests.contains(login)
    }
}

public struct MergedPR: Sendable, Codable, Hashable, Identifiable {
    public var id: String
    public var repo: String
    public var number: Int
    public var title: String
    public var url: String
    public var author: Person?
    public var mergedAt: Date
    public var mergedBy: String?
    public var baseRef: String

    public init(
        id: String, repo: String, number: Int, title: String, url: String, author: Person? = nil,
        mergedAt: Date, mergedBy: String? = nil, baseRef: String = "main"
    ) {
        self.id = id
        self.repo = repo
        self.number = number
        self.title = title
        self.url = url
        self.author = author
        self.mergedAt = mergedAt
        self.mergedBy = mergedBy
        self.baseRef = baseRef
    }
}

public struct Release: Sendable, Codable, Hashable, Identifiable {
    public var id: String
    public var repo: String
    public var name: String
    public var tagName: String
    public var url: String
    public var publishedAt: Date?
    public var isPrerelease: Bool
    public var isLatest: Bool
    public var author: String?
    public var descriptionHTML: String

    public init(
        id: String, repo: String, name: String, tagName: String, url: String,
        publishedAt: Date? = nil, isPrerelease: Bool = false, isLatest: Bool = false,
        author: String? = nil, descriptionHTML: String = ""
    ) {
        self.id = id
        self.repo = repo
        self.name = name
        self.tagName = tagName
        self.url = url
        self.publishedAt = publishedAt
        self.isPrerelease = isPrerelease
        self.isLatest = isLatest
        self.author = author
        self.descriptionHTML = descriptionHTML
    }
}

public struct RepoInfo: Sendable, Codable, Hashable, Identifiable {
    public var fullName: String
    public var url: String
    public var defaultBranch: String?
    public var defaultBranchPipeline: PipelineState
    public var autoMergeAllowed: Bool
    public var mergeMethods: [MergeMethod]
    public var viewerCanMerge: Bool
    public var error: String?

    public var id: String { fullName }

    public init(
        fullName: String, url: String, defaultBranch: String? = nil,
        defaultBranchPipeline: PipelineState = .none, autoMergeAllowed: Bool = false,
        mergeMethods: [MergeMethod] = [], viewerCanMerge: Bool = false, error: String? = nil
    ) {
        self.fullName = fullName
        self.url = url
        self.defaultBranch = defaultBranch
        self.defaultBranchPipeline = defaultBranchPipeline
        self.autoMergeAllowed = autoMergeAllowed
        self.mergeMethods = mergeMethods
        self.viewerCanMerge = viewerCanMerge
        self.error = error
    }

    public static func unavailable(_ fullName: String, error: String) -> RepoInfo {
        RepoInfo(fullName: fullName, url: "https://github.com/\(fullName)", error: error)
    }
}

public struct Snapshot: Sendable, Codable, Hashable {
    public var viewer: String?
    public var repos: [RepoInfo]
    public var open: [OpenPR]
    public var merged: [MergedPR]
    public var releases: [Release]

    public init(
        viewer: String? = nil, repos: [RepoInfo] = [], open: [OpenPR] = [], merged: [MergedPR] = [],
        releases: [Release] = []
    ) {
        self.viewer = viewer
        self.repos = repos
        self.open = open
        self.merged = merged
        self.releases = releases
    }

    public func repo(_ fullName: String) -> RepoInfo? {
        repos.first { $0.fullName == fullName }
    }

    public func openPR(_ id: String) -> OpenPR? {
        open.first { $0.id == id }
    }

    public var anyRunning: Bool {
        open.contains { $0.pipeline.state == .running }
    }
}

public enum TransitionKind: String, Sendable, Hashable {
    case failed, passed, merged
}

public struct PRTransition: Sendable, Hashable {
    public var kind: TransitionKind
    public var pr: OpenPR

    public init(kind: TransitionKind, pr: OpenPR) {
        self.kind = kind
        self.pr = pr
    }
}

/// Relevante Zustandswechsel zwischen zwei Snapshots (für Benachrichtigungen).
public func diffSnapshots(_ prev: Snapshot, _ next: Snapshot) -> [PRTransition] {
    var out: [PRTransition] = []
    for pr in next.open {
        guard let old = prev.openPR(pr.id), old.pipeline.state != pr.pipeline.state else {
            continue
        }
        switch pr.pipeline.state {
        case .failed:
            out.append(PRTransition(kind: .failed, pr: pr))
        case .passed where old.pipeline.state == .running:
            out.append(PRTransition(kind: .passed, pr: pr))
        default:
            break
        }
    }
    let mergedIDs = Set(next.merged.map(\.id))
    let openIDs = Set(next.open.map(\.id))
    for old in prev.open where !openIDs.contains(old.id) && mergedIDs.contains(old.id) {
        out.append(PRTransition(kind: .merged, pr: old))
    }
    return out
}

/// IDs offener PRs, deren sichtbarer Zustand sich zwischen zwei Snapshots geändert hat.
public func changedPRs(_ prev: Snapshot, _ next: Snapshot) -> [String] {
    next.open.filter { pr in
        guard let old = prev.openPR(pr.id) else { return false }
        return old.pipeline.state != pr.pipeline.state
            || old.autoMerge != pr.autoMerge
            || old.reviewDecision != pr.reviewDecision
            || old.hasConflict != pr.hasConflict
    }.map(\.id)
}
