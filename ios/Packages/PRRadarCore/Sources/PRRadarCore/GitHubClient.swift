// GitHub-GraphQL-Client: Requests, Dashboard-Abfrage, Branch-Regeln, Mutationen.

import Foundation

#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

public struct RateLimit: Sendable, Hashable, Codable {
    public var remaining: Int
    public var limit: Int
    public var resetAt: Date?

    public init(remaining: Int, limit: Int, resetAt: Date? = nil) {
        self.remaining = remaining
        self.limit = limit
        self.resetAt = resetAt
    }
}

/// Ergebnis eines Abrufs aller Repos.
public struct Fetched: Sendable {
    public var snapshot: Snapshot
    public var rateLimit: RateLimit?

    public init(snapshot: Snapshot, rateLimit: RateLimit? = nil) {
        self.snapshot = snapshot
        self.rateLimit = rateLimit
    }
}

public enum GitHubError: LocalizedError, Equatable, Sendable {
    case noToken
    case unauthorized
    case unreachable(String)
    case http(Int, String)
    case graphQL(String)
    case unreadable(String)

    public var errorDescription: String? {
        switch self {
        case .noToken:
            "Kein GitHub-Token hinterlegt. Trage in den Einstellungen einen Personal Access Token ein."
        case .unauthorized:
            "GitHub lehnt den Token ab (401). Token prüfen."
        case let .unreachable(detail):
            "GitHub nicht erreichbar: \(detail)"
        case let .http(status, body):
            "GitHub API \(status): \(body)"
        case let .graphQL(message):
            message
        case let .unreadable(detail):
            "Unerwartete Antwort von GitHub: \(detail)"
        }
    }
}

/// Was die App von GitHub braucht – als Protokoll, damit `Radar` ohne Netz testbar ist.
public protocol GitHubAPI: Sendable {
    func fetch(repos: [String]) async throws -> Fetched
    /// Auto-Merge an (`method`) oder aus (`nil`). `head`: nur, wenn der Branch noch auf diesem Commit steht.
    func setAutoMerge(prID: String, method: MergeMethod?, head: String?) async throws
    /// Mergt – nur, wenn der Branch noch auf `head` steht (sonst lehnt GitHub ab).
    func merge(prID: String, method: MergeMethod, head: String) async throws
}

/// Schickt einen Request und liefert Body und HTTP-Status.
public protocol HTTPTransport: Sendable {
    func send(_ request: URLRequest) async throws -> (Data, Int)
}

public struct URLSessionTransport: HTTPTransport {
    let session: URLSession

    public init(session: URLSession = .shared) {
        self.session = session
    }

    public func send(_ request: URLRequest) async throws -> (Data, Int) {
        let (data, response) = try await session.data(for: request)
        return (data, (response as? HTTPURLResponse)?.statusCode ?? 0)
    }
}

private let endpoint = URL(string: "https://api.github.com/graphql")!
/// Repos pro GraphQL-Request
private let chunkSize = 8
/// Wie lange Branch-Regeln gültig bleiben – sie ändern sich selten, kosten aber REST-Kontingent.
let rulesTTL: TimeInterval = 600

/// Branch-Regeln je (Repo, Branch). Bei Abruffehlern gilt der letzte bekannte Stand weiter,
/// damit nicht still wieder alle Merge-Methoden angeboten werden.
actor RulesCache {
    struct Key: Hashable {
        var repo: String
        var branch: String
    }

    private var entries: [Key: (at: Date, rules: [RawRule])] = [:]

    func fresh(_ key: Key, now: Date) -> [RawRule]? {
        guard let e = entries[key], now.timeIntervalSince(e.at) < rulesTTL else { return nil }
        return e.rules
    }

    /// Übernimmt ein Abrufergebnis; ohne Ergebnis bleibt der alte Eintrag gültig.
    func update(_ key: Key, now: Date, fetched: [RawRule]?) -> [RawRule]? {
        if let fetched {
            entries[key] = (now, fetched)
            return fetched
        }
        return entries[key]?.rules
    }
}

public final class GitHubClient: GitHubAPI {
    private let transport: HTTPTransport
    private let token: @Sendable () -> String?
    private let rules = RulesCache()

    /// `token` wird bei jedem Request gelesen – ein in den Einstellungen geänderter Token greift sofort.
    public init(transport: HTTPTransport = URLSessionTransport(), token: @escaping @Sendable () -> String?) {
        self.transport = transport
        self.token = token
    }

    private func bearer() throws -> String {
        guard let t = token()?.trimmingCharacters(in: .whitespacesAndNewlines), !t.isEmpty else {
            throw GitHubError.noToken
        }
        return t
    }

