// Einstieg: pollt im Vordergrund live, im Hintergrund per App-Refresh (so oft iOS es zulässt).

import BackgroundTasks
import PRRadarCore
import SwiftUI

let backgroundRefreshID = "email.rubeen.prradar.refresh"

@main
struct PRRadarApp: App {
    @State private var app = AppState()
    @Environment(\.scenePhase) private var scenePhase

    init() {
        Notifier.shared.activate()
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(app)
                .environment(app.radar)
        }
        .onChange(of: scenePhase) { _, phase in
            switch phase {
            case .active:
                app.radar.refresh()
            case .background:
                app.radar.stop()
                app.updateBadge()
                scheduleBackgroundRefresh()
            default:
                break
            }
        }
        .backgroundTask(.appRefresh(backgroundRefreshID)) { [app] in
            await refreshInBackground(app)
        }
    }
}

/// Ein Abruf im Hintergrund; Zustandswechsel lösen wie im Vordergrund Benachrichtigungen aus.
@MainActor
private func refreshInBackground(_ app: AppState) async {
    scheduleBackgroundRefresh()
    await app.radar.pollOnce()
    app.updateBadge()
}

/// Nächste Hintergrund-Aktualisierung anmelden – iOS entscheidet, wann sie wirklich läuft.
func scheduleBackgroundRefresh() {
    let request = BGAppRefreshTaskRequest(identifier: backgroundRefreshID)
    request.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)
    try? BGTaskScheduler.shared.submit(request)
}
