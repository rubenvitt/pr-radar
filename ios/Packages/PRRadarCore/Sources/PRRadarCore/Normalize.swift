// GraphQL-Rohdaten → Domänenmodell.

import Foundation

// MARK: - Rohdaten (Ausschnitt der GraphQL-Antwort)

struct Nodes<T: Decodable & Sendable>: Decodable, Sendable {
    var nodes: [T?]

    init(_ nodes: [T?] = []) { self.nodes = nodes }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        nodes = try c.decodeIfPresent([T?].self, forKey: .nodes) ?? []
    }

    var items: [T] { nodes.compactMap { $0 } }

    private enum CodingKeys: String, CodingKey { case nodes }
}

enum RawContext: Decodable, Sendable {
    case checkRun(name: String, status: String, conclusion: String?, detailsURL: String?, workflow: String?)
    case statusContext(context: String, state: String, targetURL: String?)
    case other

    private enum Keys: String, CodingKey {
        case typename = "__typename"
        case name, status, conclusion, detailsUrl, checkSuite
        case context, state, targetUrl
    }

    private struct Suite: Decodable {
        struct Run: Decodable {
            struct Workflow: Decodable { var name: String? }
            var workflow: Workflow?
        }
        var workflowRun: Run?
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        switch try c.decodeIfPresent(String.self, forKey: .typename) {
        case "CheckRun":
            self = .checkRun(
                name: try c.decodeIfPresent(String.self, forKey: .name) ?? "Check",
                status: try c.decodeIfPresent(String.self, forKey: .status) ?? "",
                conclusion: try c.decodeIfPresent(String.self, forKey: .conclusion),
                detailsURL: try c.decodeIfPresent(String.self, forKey: .detailsUrl),
                workflow: try c.decodeIfPresent(Suite.self, forKey: .checkSuite)?.workflowRun?.workflow?.name
            )
        case "StatusContext":
            self = .statusContext(
                context: try c.decodeIfPresent(String.self, forKey: .context) ?? "Status",
                state: try c.decodeIfPresent(String.self, forKey: .state) ?? "",
                targetURL: try c.decodeIfPresent(String.self, forKey: .targetUrl)
            )
        default:
            self = .other
        }
    }
}

struct RawRollup: Decodable, Sendable {
    var state: String?
    var contexts: Nodes<RawContext>?
}

struct RawPerson: Decodable, Sendable {
    var login: String
    var avatarUrl: String?
}

struct RawRepo: Decodable, Sendable {
    struct BranchRef: Decodable, Sendable {
        struct Target: Decodable, Sendable { var statusCheckRollup: RawRollup? }
        var name: String
        var target: Target?
    }

    var nameWithOwner: String
    var url: String
    var autoMergeAllowed: Bool?
    var squashMergeAllowed: Bool?
    var mergeCommitAllowed: Bool?
    var rebaseMergeAllowed: Bool?
    var viewerPermission: String?
    var defaultBranchRef: BranchRef?
    var open: Nodes<RawOpen>?
    var merged: Nodes<RawMerged>?
    var releases: Nodes<RawRelease>?
}

struct RawOpen: Decodable, Sendable {
    struct AutoMergeRequest: Decodable, Sendable {
        var mergeMethod: String
        var enabledBy: RawPerson?
    }
    struct LabelNode: Decodable, Sendable {
        var name: String
        var color: String
    }
    struct ReviewRequest: Decodable, Sendable {
        struct Reviewer: Decodable, Sendable {
            var login: String?
            var slug: String?
        }
        var requestedReviewer: Reviewer?
    }
    struct CommitNode: Decodable, Sendable {
        struct Commit: Decodable, Sendable { var statusCheckRollup: RawRollup? }
        var commit: Commit?
    }

    var id: String
    var number: Int
    var title: String
    var url: String
    var isDraft: Bool?
    var updatedAt: Date
    var headRefName: String
    var headRefOid: String?
    var baseRefName: String
    var additions: Int?
    var deletions: Int?
    var reviewDecision: String?
    var mergeable: String?
    var mergeStateStatus: String?
    var author: RawPerson?
    var autoMergeRequest: AutoMergeRequest?
    var labels: Nodes<LabelNode>?
    var reviewRequests: Nodes<ReviewRequest>?
    var commits: Nodes<CommitNode>?
}

struct RawMerged: Decodable, Sendable {
    var id: String
    var number: Int
    var title: String
    var url: String
    var mergedAt: Date?
    var baseRefName: String
    var author: RawPerson?
    var mergedBy: RawPerson?
}

