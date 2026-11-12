// GitHub-GraphQL-Client: Requests, Dashboard-Abfrage, Branch-Regeln, Mutationen.

import Foundation

#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

public struct RateLimit: Sendable, Hashable, Codable {
    public var remaining: Int
    public var limit: Int
    public var resetAt: Date?
    /// Punkte, die der letzte Poll gekostet hat (Summe über alle Requests)
    public var cost: Int

    public init(remaining: Int, limit: Int, resetAt: Date? = nil, cost: Int = 1) {
        self.remaining = remaining
        self.limit = limit
        self.resetAt = resetAt
        self.cost = cost
    }

    enum CodingKeys: String, CodingKey { case remaining, limit, resetAt, cost }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        remaining = try c.decode(Int.self, forKey: .remaining)
        limit = try c.decode(Int.self, forKey: .limit)
        resetAt = try c.decodeIfPresent(Date.self, forKey: .resetAt)
        // Ältere gespeicherte Stände kennen die Kosten noch nicht.
        cost = try c.decodeIfPresent(Int.self, forKey: .cost) ?? 1
    }
}

/// Fasst die Kontingent-Angaben mehrerer Requests eines Polls zusammen: Kosten addieren sich,
/// beim Rest zählt die knappste (jüngste) Angabe.
func mergeRateLimits(_ a: RateLimit?, _ b: RateLimit?) -> RateLimit? {
    guard let a else { return b }
    guard let b else { return a }
    var lower = b.remaining <= a.remaining ? b : a
    lower.cost = a.cost + b.cost
    return lower
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
    /// HTTP-Fehler; der zweite Wert ist GitHubs `message` (kann leer sein), nie der rohe Body.
    case http(Int, String)
    case graphQL(String)
    case unreadable(String)
    /// GitHub drosselt; `retryAt` = ab dann ist ein neuer Versuch sinnvoll (falls bekannt).
    case rateLimited(retryAt: Date?)

    public var errorDescription: String? {
        switch self {
        case .noToken:
            L10n.tr("Not signed in to GitHub. Sign in with GitHub in Settings or save a personal access token.")
        case .unauthorized:
            L10n.tr("GitHub rejected the token (401). Check the token.")
        case let .unreachable(detail):
            L10n.tr("GitHub is unreachable: %@", detail)
        case let .http(status, message):
            describeHTTPError(status, message: message)
        case let .graphQL(message):
            message
        case let .unreadable(detail):
            L10n.tr("Unexpected response from GitHub: %@", detail)
        case let .rateLimited(retryAt?):
            L10n.tr("GitHub API quota used up. Available again at %@.", TimeText.clock(retryAt))
        case .rateLimited(nil):
            L10n.tr("GitHub is throttling requests right now. Retrying in a few minutes.")
        }
    }
}

/// Verständlicher Text für einen HTTP-Fehler statt des rohen Antwort-Bodys.
func describeHTTPError(_ status: Int, message: String) -> String {
    let detail = message.trimmingCharacters(in: .whitespacesAndNewlines)
    let suffix = detail.isEmpty ? "" : L10n.tr(" GitHub says: “%@”", String(detail.prefix(160)))
    switch status {
    case 403:
        return L10n.tr("GitHub denied access (403). Does the token lack permissions for these repos?") + suffix
    case 404: return L10n.tr("GitHub does not know this address (404).") + suffix
    case 408, 504:
        return L10n.tr("GitHub did not respond in time. Retrying automatically. (HTTP %lld)", status)
    case 500..<600:
        return L10n.tr("GitHub is having problems right now. Retrying automatically. (HTTP %lld)", status)
    default: return L10n.tr("GitHub rejected the request.") + suffix
    }
}

/// Klartext für fehlende Rechte statt GitHubs Rohmeldung; `nil`, wenn es kein Rechtefehler ist.
func explainAccessError(_ message: String, type: String? = nil) -> String? {
    explainOAuthRestriction(message) ?? explainMissingWrite(message, type: type)
}

