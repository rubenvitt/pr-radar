import Foundation
import Testing

@testable import PRRadarCore

#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

func makePR(
    _ id: String, state: PipelineState = .passed, head: String = "abc", status: String = "CLEAN",
    mergeable: Mergeable = .mergeable, auto: AutoMerge? = nil, methods: [MergeMethod] = [.merge],
    repo: String = "a/b", base: String = "main"
) -> OpenPR {
    OpenPR(
        id: id, repo: repo, number: Int(id.filter(\.isNumber)) ?? 1, title: "t\(id)", url: "u", headRef: "h",
        headOID: head, baseRef: base, mergeable: mergeable, mergeStateStatus: status, autoMerge: auto,
        pipeline: Pipeline(state: state), mergeMethods: methods)
}

func repoInfo(_ name: String = "a/b", autoMerge: Bool = true, canMerge: Bool = true) -> RepoInfo {
    RepoInfo(
        fullName: name, url: "https://github.com/\(name)", defaultBranch: "main", autoMergeAllowed: autoMerge,
        mergeMethods: [.squash, .merge], viewerCanMerge: canMerge)
}

func fixture(_ name: String) throws -> Data {
    let url = try #require(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
    return try Data(contentsOf: url)
}

/// Antwortet je nach URL: GraphQL → Fixture, Regeln → vorgegebenes JSON.
final class FakeTransport: HTTPTransport, @unchecked Sendable {
    private let lock = NSLock()
    var graphQL: Data
    var rules: [String: (Data, Int)]
    private(set) var requests: [URLRequest] = []

    init(graphQL: Data, rules: [String: (Data, Int)] = [:]) {
        self.graphQL = graphQL
        self.rules = rules
    }

    func send(_ request: URLRequest) async throws -> (Data, Int) {
        lock.withLock { requests.append(request) }
        let url = request.url!.absoluteString
        if url.hasSuffix("/graphql") { return (graphQL, 200) }
        return lock.withLock { rules.first { url.hasSuffix($0.key) }?.value } ?? (Data("[]".utf8), 404)
    }
}

/// GitHub-Attrappe: Merge-Antworten der Reihe nach, Auto-Merge an/aus mitprotokolliert.
actor FakeGitHub: GitHubAPI {
    var snapshot: Snapshot
    var mergeErrors: [String?]
    var autoMergeError: String?
    private(set) var merges: [(String, MergeMethod, String)] = []
    private(set) var autoMerges: [(String, MergeMethod?, String?)] = []

    init(snapshot: Snapshot, mergeErrors: [String?] = [], autoMergeError: String? = nil) {
        self.snapshot = snapshot
        self.mergeErrors = mergeErrors
        self.autoMergeError = autoMergeError
    }

    func setSnapshot(_ s: Snapshot) { snapshot = s }

    func fetch(repos: [String]) async throws -> Fetched {
        Fetched(snapshot: snapshot)
    }

    func setAutoMerge(prID: String, method: MergeMethod?, head: String?) async throws {
        autoMerges.append((prID, method, head))
        if let autoMergeError { throw GitHubError.graphQL(autoMergeError) }
        if let i = snapshot.open.firstIndex(where: { $0.id == prID }) {
            snapshot.open[i].autoMerge = method.map { AutoMerge(method: $0) }
        }
    }

    func merge(prID: String, method: MergeMethod, head: String) async throws {
        merges.append((prID, method, head))
        let error = mergeErrors.isEmpty ? nil : mergeErrors.removeFirst()
        if let error { throw GitHubError.graphQL(error) }
        snapshot.open.removeAll { $0.id == prID }
    }
}

/// Uhr, die bei jedem `sleep` vorrückt statt zu warten.
final class FakeClock: @unchecked Sendable {
    private let lock = NSLock()
    private var current = Date(timeIntervalSince1970: 1_800_000_000)
    private(set) var sleeps: [TimeInterval] = []

    var timing: Timing {
        Timing(
            now: { self.lock.withLock { self.current } },
            sleep: { d in
                self.lock.withLock {
                    self.sleeps.append(d)
                    self.current += d
                }
                await Task.yield()
            })
    }
}
