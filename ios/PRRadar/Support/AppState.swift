// App-weiter Zustand neben `Radar`: Token-Status und Hinweis-Banner (Toasts).

import Observation
import PRRadarCore
import SwiftUI

struct Toast: Identifiable, Equatable {
    enum Kind { case success, error, info }

    let id = UUID()
    var kind: Kind
    var title: String
    var message: String
}

@MainActor
@Observable
final class AppState {
    let radar: Radar
    private(set) var tokenSource: TokenSource?
    var toast: Toast?

    init() {
        let tokens = TokenStore.shared
        radar = Radar(github: GitHubClient(token: { tokens.token }))
        tokenSource = tokens.source
        radar.onEvent = { [weak self] event in self?.handle(event) }
    }

    var hasToken: Bool { tokenSource != nil }

    func saveToken(_ token: String) throws {
        try TokenStore.shared.save(token)
        tokenSource = TokenStore.shared.source
        radar.refresh()
    }

    func deleteToken() {
        TokenStore.shared.delete()
        tokenSource = TokenStore.shared.source
        radar.refresh()
    }

    func show(_ kind: Toast.Kind, _ title: String, _ message: String = "") {
        toast = Toast(kind: kind, title: title, message: message)
    }

    private func handle(_ event: RadarEvent) {
        switch event {
        case let .actionFailed(message):
            show(.error, "Aktion fehlgeschlagen", message)
        case let .mergeAllFinished(merged, autoMerge, failed):
            var parts: [String] = []
            if merged > 0 { parts.append("\(merged) gemergt") }
            if autoMerge > 0 { parts.append("\(autoMerge) per Auto-Merge") }
            if !failed.isEmpty { parts.append("\(failed.count) fehlgeschlagen") }
            let summary = parts.joined(separator: " · ")
            if failed.isEmpty {
                show(.success, "Alle mergen", summary)
            } else {
                show(.error, "Alle mergen", ([summary] + failed).joined(separator: "\n"))
            }
        case let .transitions(transitions):
            guard radar.prefs.notify else { return }
            Notifier.shared.post(transitions)
        }
        updateBadge()
    }

    /// Rote PRs als Badge – nur, wenn Benachrichtigungen an sind.
    func updateBadge() {
        let failed = radar.snapshot?.open.filter { $0.pipeline.state == .failed }.count ?? 0
        Notifier.shared.setBadge(radar.prefs.notify ? failed : 0)
    }
}