/// Klartext, wenn eine Org OAuth-Apps einschränkt und PR Radar nicht freigegeben hat. Gilt für
/// Lesen und Schreiben, daher auch beim Abruf der Repos.
func explainOAuthRestriction(_ message: String) -> String? {
    if message.lowercased().contains("oauth app access restrictions") {
        // „… the `acme` organization has enabled OAuth App access restrictions …“
        let parts = message.split(separator: "`", omittingEmptySubsequences: false)
        let org = parts.count > 2 ? String(parts[1]) : ""
        if org.isEmpty || org.contains(" ") {
            return L10n.tr("The organization has not approved PR Radar yet. An owner can allow it under Settings → Third-party access → OAuth app policy.")
        }
        return L10n.tr("The organization “%@” has not approved PR Radar yet. An owner can allow it under Settings → Third-party access → OAuth app policy.",
            org)
    }
    return nil
}

/// Klartext für fehlende Schreibrechte – nur für Mutationen: Beim Lesen bedeutet `FORBIDDEN`
/// etwas anderes (z. B. SAML oder ein Token ohne Zugriff aufs Repo).
func explainMissingWrite(_ message: String, type: String?) -> String? {
    let lower = message.lowercased()
    let patterns = [
        "resource not accessible by", "does not have the correct permissions", "does not have permission",
        "must have push access", "must have write access", "not authorized",
    ]
    guard type == "FORBIDDEN" || patterns.contains(where: lower.contains) else { return nil }
    return L10n.tr("No write access to this pull request. Merging and auto-merge need write access to the repo and, for the token, “Pull requests: write” (fine-grained PAT) or the `repo` scope.")
}

/// Fehler einer Mutation: Rechtefehler im Klartext, sonst GitHubs Meldungen.
func describeMutationErrors(_ errors: [GQLError]) -> String {
    errors.lazy.compactMap { explainAccessError($0.message, type: $0.type) }.first
        ?? errors.map(\.message).joined(separator: "; ")
}

/// GitHubs `message` aus einer Fehlerantwort (`{"message": "…"}`), sonst leer.
func gitHubMessage(_ data: Data) -> String {
    struct Body: Decodable { var message: String? }
    return (try? JSONDecoder().decode(Body.self, from: data))?.message ?? ""
}

/// Ab wann ein neuer Versuch laut Headern sinnvoll ist (Header-Namen klein geschrieben).
/// `x-ratelimit-reset` zählt nur bei leerem Kontingent.
func headerRetryAt(_ headers: [String: String], now: Date) -> Date? {
    let retryAfter = headers["retry-after"].flatMap { TimeInterval($0.trimmingCharacters(in: .whitespaces)) }
    let exhausted = headers["x-ratelimit-remaining"]?.trimmingCharacters(in: .whitespaces) == "0"
    let reset = headers["x-ratelimit-reset"].flatMap { TimeInterval($0.trimmingCharacters(in: .whitespaces)) }
        .map { Date(timeIntervalSince1970: $0) }
    return [exhausted ? reset : nil, retryAfter.map { now.addingTimeInterval($0) }].compactMap { $0 }.max()
}

/// Meldet GitHub im Body ein sekundäres Limit („secondary rate limit“, früher „abuse detection“)?
func mentionsSecondaryLimit(_ body: Data) -> Bool {
    let text = String(decoding: body, as: UTF8.self).lowercased()
    return text.contains("secondary rate limit") || text.contains("abuse detection")
}

/// Erkennt eine Drosselung anhand von Status, Headern (Namen klein geschrieben) und Body.
/// Ein sekundäres Limit ohne Zeitangabe bleibt ohne `retryAt`: `waitAfterError` wartet dann
/// `unknownRateLimitWait` und verlängert bei Wiederholung exponentiell, wie GitHub es verlangt.
func rateLimitError(status: Int, headers: [String: String], body: Data = Data(), now: Date) -> GitHubError? {
    let hasRetryAfter = headers["retry-after"].flatMap { TimeInterval($0.trimmingCharacters(in: .whitespaces)) } != nil
    let exhausted = headers["x-ratelimit-remaining"]?.trimmingCharacters(in: .whitespaces) == "0"
    let secondary = status == 403 && mentionsSecondaryLimit(body)
    guard status == 429 || (status == 403 && (exhausted || hasRetryAfter || secondary)) else { return nil }
    return .rateLimited(retryAt: headerRetryAt(headers, now: now))
}

