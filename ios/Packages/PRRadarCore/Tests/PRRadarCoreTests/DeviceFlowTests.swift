import Foundation
import Testing

#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

@testable import PRRadarCore

/// Antwortet der Reihe nach mit den vorgegebenen Bodies (HTTP 200, sonst `status`) und merkt sich die Anfragen.
private final class QueuedTransport: HTTPTransport, @unchecked Sendable {
    private let lock = NSLock()
    private var responses: [Result<String, Error>]
    private var statuses: [Int]
    private(set) var requests: [URLRequest] = []

    init(_ responses: [Result<String, Error>], statuses: [Int] = []) {
        self.responses = responses
        self.statuses = statuses
    }

    func send(_ request: URLRequest) async throws -> (Data, Int) {
        let (next, status) = lock.withLock {
            requests.append(request)
            let body = responses.isEmpty ? .success("{}") : responses.removeFirst()
            return (body, statuses.isEmpty ? 200 : statuses.removeFirst())
        }
        return (Data(try next.get().utf8), status)
    }
}

private struct Offline: Error {}

@Suite struct DeviceFlowTests {
    let code = DeviceCode(
        deviceCode: "dc", userCode: "WDJB-MJHT", verificationURI: URL(string: "https://github.com/login/device")!,
        expiresIn: 900, interval: 5)

    @Test func liestDenGeraetecode() throws {
        let body = #"{"device_code":"3584d8","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#
        let parsed = try parseDeviceCode(Data(body.utf8))
        #expect(parsed.userCode == "WDJB-MJHT")
        #expect(parsed.verificationURI.absoluteString == "https://github.com/login/device")
        #expect(parsed.expiresIn == 900)
        // Ohne Abstand gilt GitHubs Minimum von 5 s
        let noInterval = #"{"device_code":"d","user_code":"U","verification_uri":"https://x","expires_in":900}"#
        #expect(try parseDeviceCode(Data(noInterval.utf8)).interval == 5)
    }

