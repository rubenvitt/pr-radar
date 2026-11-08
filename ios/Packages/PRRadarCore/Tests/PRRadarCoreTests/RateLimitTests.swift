import Foundation
import Testing

#if canImport(FoundationNetworking)
    import FoundationNetworking
#endif

@testable import PRRadarCore

/// Antwortet auf jede Anfrage mit festem Status, Body und Headern.
private struct FixedTransport: HTTPTransport {
    var status: Int
    var body: Data
    var headers: [String: String] = [:]

    func send(_ request: URLRequest) async throws -> (Data, Int) { (body, status) }

    func sendWithHeaders(_ request: URLRequest) async throws -> (Data, Int, [String: String]) {
        (body, status, headers)
    }
}

@Suite struct RateLimitTests {
    let now = Date(timeIntervalSince1970: 1_000)

    @Test func erkenntErschoepftesKontingent() {
        let headers = ["x-ratelimit-remaining": "0", "x-ratelimit-reset": "1600"]
        #expect(
            rateLimitError(status: 403, headers: headers, now: now)
                == .rateLimited(retryAt: Date(timeIntervalSince1970: 1_600)))
    }

    @Test func erkenntSekundaeresLimit() {
        #expect(
            rateLimitError(status: 403, headers: ["retry-after": "60"], now: now)
                == .rateLimited(retryAt: Date(timeIntervalSince1970: 1_060)))
        #expect(rateLimitError(status: 429, headers: [:], now: now) == .rateLimited(retryAt: nil))
    }

    @Test func normales403IstKeineDrosselung() {
        #expect(rateLimitError(status: 403, headers: ["x-ratelimit-remaining": "4000"], now: now) == nil)
        let denied = Data(#"{"message":"Resource not accessible by integration"}"#.utf8)
        #expect(rateLimitError(status: 403, headers: ["x-ratelimit-remaining": "4000"], body: denied, now: now) == nil)
        #expect(rateLimitError(status: 502, headers: [:], now: now) == nil)
    }

    @Test func sekundaeresLimitOhneRetryAfterIstDrosselungOhneZeit() {
        let headers = ["x-ratelimit-remaining": "4000"]
        let body = Data(
            #"{"message":"You have exceeded a secondary rate limit. Please wait a few minutes before you try again."}"#
                .utf8)
        #expect(rateLimitError(status: 403, headers: headers, body: body, now: now) == .rateLimited(retryAt: nil))
        // Ältere Formulierung
        let abuse = Data(#"{"message":"You have triggered an abuse detection mechanism."}"#.utf8)
        #expect(rateLimitError(status: 403, headers: headers, body: abuse, now: now) != nil)
        // Ein mitgeliefertes Retry-After hat Vorrang
        #expect(
            rateLimitError(status: 403, headers: ["retry-after": "120"], body: abuse, now: now)
                == .rateLimited(retryAt: Date(timeIntervalSince1970: 1_120)))
    }

    @Test func meldetDenChunkMitDerLaengstenWartezeit() {
        let errors: [Error] = [
            GitHubError.rateLimited(retryAt: now.addingTimeInterval(60)),
            GitHubError.unreachable("x"),
            GitHubError.rateLimited(retryAt: now.addingTimeInterval(900)),
            GitHubError.rateLimited(retryAt: nil),
        ]
        #expect(longestRateLimit(errors, now: now) == .rateLimited(retryAt: now.addingTimeInterval(900)))
        // Ohne Zeitangabe zählt die Standardwartezeit (120 s) – länger als 60 s
        let short: [Error] = [GitHubError.rateLimited(retryAt: now.addingTimeInterval(60)), GitHubError.rateLimited(retryAt: nil)]
        #expect(longestRateLimit(short, now: now) == .rateLimited(retryAt: nil))
        #expect(longestRateLimit([GitHubError.unauthorized], now: now) == nil)
    }

    @Test func httpFehlerOhneRohenBody() async {
        let html = FixedTransport(status: 502, body: Data("<!DOCTYPE html><html>Unicorn!</html>".utf8))
        let client = GitHubClient(transport: html, token: { "t" })
        do {
            _ = try await client.fetch(repos: ["a/b"])
            Issue.record("Fehler erwartet")
        } catch {
            let text = error.localizedDescription
            #expect(!text.contains("html"), "\(text)")
            #expect(text.contains("502"), "\(text)")
        }

        let denied = FixedTransport(
            status: 403, body: Data(#"{"message":"Resource not accessible by integration"}"#.utf8))
        await #expect(throws: GitHubError.http(403, "Resource not accessible by integration")) {
            _ = try await GitHubClient(transport: denied, token: { "t" }).fetch(repos: ["a/b"])
        }
        #expect(describeHTTPError(403, message: "x").contains("Rechte"))
    }

    @Test func drosselungUeberHeaderUndGraphQL() async {
        let limited = FixedTransport(
            status: 403, body: Data("{}".utf8),
            headers: ["x-ratelimit-remaining": "0", "x-ratelimit-reset": "4102444800"])
        await #expect(throws: GitHubError.rateLimited(retryAt: Date(timeIntervalSince1970: 4_102_444_800))) {
            _ = try await GitHubClient(transport: limited, token: { "t" }).fetch(repos: ["a/b"])
        }

        let gql = FixedTransport(
            status: 200,
            body: Data(#"{"errors":[{"type":"RATE_LIMITED","message":"API rate limit exceeded"}]}"#.utf8))
        await #expect(throws: GitHubError.rateLimited(retryAt: nil)) {
            _ = try await GitHubClient(transport: gql, token: { "t" }).fetch(repos: ["a/b"])
        }

        // RATE_LIMITED mit HTTP 200: Reset-Zeit aus den Headern übernehmen
        let gqlWithReset = FixedTransport(
            status: 200,
            body: Data(#"{"errors":[{"type":"RATE_LIMITED","message":"API rate limit exceeded"}]}"#.utf8),
            headers: ["x-ratelimit-remaining": "0", "x-ratelimit-reset": "4102444800"])
        await #expect(throws: GitHubError.rateLimited(retryAt: Date(timeIntervalSince1970: 4_102_444_800))) {
            _ = try await GitHubClient(transport: gqlWithReset, token: { "t" }).fetch(repos: ["a/b"])
        }

        // Sekundäres Limit über den Body
        let secondary = FixedTransport(
            status: 403, body: Data(#"{"message":"You have exceeded a secondary rate limit."}"#.utf8),
            headers: ["x-ratelimit-remaining": "4000"])
        do {
            _ = try await GitHubClient(transport: secondary, token: { "t" }).fetch(repos: ["a/b"])
            Issue.record("Drosselung erwartet")
        } catch GitHubError.rateLimited(retryAt: nil) {
            // erwartet: ohne Zeitangabe, der Radar wartet mit wachsendem Backoff
        } catch {
            Issue.record("Unerwarteter Fehler: \(error)")
        }
    }

    @Test func backoffSteigtBisFuenfMinuten() {
        let error = GitHubError.unreachable("offline")
        let waits = (1...6).map { waitAfterError(error, failures: $0, base: activeInterval, now: now) }
        #expect(waits == [30, 60, 120, 240, 300, 300])
        #expect(waitAfterError(error, failures: 1000, base: normalInterval, now: now) == maxBackoff)
    }

    @Test func drosselungWartetBisZumReset() {
        let reset = GitHubError.rateLimited(retryAt: now.addingTimeInterval(600))
        #expect(waitAfterError(reset, failures: 1, base: normalInterval, now: now) == 605)
        #expect(waitAfterError(reset, failures: 1, base: normalInterval, now: now.addingTimeInterval(5_000)) == normalInterval)
        #expect(waitAfterError(GitHubError.rateLimited(retryAt: nil), failures: 1, base: normalInterval, now: now) == unknownRateLimitWait)
        // Wiederholte Drosselung ohne Zeitangabe: Pause verdoppelt sich bis 1 h
        let unknown = (1...7).map {
            waitAfterError(GitHubError.rateLimited(retryAt: nil), failures: $0, base: normalInterval, now: now)
        }
        #expect(unknown == [120, 240, 480, 960, 1920, 3600, 3600])
        let far = GitHubError.rateLimited(retryAt: now.addingTimeInterval(100_000))
        #expect(waitAfterError(far, failures: 1, base: normalInterval, now: now) == maxRateLimitWait)
    }

    @Test func leeresKontingentNachErfolgPausiertBisReset() {
        let empty = RateLimit(remaining: 0, limit: 5000, resetAt: now.addingTimeInterval(120))
        #expect(waitAfterSuccess(empty, base: activeInterval, now: now) == 125)
        let some = RateLimit(remaining: 10, limit: 5000, resetAt: now.addingTimeInterval(120))
        #expect(waitAfterSuccess(some, base: activeInterval, now: now) == activeInterval)
        #expect(waitAfterSuccess(nil, base: normalInterval, now: now) == normalInterval)
    }

    @Test func intervallText() {
        #expect(intervalLabel(30) == "30 s")
        #expect(intervalLabel(300) == "5 Min.")
        #expect(intervalLabel(605) == "11 Min.")
    }
}