/// Unter allen gedrosselten Chunks der mit der längsten Wartezeit (ohne Zeitangabe: `unknownRateLimitWait`).
func longestRateLimit(_ errors: [Error], now: Date) -> GitHubError? {
    errors.compactMap { error -> (GitHubError, Date)? in
        guard case let .rateLimited(retryAt)? = error as? GitHubError else { return nil }
        return (.rateLimited(retryAt: retryAt), retryAt ?? now.addingTimeInterval(unknownRateLimitWait))
    }
    .max { $0.1 < $1.1 }?.0
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
    /// Wie `send`, zusätzlich mit den Antwort-Headern (Namen klein geschrieben) – für Rate-Limits.
    func sendWithHeaders(_ request: URLRequest) async throws -> (Data, Int, [String: String])
}

extension HTTPTransport {
    public func sendWithHeaders(_ request: URLRequest) async throws -> (Data, Int, [String: String]) {
        let (data, status) = try await send(request)
        return (data, status, [:])
    }
}

public struct URLSessionTransport: HTTPTransport {
    let session: URLSession

    public init(session: URLSession = .shared) {
        self.session = session
    }

    public func send(_ request: URLRequest) async throws -> (Data, Int) {
        let (data, status, _) = try await sendWithHeaders(request)
        return (data, status)
    }

    public func sendWithHeaders(_ request: URLRequest) async throws -> (Data, Int, [String: String]) {
        let (data, response) = try await session.data(for: request)
        let http = response as? HTTPURLResponse
        var headers: [String: String] = [:]
        for (key, value) in http?.allHeaderFields ?? [:] {
            if let key = key as? String, let value = value as? String { headers[key.lowercased()] = value }
        }
        return (data, http?.statusCode ?? 0, headers)
    }
}

private let endpoint = URL(string: "https://api.github.com/graphql")!
/// Repos pro GraphQL-Request
private let chunkSize = 8
/// Wie lange Branch-Regeln gültig bleiben – sie ändern sich selten, kosten aber REST-Kontingent.
let rulesTTL: TimeInterval = 600

/// Ergebnis eines Abrufs der Branch-Regeln.
enum RulesFetch: Sendable {
    case fresh([RawRule], etag: String?)
    /// 304: Der zwischengespeicherte Stand gilt weiter.
    case notModified
    case failed
}

/// Branch-Regeln je (Repo, Branch). Bei Abruffehlern gilt der letzte bekannte Stand weiter,
/// damit nicht still wieder alle Merge-Methoden angeboten werden.
actor RulesCache {
    struct Key: Hashable {
        var repo: String
        var branch: String
    }

    /// `etag` der letzten Antwort: Eine Nachfrage mit `If-None-Match` beantwortet GitHub mit 304,
    /// und die zählt nicht gegen das Kontingent.
    private var entries: [Key: (at: Date, rules: [RawRule], etag: String?)] = [:]

    func fresh(_ key: Key, now: Date) -> [RawRule]? {
        guard let e = entries[key], now.timeIntervalSince(e.at) < rulesTTL else { return nil }
        return e.rules
    }

    func etag(_ key: Key) -> String? { entries[key]?.etag }

    /// Übernimmt ein Abrufergebnis; bei 304 gilt der alte Eintrag wieder `rulesTTL` lang, bei
    /// einem Fehler bleibt er gültig, wird aber beim nächsten Mal erneut angefragt.
    func update(_ key: Key, now: Date, fetched: RulesFetch) -> [RawRule]? {
        switch fetched {
        case let .fresh(rules, etag):
            entries[key] = (now, rules, etag)
            return rules
        case .notModified:
            guard var e = entries[key] else { return nil }
            e.at = now
            entries[key] = e
            return e.rules
        case .failed:
            return entries[key]?.rules
        }
    }
}

/// Zuletzt gesehene Zahl offener PRs je Repo – bestimmt die Seitengröße der nächsten Abfrage.
actor OpenCounts {
    private var counts: [String: Int] = [:]

    func pageSizes(_ repos: [String]) -> [(String, Int)] {
        repos.map { ($0, Queries.openPageSize(knownOpen: counts[$0])) }
    }

    /// Unbekannt (Fehler, kein Zugriff): beim nächsten Mal wieder die volle Seite.
    func remember(_ repo: String, _ total: Int?) {
        counts[repo] = total
    }
}