struct RawRelease: Decodable, Sendable {
    var id: String
    var name: String?
    var tagName: String
    var url: String
    var publishedAt: Date?
    var isPrerelease: Bool?
    var isLatest: Bool?
    var isDraft: Bool?
    var descriptionHTML: String?
    var author: RawPerson?
}

/// Eine Regel aus `GET /repos/{repo}/rules/branches/{branch}`.
struct RawRule: Decodable, Sendable, Equatable {
    struct Parameters: Decodable, Sendable, Equatable {
        var allowedMergeMethods: [String]?

        enum CodingKeys: String, CodingKey {
            case allowedMergeMethods = "allowed_merge_methods"
        }
    }

    var type: String
    var parameters: Parameters?
}

// MARK: - Checks & Pipeline

private let failedConclusions: Set<String> = [
    "FAILURE", "TIMED_OUT", "CANCELLED", "STARTUP_FAILURE", "ACTION_REQUIRED", "STALE",
]

func normalizeCheck(_ ctx: RawContext) -> Check? {
    switch ctx {
    case let .statusContext(context, state, targetURL):
        let s: CheckState =
            switch state {
            case "SUCCESS": .passed
            case "FAILURE", "ERROR": .failed
            default: .running
            }
        return Check(name: context, state: s, url: targetURL, group: nil)
    case let .checkRun(name, status, conclusion, detailsURL, workflow):
        let s: CheckState
        if status != "COMPLETED" {
            s = .running
        } else if conclusion == "SUCCESS" {
            s = .passed
        } else if conclusion == "SKIPPED" {
            s = .skipped
        } else if let conclusion, failedConclusions.contains(conclusion) {
            s = .failed
        } else {
            s = .neutral
        }
        return Check(name: name, state: s, url: detailsURL, group: workflow)
    case .other:
        return nil
    }
}

func rollupState(_ state: String?) -> PipelineState {
    switch state {
    case "SUCCESS": .passed
    case "FAILURE", "ERROR": .failed
    case "PENDING", "EXPECTED": .running
    default: .none
    }
}

func buildPipeline(_ rollup: RawRollup?) -> Pipeline {
    let checks = (rollup?.contexts?.items ?? [])
        .compactMap(normalizeCheck)
        .sorted { a, b in
            a.state.rank != b.state.rank ? a.state.rank < b.state.rank : a.name < b.name
        }
    let count = { (s: CheckState) in checks.filter { $0.state == s }.count }
    let failed = count(.failed)
    let running = count(.running)
    let passed = count(.passed)

    let state: PipelineState =
        if failed > 0 { .failed }
        else if running > 0 { .running }
        else if !checks.isEmpty { .passed }
        else { rollupState(rollup?.state) }

    return Pipeline(
        state: state, total: checks.count, passed: passed, failed: failed, running: running,
        checks: checks)
}

// MARK: - PRs, Releases, Repo

private func person(_ p: RawPerson?) -> Person? {
    p.map { Person(login: $0.login, avatarURL: $0.avatarUrl ?? "") }
}

func normalizeOpen(repo: String, _ n: RawOpen) -> OpenPR {
    let rollup = n.commits?.items.first?.commit?.statusCheckRollup
    let review: ReviewDecision? =
        switch n.reviewDecision {
        case "APPROVED": .approved
        case "CHANGES_REQUESTED": .changesRequested
        case "REVIEW_REQUIRED": .reviewRequired
        default: nil
        }
    let mergeable: Mergeable =
        switch n.mergeable {
        case "MERGEABLE": .mergeable
        case "CONFLICTING": .conflicting
        default: .unknown
        }
    return OpenPR(
        id: n.id,
        repo: repo,
        number: n.number,
        title: n.title,
        url: n.url,
        author: person(n.author),
        isDraft: n.isDraft ?? false,
        updatedAt: n.updatedAt,
        headRef: n.headRefName,
        headOID: n.headRefOid ?? "",
        baseRef: n.baseRefName,
        additions: n.additions ?? 0,
        deletions: n.deletions ?? 0,
        reviewDecision: review,
        mergeable: mergeable,
        mergeStateStatus: n.mergeStateStatus ?? "UNKNOWN",
        autoMerge: n.autoMergeRequest.flatMap { a in
            MergeMethod(graphQL: a.mergeMethod).map { AutoMerge(method: $0, enabledBy: a.enabledBy?.login) }
        },
        labels: (n.labels?.items ?? []).map { PRLabel(name: $0.name, color: $0.color) },
        reviewRequests: (n.reviewRequests?.items ?? []).compactMap { r in
            guard let rv = r.requestedReviewer else { return nil }
            return rv.login ?? rv.slug.map { "@\($0)" }
        },
        pipeline: buildPipeline(rollup),
        // wird nach dem Laden der Rulesets gesetzt
        mergeMethods: []
    )
}

