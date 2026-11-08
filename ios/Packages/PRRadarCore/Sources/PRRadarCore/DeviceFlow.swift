// GitHub-Anmeldung per OAuth Device Flow: Die App zeigt einen Code, der Nutzer bestätigt ihn auf
// github.com/login/device, die App holt sich danach den Token ab. Kein Client-Secret nötig.
// Ablauf und Fehlercodes: https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps#device-flow

import Foundation

#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

public enum GitHubOAuth {
    /// Client-ID der OAuth App „PR Radar“ (öffentlich, kein Geheimnis). In der OAuth App muss
    /// „Enable Device Flow“ an sein. Leer: Anmeldung ausgeschaltet, nur Token per Einstellungen.
    static let builtInClientID = ""

    /// `repo`: private Repos lesen sowie mergen und Auto-Merge schalten. Feiner geht es bei
    /// OAuth Apps nicht; wer weniger Rechte geben will, nimmt einen Fine-grained PAT.
    public static let scopes = "repo"

    /// `PR_RADAR_GITHUB_CLIENT_ID` (Xcode-Scheme, zum Ausprobieren), sonst die eingebaute; `nil`, wenn keine.
    public static var clientID: String? {
        let env = ProcessInfo.processInfo.environment["PR_RADAR_GITHUB_CLIENT_ID"]?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if let env, !env.isEmpty { return env }
        return builtInClientID.isEmpty ? nil : builtInClientID
    }
}

/// Antwort auf die Code-Anfrage: Den `userCode` gibt der Nutzer auf `verificationURI` ein.
public struct DeviceCode: Decodable, Sendable, Equatable {
    public var deviceCode: String
    public var userCode: String
    public var verificationURI: URL
    /// Gültigkeit des Codes in Sekunden (meist 900)
    public var expiresIn: TimeInterval
    /// Mindestabstand zwischen zwei Abfragen in Sekunden
    public var interval: TimeInterval

    public init(deviceCode: String, userCode: String, verificationURI: URL, expiresIn: TimeInterval, interval: TimeInterval = 5) {
        self.deviceCode = deviceCode
        self.userCode = userCode
        self.verificationURI = verificationURI
        self.expiresIn = expiresIn
        self.interval = interval
    }

    enum CodingKeys: String, CodingKey {
        case deviceCode = "device_code"
        case userCode = "user_code"
        case verificationURI = "verification_uri"
        case expiresIn = "expires_in"
        case interval
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        deviceCode = try c.decode(String.self, forKey: .deviceCode)
        userCode = try c.decode(String.self, forKey: .userCode)
        verificationURI = try c.decode(URL.self, forKey: .verificationURI)
        expiresIn = try c.decode(TimeInterval.self, forKey: .expiresIn)
        // Ohne Angabe gilt GitHubs Minimum von 5 s
        interval = try c.decodeIfPresent(TimeInterval.self, forKey: .interval) ?? 5
    }
}

/// Ergebnis einer Abfrage, ob der Nutzer schon bestätigt hat.
public enum DeviceFlowStep: Equatable, Sendable {
    case token(String)
    /// Noch nicht bestätigt, im selben Abstand weiter fragen.
    case pending
    /// Zu schnell gefragt, ab jetzt in diesem Abstand.
    case slowDown(TimeInterval)
}

public enum DeviceFlowError: LocalizedError, Equatable, Sendable {
    case notConfigured
    case expired
    case denied
    case disabled
    case badClient
    case rejected(String)
    case unreadable
    /// GitHub war wiederholt nicht erreichbar; die Anmeldung ist beendet.
    case unavailable

    public var errorDescription: String? {
        switch self {
        case .notConfigured: L10n.tr("Sign-in is not set up in this build (no client ID).")
        case .expired: L10n.tr("The sign-in code has expired. Please sign in again.")
        case .denied: L10n.tr("Sign-in was declined on GitHub.")
        case .disabled: L10n.tr("Sign-in with a code is not enabled for PR Radar on GitHub.")
        case .badClient: L10n.tr("GitHub does not know PR Radar’s client ID. Sign-in is misconfigured.")
        case let .rejected(detail): L10n.tr("GitHub rejects the sign-in: %@", detail)
        case .unreadable: L10n.tr("Unexpected response from GitHub during sign-in")
        case .unavailable: L10n.tr("GitHub can’t be reached right now. Please sign in again.")
        }
    }

    init(code: String, description: String?) {
        switch code {
        case "expired_token": self = .expired
        case "access_denied": self = .denied
        case "device_flow_disabled": self = .disabled
        case "incorrect_client_credentials": self = .badClient
        default:
            let detail = description?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            self = .rejected(detail.isEmpty ? code : detail)
        }
    }
}

private struct OAuthResponse: Decodable {
    var accessToken: String?
    var error: String?
    var errorDescription: String?
    var interval: TimeInterval?

    enum CodingKeys: String, CodingKey {
        case accessToken = "access_token"
        case error
        case errorDescription = "error_description"
        case interval
    }
}