    private func request(_ query: String, variables: [String: String?] = [:]) async throws -> GQLResponse {
        var req = URLRequest(url: endpoint)
        req.httpMethod = "POST"
        req.setValue("bearer \(try bearer())", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.setValue("pr-radar-ios", forHTTPHeaderField: "User-Agent")
        req.httpBody = try JSONEncoder().encode(GQLRequest(query: query, variables: variables))
        req.timeoutInterval = 30

        let data: Data
        let status: Int
        do {
            (data, status) = try await transport.send(req)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            throw GitHubError.unreachable(error.localizedDescription)
        }
        if status == 401 { throw GitHubError.unauthorized }
        guard (200..<300).contains(status) else {
            let body = String(decoding: data, as: UTF8.self)
            throw GitHubError.http(status, String(body.prefix(300)))
        }
        do {
            return try gitHubDecoder().decode(GQLResponse.self, from: data)
        } catch {
            throw GitHubError.unreadable(String(describing: error))
        }
    }

    /// Führt eine Mutation aus; GraphQL-Fehler werden zu einem Fehler.
    private func mutate(_ query: String, variables: [String: String?]) async throws {
        let res = try await request(query, variables: variables)
        if !res.errors.isEmpty {
            throw GitHubError.graphQL(res.errors.map(\.message).joined(separator: "; "))
        }
    }

    public func setAutoMerge(prID: String, method: MergeMethod?, head: String?) async throws {
        if let method {
            try await mutate(Queries.enableAutoMerge, variables: ["id": prID, "method": method.graphQL, "head": head])
        } else {
            try await mutate(Queries.disableAutoMerge, variables: ["id": prID])
        }
    }

    public func merge(prID: String, method: MergeMethod, head: String) async throws {
        try await mutate(Queries.mergePR, variables: ["id": prID, "method": method.graphQL, "head": head])
    }

    /// Lädt alle Repos (in Blöcken zu je `chunkSize`, parallel) und baut daraus einen Snapshot.
    public func fetch(repos: [String]) async throws -> Fetched {
        let chunks = stride(from: 0, to: repos.count, by: chunkSize).map {
            Array(repos[$0..<min($0 + chunkSize, repos.count)])
        }
        let responses = try await withThrowingTaskGroup(of: (Int, GQLResponse).self) { group in
            for (i, chunk) in chunks.enumerated() {
                group.addTask { (i, try await self.request(Queries.dashboard(chunk))) }
            }
            var out = [GQLResponse?](repeating: nil, count: chunks.count)
            for try await (i, res) in group { out[i] = res }
            return out.compactMap { $0 }
        }

        var snapshot = Snapshot()
        var rateLimit: RateLimit?
        for (chunk, res) in zip(chunks, responses) {
            guard let data = res.data else {
                let msg = res.errors.map(\.message).joined(separator: "; ")
                throw GitHubError.graphQL(msg.isEmpty ? "Leere Antwort von GitHub" : msg)
            }
            if snapshot.viewer == nil { snapshot.viewer = data.viewer }
            if let rl = data.rateLimit { rateLimit = rl }
            for (i, full) in chunk.enumerated() {
                let alias = "r\(i)"
                switch data.repos[alias] {
                case let .repo(raw)?:
                    pushRepo(&snapshot, raw)
                case let .failed(message)?:
                    snapshot.repos.append(.unavailable(full, error: "Antwort nicht lesbar: \(message)"))
                case .null?, nil:
                    let err = res.errors.first { $0.path.first == alias }
                    snapshot.repos.append(
                        .unavailable(full, error: err?.message ?? "Repository nicht gefunden oder kein Zugriff"))
                }
            }
        }

        await applyRulesets(&snapshot)
        snapshot.open.sort { $0.updatedAt > $1.updatedAt }
        snapshot.merged.sort { $0.mergedAt > $1.mergedAt }
        snapshot.releases.sort { ($0.publishedAt ?? .distantPast) > ($1.publishedAt ?? .distantPast) }
        return Fetched(snapshot: snapshot, rateLimit: rateLimit)
    }

    /// Setzt je PR die effektiv erlaubten Merge-Methoden (Repo-Einstellungen ∩ Rulesets des Ziel-Branches).
    private func applyRulesets(_ snapshot: inout Snapshot) async {
        var targets: [RulesCache.Key] = []
        for pr in snapshot.open {
            let key = RulesCache.Key(repo: pr.repo, branch: pr.baseRef)
            if !targets.contains(key) { targets.append(key) }
        }
        let rules = await withTaskGroup(of: (RulesCache.Key, [RawRule]?).self) { group in
            for key in targets {
                group.addTask { (key, await self.branchRules(key)) }
            }
            var out: [RulesCache.Key: [RawRule]] = [:]
            for await (key, value) in group { out[key] = value }
            return out
        }
        let repoMethods = Dictionary(snapshot.repos.map { ($0.fullName, $0.mergeMethods) }) { a, _ in a }
        for i in snapshot.open.indices {
            let pr = snapshot.open[i]
            let methods = repoMethods[pr.repo] ?? []
            // Regeln nicht lesbar: GitHub lehnt Unerlaubtes beim Mergen ohnehin ab.
            snapshot.open[i].mergeMethods =
                rules[RulesCache.Key(repo: pr.repo, branch: pr.baseRef)].map { applyBranchRules(methods, $0) }
                ?? methods
        }
    }

    /// Effektive Regeln eines Branches (inkl. Org-Rulesets); `nil`, wenn nicht abrufbar.
    private func branchRules(_ key: RulesCache.Key) async -> [RawRule]? {
        if let cached = await rules.fresh(key, now: Date()) { return cached }
        let fetched = await fetchBranchRules(key)
        return await rules.update(key, now: Date(), fetched: fetched)
    }

    private func fetchBranchRules(_ key: RulesCache.Key) async -> [RawRule]? {
        guard let token = try? bearer(),
            let url = URL(string: "https://api.github.com/repos/\(key.repo)/rules/branches/\(encodePath(key.branch))")
        else { return nil }
        var req = URLRequest(url: url)
        req.setValue("bearer \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        req.setValue("pr-radar-ios", forHTTPHeaderField: "User-Agent")
        guard let (data, status) = try? await transport.send(req), (200..<300).contains(status) else {
            return nil
        }
        return try? JSONDecoder().decode([RawRule].self, from: data)
    }
}

// MARK: - GraphQL-Hüllen

private struct GQLRequest: Encodable {
    var query: String
    var variables: [String: String?]

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(query, forKey: .query)
        // `nil` als JSON-null, nicht weglassen – GitHub erwartet die Variable.
        var vars = c.nestedContainer(keyedBy: DynamicKey.self, forKey: .variables)
        for (k, v) in variables {
            if let v { try vars.encode(v, forKey: DynamicKey(k)) } else { try vars.encodeNil(forKey: DynamicKey(k)) }
        }
    }

    enum CodingKeys: String, CodingKey { case query, variables }
}

struct GQLError: Decodable, Sendable {
    var message: String
    /// Nur die String-Teile des Pfads (Aliase wie `r0`); Indizes werden ausgelassen.
    var path: [String]