/// Rohdaten eines Abrufdurchgangs, je Repo in Abfragereihenfolge.
private struct Round {
    enum Entry {
        case repo(RawRepo)
        case unavailable(String)
    }

    var viewer: String?
    var repos: [(String, Entry)] = []
    var rateLimit: RateLimit?
}

public final class GitHubClient: GitHubAPI {
    private let transport: HTTPTransport
    private let token: @Sendable () -> String?
    private let rules = RulesCache()
    private let openCounts = OpenCounts()

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
        let headers: [String: String]
        do {
            (data, status, headers) = try await transport.sendWithHeaders(req)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            throw GitHubError.unreachable(error.localizedDescription)
        }
        if let limited = rateLimitError(status: status, headers: headers, body: data, now: Date()) { throw limited }
        if status == 401 { throw GitHubError.unauthorized }
        guard (200..<300).contains(status) else {
            throw GitHubError.http(status, gitHubMessage(data))
        }
        let response: GQLResponse
        do {
            response = try gitHubDecoder().decode(GQLResponse.self, from: data)
        } catch {
            throw GitHubError.unreadable(String(describing: error))
        }
        // GraphQL meldet das erschöpfte Kontingent mit HTTP 200 und Fehlertyp RATE_LIMITED;
        // die Reset-Zeit steht dann trotzdem in den Headern.
        if response.errors.contains(where: { $0.type == "RATE_LIMITED" }) {
            throw GitHubError.rateLimited(retryAt: headerRetryAt(headers, now: Date()))
        }
        return response
    }

    /// Führt eine Mutation aus; GraphQL-Fehler werden zu einem Fehler.
    private func mutate(_ query: String, variables: [String: String?]) async throws {
        let res = try await request(query, variables: variables)
        if !res.errors.isEmpty {
            throw GitHubError.graphQL(describeMutationErrors(res.errors))
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
        let sized = await openCounts.pageSizes(repos)
        var round = try await fetchRound(sized)
        // Mehr offene PRs als die verkleinerte Seite fasst: genau diese Repos sofort mit
        // passender Seite nachladen, damit kein PR auch nur für einen Poll fehlt.
        var truncated: [(String, Int)] = []
        for ((full, entry), (_, page)) in zip(round.repos, sized) {
            guard case let .repo(raw) = entry, let total = raw.open?.totalCount,
                total > page, page < Queries.maxOpenPage
            else { continue }
            truncated.append((full, Queries.openPageSize(knownOpen: total)))
        }
        if !truncated.isEmpty {
            let again = try await fetchRound(truncated)
            round.rateLimit = mergeRateLimits(round.rateLimit, again.rateLimit)
            for (full, entry) in again.repos {
                if let i = round.repos.firstIndex(where: { $0.0 == full }) { round.repos[i].1 = entry }
            }
        }

        var snapshot = Snapshot()
        snapshot.viewer = round.viewer
        for (full, entry) in round.repos {
            switch entry {
            case let .repo(raw):
                await openCounts.remember(full, raw.open?.totalCount)
                pushRepo(&snapshot, raw)
            case let .unavailable(message):
                await openCounts.remember(full, nil)
                snapshot.repos.append(.unavailable(full, error: message))
            }
        }
        let rateLimit = round.rateLimit

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

    /// Ein Durchgang über alle übergebenen Repos (in Blöcken zu je `chunkSize`, parallel).
    private func fetchRound(_ sized: [(String, Int)]) async throws -> Round {
        let chunks = stride(from: 0, to: sized.count, by: chunkSize).map {
            Array(sized[$0..<min($0 + chunkSize, sized.count)])
        }
        let results = await withTaskGroup(of: (Int, Result<GQLResponse, Error>).self) { group in
            for (i, chunk) in chunks.enumerated() {
                group.addTask {
                    do { return (i, .success(try await self.request(Queries.dashboard(chunk)))) } catch { return (i, .failure(error)) }
                }
            }
            var out = [Result<GQLResponse, Error>?](repeating: nil, count: chunks.count)
            for await (i, res) in group { out[i] = res }
            return out
        }
        // Ein Rate-Limit in irgendeinem Chunk zählt vor anderen Fehlern, damit der
        // Backoff bis zum Reset wartet statt zu früh erneut anzufragen.
        let failures = results.compactMap { result -> Error? in
            if case let .failure(error)? = result { return error }
            return nil
        }
        if let limited = longestRateLimit(failures, now: Date()) { throw limited }
        // Sonst Fehler in Chunk-Reihenfolge melden – wie die macOS-App
        let responses = try results.map { try $0!.get() }

        var round = Round()
        for (chunk, res) in zip(chunks, responses) {
            guard let data = res.data else {
                let msg = res.errors.map(\.message).joined(separator: "; ")
                throw GitHubError.graphQL(msg.isEmpty ? L10n.tr("Empty response from GitHub") : msg)
            }
            if round.viewer == nil { round.viewer = data.viewer }
            round.rateLimit = mergeRateLimits(round.rateLimit, data.rateLimit)
            for (i, (full, _)) in chunk.enumerated() {
                let alias = "r\(i)"
                switch data.repos[alias] {
                case let .repo(raw)?:
                    round.repos.append((full, .repo(raw)))
                case let .failed(message)?:
                    round.repos.append((full, .unavailable(L10n.tr("Unreadable response: %@", message))))
                case .null?, nil:
                    let err = res.errors.first { $0.path.first == alias }
                    let message = err.map { explainOAuthRestriction($0.message) ?? $0.message }
                    round.repos.append((full, .unavailable(message ?? L10n.tr("Repository not found or no access"))))
                }
            }
        }
        return round
    }

    /// Effektive Regeln eines Branches (inkl. Org-Rulesets); `nil`, wenn nicht abrufbar.
    private func branchRules(_ key: RulesCache.Key) async -> [RawRule]? {
        if let cached = await rules.fresh(key, now: Date()) { return cached }
        let fetched = await fetchBranchRules(key, etag: await rules.etag(key))
        return await rules.update(key, now: Date(), fetched: fetched)
    }

    func fetchBranchRules(_ key: RulesCache.Key, etag: String?) async -> RulesFetch {
        guard let token = try? bearer(),
            let url = URL(string: "https://api.github.com/repos/\(key.repo)/rules/branches/\(encodePath(key.branch))")
        else { return .failed }
        var req = URLRequest(url: url)
        req.setValue("bearer \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        req.setValue("pr-radar-ios", forHTTPHeaderField: "User-Agent")
        if let etag { req.setValue(etag, forHTTPHeaderField: "If-None-Match") }
        guard let (data, status, headers) = try? await transport.sendWithHeaders(req) else { return .failed }
        if status == 304 { return .notModified }
        guard (200..<300).contains(status), let rules = decodeRules(data) else { return .failed }
        return .fresh(rules, etag: headers["etag"])
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
    /// Fehlertyp, z. B. `RATE_LIMITED` oder `NOT_FOUND`.
    var type: String?

    enum CodingKeys: String, CodingKey { case message, path, type }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        message = try c.decodeIfPresent(String.self, forKey: .message) ?? L10n.tr("Unknown error")
        type = try? c.decodeIfPresent(String.self, forKey: .type)
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
        var resetAt: String?
        var cost: Int?
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: DynamicKey.self)
        viewer = (try? c.decodeIfPresent(Viewer.self, forKey: DynamicKey("viewer")))??.login
        if let rl = try? c.decodeIfPresent(RawRateLimit.self, forKey: DynamicKey("rateLimit")) {
            rateLimit = RateLimit(
                remaining: rl.remaining ?? 0, limit: rl.limit ?? 0, resetAt: rl.resetAt.flatMap(parseISO8601),
                cost: rl.cost ?? 1)
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

/// Branch-Regeln tolerant lesen: unlesbare Einträge überspringen statt die ganze Antwort zu
/// verwerfen; eine Antwort, die kein Array ist, gilt als „keine Regeln“ (wie die macOS-App).
func decodeRules(_ data: Data) -> [RawRule]? {
    guard let json = try? JSONSerialization.jsonObject(with: data) else { return nil }
    guard let items = json as? [Any] else { return [] }
    return items.compactMap { item in
        guard let rule = item as? [String: Any], let type = rule["type"] as? String else { return nil }
        let allowed = ((rule["parameters"] as? [String: Any])?["allowed_merge_methods"] as? [Any])?
            .compactMap { $0 as? String }
        return RawRule(type: type, parameters: allowed.map { RawRule.Parameters(allowedMergeMethods: $0) })
    }
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
