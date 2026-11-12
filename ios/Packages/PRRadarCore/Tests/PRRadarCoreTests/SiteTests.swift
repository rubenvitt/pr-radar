import Foundation
import Testing

@testable import PRRadarCore

@Suite struct SiteTests {
    @Test func urlsSindAbsolutUndEnden() {
        for page in Site.Page.allCases {
            for german in [false, true] {
                let url = Site.url(page, german: german)
                #expect(url.scheme == "https")
                #expect(url.absoluteString.hasSuffix("/"))
                #expect(!url.absoluteString.dropFirst("https://".count).contains("//"))
            }
        }
        #expect(Site.url(.privacy, german: true).absoluteString == "https://pr-radar.rubeen.dev/de/datenschutz/")
        #expect(Site.reportIssueURL.absoluteString == "https://github.com/rubenvitt/pr-radar/issues/new/choose")
    }

    /// Jeder Link der App zeigt auf eine Seite, die es in `site/public` gibt.
    @Test func jedeSeiteExistiert() {
        let root = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // PRRadarCoreTests
            .deletingLastPathComponent()  // Tests
            .deletingLastPathComponent()  // PRRadarCore
            .deletingLastPathComponent()  // Packages
            .deletingLastPathComponent()  // ios
            .deletingLastPathComponent()  // Repo
            .appendingPathComponent("site/public")
        for page in Site.Page.allCases {
            for german in [false, true] {
                let path = String(page.path(german: german).dropFirst())
                let file = root.appendingPathComponent(path).appendingPathComponent("index.html")
                #expect(FileManager.default.fileExists(atPath: file.path), "fehlt: \(file.path)")
            }
        }
    }
}
