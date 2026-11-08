// App-weiter Zustand neben `Radar`: Token-Status, GitHub-Anmeldung, Demo-Modus und Hinweis-Banner (Toasts).

import Observation
import PRRadarCore
import SwiftUI
import UIKit

struct Toast: Identifiable, Equatable {
    enum Kind { case success, error, info }

    let id = UUID()
    var kind: Kind
    var title: String
    var message: String
}

/// Stand der GitHub-Anmeldung per Device Flow.
enum LoginState: Equatable {
    case idle
    /// Code wird bei GitHub angefordert.
    case starting
    /// Nutzer soll `code.userCode` auf `code.verificationURI` eingeben.
    case waiting(DeviceCode)
    case failed(String)
}

@MainActor
@Observable
final class AppState {
    /// Der echte Radar mit Token, Konfiguration und Cache.
    let liveRadar: Radar
    /// Gesetzt, solange der Demo-Modus läuft: Beispieldaten ohne GitHub, nichts wird gespeichert.
    private(set) var demoRadar: Radar?
    private(set) var tokenSource: TokenSource?
    private(set) var login: LoginState = .idle
    @ObservationIgnored private var loginTask: Task<Void, Never>?
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

    /// In diesem Build ist eine OAuth App für die Anmeldung hinterlegt.
    var canLogin: Bool { GitHubOAuth.clientID != nil }

    /// „Mit GitHub anmelden“: Code anfordern, Code in die Zwischenablage, GitHub im Browser
    /// öffnen und warten, bis der Nutzer bestätigt. Der Token landet im Schlüsselbund.
    func startLogin() {
        guard let clientID = GitHubOAuth.clientID else {
            login = .failed(DeviceFlowError.notConfigured.localizedDescription)
            return
        }
        loginTask?.cancel()
        login = .starting
        loginTask = Task {
            let flow = DeviceFlow(clientID: clientID)
            do {
                let code = try await flow.start()
                try Task.checkCancellation()
                login = .waiting(code)
                UIPasteboard.general.string = code.userCode
                _ = await UIApplication.shared.open(code.verificationURI)
                let token = try await flow.waitForToken(code)
                try Task.checkCancellation()
                try TokenStore.shared.save(token, fromLogin: true)
                tokenSource = TokenStore.shared.source
                login = .idle
                // Wer sich anmeldet, will seine eigenen Daten sehen.
                endDemo()
                radar.refresh()
                show(.success, String(localized: "Signed In"), String(localized: "PR Radar is connected to GitHub."))
            } catch {
                guard !Task.isCancelled, !(error is CancellationError) else { return }
                login = .failed(error.localizedDescription)
            }
        }
    }

    func cancelLogin() {
        loginTask?.cancel()
        loginTask = nil
        login = .idle
    }

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
        cancelLogin()
        try TokenStore.shared.save(token)
        tokenSource = TokenStore.shared.source
        // Wer einen Token einträgt, will seine eigenen Daten sehen.
        endDemo()
        radar.refresh()
    }

    func deleteToken() {
        cancelLogin()
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
            show(.error, String(localized: "Action Failed"), message)
        case let .mergeAllFinished(merged, autoMerge, failed):
            var parts: [String] = []
            if merged > 0 { parts.append(String(localized: "\(merged) merged")) }
            if autoMerge > 0 { parts.append(String(localized: "\(autoMerge) via Auto-merge")) }
            if !failed.isEmpty { parts.append(String(localized: "\(failed.count) failed")) }
            let summary = parts.joined(separator: " · ")
            let title = String(localized: "Merge All")
            if failed.isEmpty {
                show(.success, title, summary)
            } else {
                show(.error, title, ([summary] + failed).joined(separator: "\n"))
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