/// Liest die Antwort auf die Code-Anfrage. GitHub meldet Fehler hier mit HTTP 200 und `error`.
func parseDeviceCode(_ data: Data) throws -> DeviceCode {
    if let res = try? JSONDecoder().decode(OAuthResponse.self, from: data), let code = res.error {
        throw DeviceFlowError(code: code, description: res.errorDescription)
    }
    guard let code = try? JSONDecoder().decode(DeviceCode.self, from: data) else {
        throw DeviceFlowError.unreadable
    }
    return code
}

/// Liest die Antwort auf eine Token-Abfrage. `current`: bisheriger Abstand, für `slow_down` ohne
/// neue Angabe (dann laut GitHub 5 s mehr).
func parseTokenResponse(_ data: Data, current: TimeInterval) throws -> DeviceFlowStep {
    guard let res = try? JSONDecoder().decode(OAuthResponse.self, from: data) else {
        throw DeviceFlowError.unreadable
    }
    if let token = res.accessToken?.trimmingCharacters(in: .whitespacesAndNewlines), !token.isEmpty {
        return .token(token)
    }
    switch res.error {
    case "authorization_pending"?: return .pending
    case "slow_down"?: return .slowDown(max(res.interval ?? current + 5, current))
    case let code?: throw DeviceFlowError(code: code, description: res.errorDescription)
    case nil: throw DeviceFlowError.unreadable
    }
}

private let deviceCodeURL = URL(string: "https://github.com/login/device/code")!
private let tokenURL = URL(string: "https://github.com/login/oauth/access_token")!
private let grantType = "urn:ietf:params:oauth:grant-type:device_code"

public struct DeviceFlow: Sendable {
    let clientID: String
    let transport: HTTPTransport
    let timing: Timing

    public init(clientID: String, transport: HTTPTransport = URLSessionTransport(), timing: Timing = .live) {
        self.clientID = clientID
        self.transport = transport
        self.timing = timing
    }

    /// Schritt 1: Code für die Anmeldung anfordern.
    public func start() async throws -> DeviceCode {
        do {
            return try parseDeviceCode(
                try await post(deviceCodeURL, ["client_id": clientID, "scope": GitHubOAuth.scopes]))
        } catch let error as GitHubError where error.isTransient {
            // „Neuer Versuch folgt automatisch“ stimmt hier nicht: Die Anmeldung ist beendet.
            throw DeviceFlowError.unavailable
        }
    }

    /// Schritt 2: Fragen, ob der Nutzer den Code bestätigt hat.
    public func poll(_ code: DeviceCode, interval: TimeInterval) async throws -> DeviceFlowStep {
        let data = try await post(
            tokenURL, ["client_id": clientID, "device_code": code.deviceCode, "grant_type": grantType])
        return try parseTokenResponse(data, current: interval)
    }

    /// Fragt im verlangten Abstand, bis der Nutzer bestätigt, ablehnt oder der Code abläuft.
    /// Vorübergehende Fehler (Netz, 408/5xx) beim Warten werden kurz toleriert.
    public func waitForToken(_ code: DeviceCode) async throws -> String {
        let deadline = timing.now().addingTimeInterval(code.expiresIn)
        var interval = max(code.interval, 1)
        var failures = 0
        while true {
            try await timing.sleep(interval)
            guard timing.now() < deadline else { throw DeviceFlowError.expired }
            do {
                switch try await poll(code, interval: interval) {
                case let .token(token): return token
                case .pending: failures = 0
                case let .slowDown(next): interval = next
                }
            } catch let error as GitHubError where error.isTransient {
                failures += 1
                if failures > 3 { throw DeviceFlowError.unavailable }
            }
        }
    }

    /// POST mit Formular-Body an github.com (nicht die API); liefert den JSON-Body.
    private func post(_ url: URL, _ form: KeyValuePairs<String, String>) async throws -> Data {
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        req.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        req.setValue("pr-radar-ios", forHTTPHeaderField: "User-Agent")
        req.httpBody = Data(formBody(form).utf8)
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
        // 408/429/5xx sind vorübergehend, auch wenn der Body ein `error` enthält.
        let http = GitHubError.http(status, gitHubMessage(data))
        if http.isTransient { throw http }
        // Fehler des Device Flows kommen als JSON mit `error`, auch bei HTTP 4xx.
        let hasOAuthError = (try? JSONDecoder().decode(OAuthResponse.self, from: data))?.error != nil
        guard (200..<300).contains(status) || hasOAuthError else {
            throw GitHubError.http(status, gitHubMessage(data))
        }
        return data
    }
}

extension GitHubError {
    /// Lohnt sich beim Anmelden ein neuer Versuch?
    var isTransient: Bool {
        switch self {
        case .unreachable: true
        case let .http(status, _): status == 408 || status == 429 || (500..<600).contains(status)
        default: false
        }
    }
}

/// `application/x-www-form-urlencoded` in fester Reihenfolge.
func formBody(_ pairs: KeyValuePairs<String, String>) -> String {
    pairs.map { "\($0.key)=\(encodePath($0.value))" }.joined(separator: "&")
}
