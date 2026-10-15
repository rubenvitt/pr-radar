// Deutsche Zeitangaben: relativ („vor 5 Min.“), Tageslabel („Heute“), Uhrzeit.

import Foundation

public enum GermanTime {
    public static func ago(_ t: Date?, now: Date = Date()) -> String {
        guard let t else { return "–" }
        let s = max(0, Int(now.timeIntervalSince(t)))
        let n: Int
        let one: String
        let many: String
        switch s {
        case ..<45: return "gerade eben"
        case ..<3600: (n, one, many) = ((s + 30) / 60, "Min.", "Min.")
        case ..<86_400: (n, one, many) = ((s + 1800) / 3600, "Std.", "Std.")
        case ..<(2 * 86_400): return "gestern"
        case ..<(7 * 86_400): (n, one, many) = (s / 86_400, "Tag", "Tagen")
        case ..<(30 * 86_400): (n, one, many) = (s / (7 * 86_400), "Woche", "Wochen")
        case ..<(365 * 86_400): (n, one, many) = (s / (30 * 86_400), "Monat", "Monaten")
        default: (n, one, many) = (s / (365 * 86_400), "Jahr", "Jahren")
        }
        return "vor \(n) \(n == 1 ? one : many)"
    }

    private static let weekdays = ["Sonntag", "Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag"]
    private static let months = [
        "Januar", "Februar", "März", "April", "Mai", "Juni",
        "Juli", "August", "September", "Oktober", "November", "Dezember",
    ]

    public static func dayLabel(_ t: Date, now: Date = Date(), calendar: Calendar = .current) -> String {
        if calendar.isDate(t, inSameDayAs: now) { return "Heute" }
        if let yesterday = calendar.date(byAdding: .day, value: -1, to: now),
            calendar.isDate(t, inSameDayAs: yesterday)
        {
            return "Gestern"
        }
        let c = calendar.dateComponents([.weekday, .day, .month], from: t)
        return "\(weekdays[(c.weekday ?? 1) - 1]), \(c.day ?? 1). \(months[(c.month ?? 1) - 1])"
    }

    public static func clock(_ t: Date, calendar: Calendar = .current) -> String {
        let c = calendar.dateComponents([.hour, .minute], from: t)
        return String(format: "%02d:%02d", c.hour ?? 0, c.minute ?? 0)
    }

    public static func full(_ t: Date, calendar: Calendar = .current) -> String {
        let c = calendar.dateComponents([.year, .month, .day, .hour, .minute], from: t)
        return String(
            format: "%02d.%02d.%04d, %02d:%02d", c.day ?? 0, c.month ?? 0, c.year ?? 0, c.hour ?? 0, c.minute ?? 0)
    }
}
