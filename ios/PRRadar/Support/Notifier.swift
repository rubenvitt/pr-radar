// Systembenachrichtigungen, wenn eine Pipeline rot/grün wird oder ein PR gemergt wurde.
// Im Vordergrund erscheinen sie als Banner, Tippen öffnet den PR auf GitHub.

import PRRadarCore
import UIKit
import UserNotifications

@MainActor
final class Notifier: NSObject, UNUserNotificationCenterDelegate {
    static let shared = Notifier()

    private var center: UNUserNotificationCenter { .current() }

    func activate() {
        center.delegate = self
    }

    /// Fragt einmalig nach Erlaubnis; `false`, wenn abgelehnt.
    /// nonisolated: Das Center ist nicht Sendable und darf nicht vom MainActor herübergereicht werden.
    nonisolated func requestAuthorization() async -> Bool {
        (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])) ?? false
    }

    func post(_ transitions: [PRTransition]) {
        for t in transitions {
            let content = UNMutableNotificationContent()
            let title: String
            switch t.kind {
            case .failed:
                title = "Pipeline rot"
            case .passed:
                title = "Pipeline grün"
            case .merged:
                title = "Gemergt"
            }
            content.title = "\(title) · \(t.pr.repo)#\(t.pr.number)"
            content.body = t.pr.title
            content.sound = t.kind == .failed ? .default : nil
            content.threadIdentifier = t.pr.repo
            content.userInfo = ["url": t.pr.url]
            let request = UNNotificationRequest(
                identifier: "\(t.pr.id)-\(t.kind.rawValue)", content: content, trigger: nil)
            center.add(request)
        }
    }

    /// Anzahl roter PRs als Badge am App-Symbol.
    func setBadge(_ count: Int) {
        center.setBadgeCount(count)
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        [.banner, .list, .sound]
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse
    ) async {
        guard let string = response.notification.request.content.userInfo["url"] as? String,
            let url = URL(string: string)
        else { return }
        await MainActor.run { UIApplication.shared.open(url) }
    }
}
