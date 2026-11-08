// App-weiter Zustand neben `Radar`: Token-Status, Demo-Modus und Hinweis-Banner (Toasts).

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
    /// Der echte Radar mit Token, Konfiguration und Cache.
    let liveRadar: Radar
    /// Gesetzt, solange der Demo-Modus läuft: Beispieldaten ohne GitHub, nichts wird gespeichert.
    private(set) var demoRadar: Radar?
    private(set) var tokenSource: TokenSource?
    var toast: Toast?

    /// Startargument für Screenshots und UI-Tests: App startet direkt im Demo-Modus.
    static let demoLaunchArgument = "--demo"

    init() {
        let tokens = TokenStore.shared
        liveRadar = Radar(github: GitHubClient(token: { tokens.token }))
        tokenSource = tokens.source
        liveRadar.onEvent = { [weak self] event in self?.handle(event) }
        if ProcessInfo.processInfo.arguments.contains(Self.demoLaunchArgument) {
            startDemo()
        }
    }

    /// Der Radar, den die Oberfläche gerade zeigt.
    var radar: Radar { demoRadar ?? liveRadar }

    var isDemo: Bool { demoRadar != nil }

    var hasToken: Bool { tokenSource != nil }

    /// Beispieldaten zeigen, ohne Token und Repos – für App-Review und zum Ausprobieren.
    /// Ansichts-Einstellungen (Liste/Flow, gruppiert) kommen aus dem echten Radar.
    func startDemo() {
        guard demoRadar == nil else { return }
        liveRadar.stop()
        var prefs = liveRadar.prefs
        prefs.tab = .open
        prefs.quick = .all
        prefs.repoFilter = []
        prefs.notify = false
        let demo = Radar.demo(prefs: prefs)
        demo.onEvent = { [weak self] event in self?.handle(event) }
        demoRadar = demo
        demo.refresh()
        updateBadge()
    }

    /// Zurück zu den eigenen Daten, auf demselben Tab.
    func endDemo() {
        guard let demo = demoRadar else { return }
        demo.stop()
        demoRadar = nil
        let tab = demo.prefs.tab
        liveRadar.updatePrefs { $0.tab = tab }
        liveRadar.refresh()
        updateBadge()
    }

    func saveToken(_ token: String) throws {
        try TokenStore.shared.save(token)
        tokenSource = TokenStore.shared.source
        // Wer einen Token einträgt, will seine eigenen Daten sehen.
        endDemo()
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
