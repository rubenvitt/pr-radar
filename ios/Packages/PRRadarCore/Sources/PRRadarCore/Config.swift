// Konfiguration und Ansichts-Einstellungen – als JSON wie in der macOS-App.

import Foundation

public enum RadarTab: String, Sendable, Codable, Hashable, CaseIterable {
    case open, merged, releases, settings
}

public enum Quick: String, Sendable, Codable, Hashable, CaseIterable, Identifiable {
    case all, running, failed, passed, auto, conflict, review, mine

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .all: "Alle"
        case .running: "Pipeline läuft"
        case .failed: "Rot"
        case .passed: "Grün"
        case .auto: "Auto-Merge"
        case .conflict: "Konflikt"
        case .review: "Dein Review"
        case .mine: "Von dir"
        }
    }

    /// SF Symbol
    public var symbol: String {
        switch self {
        case .all: "square.stack.3d.up"
        case .running: "hourglass"
        case .failed: "xmark.circle"
        case .passed: "checkmark.circle"
        case .auto: "bolt"
        case .conflict: "exclamationmark.triangle"
        case .review: "tray"
        case .mine: "person.crop.circle"
        }
    }

    public func matches(_ pr: OpenPR, viewer: String?) -> Bool {
        switch self {
        case .all: true
        case .running: pr.pipeline.state == .running
        case .failed: pr.pipeline.state == .failed
        case .passed: pr.pipeline.state == .passed
        case .auto: pr.autoMerge != nil
        case .conflict: pr.hasConflict
        case .review: pr.requestsReview(from: viewer)
        case .mine: pr.isBy(viewer)
        }
    }
}

public enum ViewMode: String, Sendable, Codable, Hashable {
    case list, flow
}

public struct Prefs: Sendable, Codable, Hashable {
    public var tab: RadarTab = .open
    public var quick: Quick = .all
    public var view: ViewMode = .list
    public var grouped: Bool = true
    public var repoFilter: [String] = []
    public var notify: Bool = false

    public init() {}

    enum CodingKeys: String, CodingKey {
        case tab, quick, view, grouped
        case repoFilter = "repo_filter"
        case notify
    }

    /// Fehlende oder unbekannte Felder fallen auf den Standard zurück.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = Prefs()
        tab = (try? c.decodeIfPresent(RadarTab.self, forKey: .tab)) ?? d.tab
        quick = (try? c.decodeIfPresent(Quick.self, forKey: .quick)) ?? d.quick
        view = (try? c.decodeIfPresent(ViewMode.self, forKey: .view)) ?? d.view
        grouped = (try? c.decodeIfPresent(Bool.self, forKey: .grouped)) ?? d.grouped
        repoFilter = (try? c.decodeIfPresent([String].self, forKey: .repoFilter)) ?? d.repoFilter
        notify = (try? c.decodeIfPresent(Bool.self, forKey: .notify)) ?? d.notify
    }
}

public struct Config: Sendable, Codable, Hashable {
    public var repos: [String] = []
    public var prefs = Prefs()

    public init(repos: [String] = [], prefs: Prefs = Prefs()) {
        self.repos = repos
        self.prefs = prefs
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        repos = (try? c.decodeIfPresent([String].self, forKey: .repos)) ?? []
        prefs = (try? c.decodeIfPresent(Prefs.self, forKey: .prefs)) ?? Prefs()
    }

    enum CodingKeys: String, CodingKey { case repos, prefs }
}

/// Liest und schreibt die Konfiguration als JSON-Datei.
public struct ConfigStore: Sendable {
    public let url: URL

    public init(url: URL) {
        self.url = url
    }

    /// `Application Support/pr-radar/config.json` der App.
    public static var standard: ConfigStore {
        let base =
            FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSTemporaryDirectory())
        return ConfigStore(url: base.appendingPathComponent("pr-radar/config.json"))
    }

    public func load() -> Config {
        guard let data = try? Data(contentsOf: url), let cfg = try? JSONDecoder().decode(Config.self, from: data)
        else { return Config() }
        return cfg
    }

    public func save(_ config: Config) throws {
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(config).write(to: url, options: .atomic)
    }
}

private func validPart(_ s: Substring) -> Bool {
    !s.isEmpty && s.allSatisfy { c in c.isASCII && (c.isLetter || c.isNumber || "_.-".contains(c)) }
}

/// Akzeptiert "owner/name", "https://github.com/owner/name(.git)(/...)" und "git@github.com:owner/name.git".
public func parseRepo(_ input: String) -> String? {
    var s = Substring(input.trimmingCharacters(in: .whitespacesAndNewlines))
    if s.hasPrefix("git@github.com:") { s = s.dropFirst("git@github.com:".count) }
    for prefix in ["https://www.github.com/", "http://www.github.com/", "https://github.com/", "http://github.com/"]
    where s.hasPrefix(prefix) {
        s = s.dropFirst(prefix.count)
        break
    }
    let parts = s.split(separator: "/", omittingEmptySubsequences: false)
    guard parts.count >= 2 else { return nil }
    let owner = parts[0]
    var name = parts[1]
    if name.hasSuffix(".git") { name = name.dropLast(4) }
    guard validPart(owner), validPart(name) else { return nil }
    return "\(owner)/\(name)"
}