    @Test func geraetecodeFehlerImKlartext() {
        #expect(throws: DeviceFlowError.disabled) {
            try parseDeviceCode(Data(#"{"error":"device_flow_disabled"}"#.utf8))
        }
        #expect(throws: DeviceFlowError.rejected("Bad client")) {
            try parseDeviceCode(Data(#"{"error":"unauthorized_client","error_description":"Bad client"}"#.utf8))
        }
        #expect(throws: DeviceFlowError.unreadable) { try parseDeviceCode(Data("<html>".utf8)) }
    }

    @Test func tokenAbfrageSchritte() throws {
        #expect(
            try parseTokenResponse(Data(#"{"access_token":"gho_abc","token_type":"bearer"}"#.utf8), current: 5)
                == .token("gho_abc"))
        #expect(try parseTokenResponse(Data(#"{"error":"authorization_pending"}"#.utf8), current: 5) == .pending)
        #expect(
            try parseTokenResponse(Data(#"{"error":"slow_down","interval":10}"#.utf8), current: 5) == .slowDown(10))
        #expect(try parseTokenResponse(Data(#"{"error":"slow_down"}"#.utf8), current: 10) == .slowDown(15))
        #expect(throws: DeviceFlowError.expired) {
            try parseTokenResponse(Data(#"{"error":"expired_token"}"#.utf8), current: 5)
        }
        #expect(throws: DeviceFlowError.denied) {
            try parseTokenResponse(Data(#"{"error":"access_denied"}"#.utf8), current: 5)
        }
        #expect(throws: DeviceFlowError.unreadable) { try parseTokenResponse(Data("{}".utf8), current: 5) }
    }

    @Test func wartetBisZurBestaetigung() async throws {
        let transport = QueuedTransport([
            .success(#"{"error":"authorization_pending"}"#),
            .success(#"{"error":"slow_down","interval":10}"#),
            .failure(Offline()),
            .success(#"{"access_token":"gho_abc"}"#),
        ])
        let clock = FakeClock()
        let flow = DeviceFlow(clientID: "Ov23li", transport: transport, timing: clock.timing)
        #expect(try await flow.waitForToken(code) == "gho_abc")
        #expect(clock.sleeps == [5, 5, 10, 10], "slow_down verlängert den Abstand")

        let request = try #require(transport.requests.first)
        #expect(request.url?.absoluteString == "https://github.com/login/oauth/access_token")
        #expect(request.value(forHTTPHeaderField: "Accept") == "application/json")
        let body = String(decoding: request.httpBody ?? Data(), as: UTF8.self)
        #expect(
            body
                == "client_id=Ov23li&device_code=dc&grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code")
    }

    @Test func serverfehlerBeimWartenWerdenToleriert() async throws {
        let transport = QueuedTransport(
            [
                .success("<html>Bad Gateway</html>"),
                .success(#"{"message":"Server Error"}"#),
                .success(#"{"access_token":"gho_abc"}"#),
            ], statuses: [502, 500, 200])
        let flow = DeviceFlow(clientID: "id", transport: transport, timing: FakeClock().timing)
        #expect(try await flow.waitForToken(code) == "gho_abc")
    }

    @Test func dauerhafteServerfehlerBrechenAb() async {
        let transport = QueuedTransport(Array(repeating: .success("<html>"), count: 10), statuses: Array(repeating: 503, count: 10))
        let flow = DeviceFlow(clientID: "id", transport: transport, timing: FakeClock().timing)
        await #expect(throws: GitHubError.self) { _ = try await flow.waitForToken(code) }
        #expect(transport.requests.count == 4, "drei Fehler toleriert, der vierte bricht ab")
    }

    @Test func brichtNachAblaufAb() async {
        let transport = QueuedTransport(Array(repeating: .success(#"{"error":"authorization_pending"}"#), count: 500))
        let clock = FakeClock()
        let short = DeviceCode(
            deviceCode: "dc", userCode: "U", verificationURI: URL(string: "https://x")!, expiresIn: 12, interval: 5)
        let flow = DeviceFlow(clientID: "id", transport: transport, timing: clock.timing)
        await #expect(throws: DeviceFlowError.expired) { _ = try await flow.waitForToken(short) }
    }

    @Test func fordertDenCodeMitScopeAn() async throws {
        let transport = QueuedTransport([
            .success(#"{"device_code":"dc","user_code":"U","verification_uri":"https://github.com/login/device","expires_in":900}"#)
        ])
        let flow = DeviceFlow(clientID: "Ov23li", transport: transport)
        #expect(try await flow.start().userCode == "U")
        let request = try #require(transport.requests.first)
        #expect(request.url?.absoluteString == "https://github.com/login/device/code")
        #expect(String(decoding: request.httpBody ?? Data(), as: UTF8.self) == "client_id=Ov23li&scope=repo")
    }
}

@Suite struct AccessErrorTests {
    private func errors(_ json: String) throws -> [GQLError] {
        try JSONDecoder().decode([GQLError].self, from: Data(json.utf8))
    }

    @Test func fehlendeSchreibrechteImKlartext() throws {
        let forbidden = try errors(#"[{"type":"FORBIDDEN","message":"Resource not accessible by personal access token"}]"#)
        let msg = describeMutationErrors(forbidden)
        #expect(msg.contains("Keine Schreibrechte"))
        #expect(!msg.contains("Resource not accessible"))

        let perms = try errors(
            #"[{"message":"octocat does not have the correct permissions to execute `MergePullRequest`"}]"#)
        #expect(describeMutationErrors(perms).contains("Keine Schreibrechte"))

        // Andere Fehler bleiben, wie sie sind – „Base branch was modified“ wird erneut versucht.
        let other = try errors(#"[{"message":"Base branch was modified."},{"message":"zweiter"}]"#)
        #expect(describeMutationErrors(other) == "Base branch was modified.; zweiter")
    }

    @Test func orgOhneFreigabeImKlartext() {
        let raw =
            "Although you appear to have the correct authorization credentials, the `acme` organization has enabled OAuth App access restrictions, meaning that data access to third-parties is limited."
        let msg = explainAccessError(raw, type: "FORBIDDEN")
        #expect(msg?.contains("„acme“") == true)
        #expect(msg?.contains("Third-party access") == true)
        #expect(explainAccessError("Could not resolve to a Repository", type: "NOT_FOUND") == nil)
        #expect(explainOAuthRestriction(raw) == msg)
    }

    @Test func lesefehlerSindKeineFehlendenSchreibrechte() {
        // Beim Abruf (Lesen) zählt nur die OAuth-Sperre der Org, sonst bleibt GitHubs Meldung.
        #expect(explainOAuthRestriction("Resource protected by organization SAML enforcement.") == nil)
        #expect(explainOAuthRestriction("Resource not accessible by personal access token") == nil)
    }
}
