// Zeitangaben in der Sprache der App: relativ („5 min. ago“ / „vor 5 Min.“),
// Tageslabel („Today“ / „Heute“), Uhrzeit und Datum.

import Foundation

public enum TimeText {
    /// Sprache der Texte (en/de, wie der String Catalog sie auflöst) mit den Formaten des Systems.
    public static var locale: Locale { L10n.locale }

    public static func ago(_ t: Date?, now: Date = Date(), locale: Locale = TimeText.locale) -> String {
        guard let t else { return "–" }
        let s = max(0, Int(now.timeIntervalSince(t)))
        if s < 45 { return L10n.tr("just now") }
        #if canImport(Darwin)
            let f = RelativeDateTimeFormatter()
            f.locale = locale
            f.dateTimeStyle = .named
            f.unitsStyle = .short
            return f.localizedString(for: t, relativeTo: now)
        #else
            // RelativeDateTimeFormatter fehlt unter Linux (nur Tests): einfache englische Form.
            let (n, unit): (Int, String) =
                switch s {
                case ..<3600: ((s + 30) / 60, "min.")
                case ..<86_400: ((s + 1800) / 3600, "hr.")
                case ..<(2 * 86_400): (1, "day")
                case ..<(7 * 86_400): (s / 86_400, "days")
                case ..<(30 * 86_400): (s / (7 * 86_400), "wk.")
                case ..<(365 * 86_400): (s / (30 * 86_400), "mo.")
                default: (s / (365 * 86_400), "yr.")
                }
            return n == 1 && unit == "day" ? "yesterday" : "\(n) \(unit) ago"
        #endif
    }

    public static func dayLabel(
        _ t: Date, now: Date = Date(), calendar: Calendar = .current, locale: Locale = TimeText.locale
    ) -> String {
        if calendar.isDate(t, inSameDayAs: now) { return L10n.tr("Today") }
        if let yesterday = calendar.date(byAdding: .day, value: -1, to: now),
            calendar.isDate(t, inSameDayAs: yesterday)
        {
            return L10n.tr("Yesterday")
        }
        return formatter(calendar, locale) { $0.setLocalizedDateFormatFromTemplate("EEEEdMMMM") }.string(from: t)
    }

    public static func clock(_ t: Date, calendar: Calendar = .current, locale: Locale = TimeText.locale) -> String {
        formatter(calendar, locale) { $0.timeStyle = .short }.string(from: t)
    }

    public static func full(_ t: Date, calendar: Calendar = .current, locale: Locale = TimeText.locale) -> String {
        formatter(calendar, locale) {
            $0.dateStyle = .medium
            $0.timeStyle = .short
        }.string(from: t)
    }

    private static func formatter(
        _ calendar: Calendar, _ locale: Locale, _ configure: (DateFormatter) -> Void
    ) -> DateFormatter {
        let f = DateFormatter()
        f.calendar = calendar
        f.timeZone = calendar.timeZone
        f.locale = locale
        configure(f)
        return f
    }
}