/// Schränkt die Repo-Methoden mit den effektiven Branch-Regeln ein
/// (`GET /repos/{repo}/rules/branches/{branch}`): `pull_request.allowed_merge_methods`
/// und `required_linear_history` (verbietet Merge-Commits).
func applyBranchRules(_ repoMethods: [MergeMethod], _ rules: [RawRule]) -> [MergeMethod] {
    var methods = repoMethods
    for rule in rules {
        switch rule.type {
        case "pull_request":
            if let allowed = rule.parameters?.allowedMergeMethods {
                let allowed = Set(allowed.compactMap { MergeMethod(graphQL: $0.uppercased()) })
                methods.removeAll { !allowed.contains($0) }
            }
        case "required_linear_history":
            methods.removeAll { $0 == .merge }
        default:
            break
        }
    }
    return methods
}

func normalizeMerged(repo: String, _ n: RawMerged) -> MergedPR? {
    guard let mergedAt = n.mergedAt else { return nil }
    return MergedPR(
        id: n.id, repo: repo, number: n.number, title: n.title, url: n.url,
        author: person(n.author), mergedAt: mergedAt, mergedBy: n.mergedBy?.login,
        baseRef: n.baseRefName)
}

func normalizeRelease(repo: String, _ n: RawRelease) -> Release? {
    if n.isDraft == true { return nil }
    let name = n.name.flatMap { $0.isEmpty ? nil : $0 } ?? n.tagName
    return Release(
        id: n.id, repo: repo, name: name, tagName: n.tagName, url: n.url,
        publishedAt: n.publishedAt, isPrerelease: n.isPrerelease ?? false,
        isLatest: n.isLatest ?? false, author: n.author?.login,
        descriptionHTML: n.descriptionHTML ?? "")
}

func normalizeRepo(_ raw: RawRepo) -> RepoInfo {
    var methods: [MergeMethod] = []
    if raw.squashMergeAllowed == true { methods.append(.squash) }
    if raw.mergeCommitAllowed == true { methods.append(.merge) }
    if raw.rebaseMergeAllowed == true { methods.append(.rebase) }
    return RepoInfo(
        fullName: raw.nameWithOwner,
        url: raw.url,
        defaultBranch: raw.defaultBranchRef?.name,
        defaultBranchPipeline: rollupState(raw.defaultBranchRef?.target?.statusCheckRollup?.state),
        autoMergeAllowed: raw.autoMergeAllowed ?? false,
        mergeMethods: methods,
        viewerCanMerge: ["ADMIN", "MAINTAIN", "WRITE"].contains(raw.viewerPermission ?? ""),
        error: nil)
}

/// Fügt ein Repo mit allen PRs, Merges und Releases in den Snapshot ein.
func pushRepo(_ snapshot: inout Snapshot, _ raw: RawRepo) {
    let name = raw.nameWithOwner
    snapshot.repos.append(normalizeRepo(raw))
    snapshot.open += (raw.open?.items ?? []).map { normalizeOpen(repo: name, $0) }
    snapshot.merged += (raw.merged?.items ?? []).compactMap { normalizeMerged(repo: name, $0) }
    snapshot.releases += (raw.releases?.items ?? []).compactMap { normalizeRelease(repo: name, $0) }
}

/// JSON-Decoder für GitHub: ISO-8601-Zeitstempel, mit und ohne Sekundenbruchteile.
func gitHubDecoder() -> JSONDecoder {
    let decoder = JSONDecoder()
    decoder.dateDecodingStrategy = .custom { d in
        let s = try d.singleValueContainer().decode(String.self)
        if let date = parseISO8601(s) { return date }
        throw DecodingError.dataCorrupted(
            .init(codingPath: d.codingPath, debugDescription: "Ungültiges Datum: \(s)"))
    }
    return decoder
}

func parseISO8601(_ s: String) -> Date? {
    if let date = try? Date(s, strategy: .iso8601) { return date }
    let fractional = Date.ISO8601FormatStyle(includingFractionalSeconds: true)
    return try? fractional.parse(s)
}