    enum CodingKeys: String, CodingKey { case message, path }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        message = try c.decodeIfPresent(String.self, forKey: .message) ?? "Unbekannter Fehler"
        var out: [String] = []
        if var p = try? c.nestedUnkeyedContainer(forKey: .path) {
            while !p.isAtEnd {
                if let s = try? p.decode(String.self) {
                    out.append(s)
                } else {
                    _ = try? p.decode(Int.self)
                }
            }
        }
        path = out
    }
}

/// Ein Repo-Alias der Antwort: gelesen, `null` (kein Zugriff) oder nicht lesbar.
enum RepoSlot: Sendable {
    case repo(RawRepo)
    case null
    case failed(String)
}

struct DashboardData: Decodable, Sendable {
    var viewer: String?
    var rateLimit: RateLimit?
    var repos: [String: RepoSlot]

    private struct Viewer: Decodable { var login: String? }
    private struct RawRateLimit: Decodable {
        var remaining: Int?
        var limit: Int?
        var resetAt: Date?
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: DynamicKey.self)
        viewer = (try? c.decodeIfPresent(Viewer.self, forKey: DynamicKey("viewer")))??.login
        if let rl = try? c.decodeIfPresent(RawRateLimit.self, forKey: DynamicKey("rateLimit")) {
            rateLimit = RateLimit(remaining: rl.remaining ?? 0, limit: rl.limit ?? 0, resetAt: rl.resetAt)
        }
        var repos: [String: RepoSlot] = [:]
        for key in c.allKeys where key.stringValue.hasPrefix("r") && Int(key.stringValue.dropFirst()) != nil {
            if (try? c.decodeNil(forKey: key)) == true {
                repos[key.stringValue] = .null
                continue
            }
            do {
                repos[key.stringValue] = .repo(try c.decode(RawRepo.self, forKey: key))
            } catch {
                repos[key.stringValue] = .failed(String(describing: error))
            }
        }
        self.repos = repos
    }
}

struct GQLResponse: Decodable, Sendable {
    var data: DashboardData?
    var errors: [GQLError]

    enum CodingKeys: String, CodingKey { case data, errors }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        data = try c.decodeIfPresent(DashboardData.self, forKey: .data)
        errors = try c.decodeIfPresent([GQLError].self, forKey: .errors) ?? []
    }
}

struct DynamicKey: CodingKey {
    var stringValue: String
    var intValue: Int? { nil }

    init(_ s: String) { stringValue = s }
    init?(stringValue: String) { self.stringValue = stringValue }
    init?(intValue: Int) { nil }
}

/// Prozent-Kodierung für ein Pfadsegment (Branch-Namen dürfen `/` enthalten).
func encodePath(_ s: String) -> String {
    s.utf8.map { b -> String in
        switch b {
        case UInt8(ascii: "A")...UInt8(ascii: "Z"), UInt8(ascii: "a")...UInt8(ascii: "z"),
            UInt8(ascii: "0")...UInt8(ascii: "9"), UInt8(ascii: "-"), UInt8(ascii: "."),
            UInt8(ascii: "_"), UInt8(ascii: "~"):
            String(UnicodeScalar(b))
        default:
            String(format: "%%%02X", b)
        }
    }.joined()
}
