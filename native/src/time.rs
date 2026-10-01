//! Deutsche Zeitangaben: relativ („vor 5 Min.“), Tageslabel („Heute“), Uhrzeit.

use chrono::{DateTime, Datelike, Local, Utc};

pub fn ago(t: Option<DateTime<Utc>>) -> String {
    let Some(t) = t else { return "–".into() };
    let secs = (Utc::now() - t).num_seconds().max(0);
    let (n, unit_one, unit_many) = match secs {
        s if s < 45 => return "gerade eben".into(),
        s if s < 3600 => ((s + 30) / 60, "Min.", "Min."),
        s if s < 86_400 => ((s + 1800) / 3600, "Std.", "Std."),
        s if s < 2 * 86_400 => return "gestern".into(),
        s if s < 7 * 86_400 => (s / 86_400, "Tag", "Tagen"),
        s if s < 30 * 86_400 => (s / (7 * 86_400), "Woche", "Wochen"),
        s if s < 365 * 86_400 => (s / (30 * 86_400), "Monat", "Monaten"),
        s => (s / (365 * 86_400), "Jahr", "Jahren"),
    };
    format!("vor {n} {}", if n == 1 { unit_one } else { unit_many })
}

const WEEKDAYS: [&str; 7] = [
    "Montag",
    "Dienstag",
    "Mittwoch",
    "Donnerstag",
    "Freitag",
    "Samstag",
    "Sonntag",
];
const MONTHS: [&str; 12] = [
    "Januar",
    "Februar",
    "März",
    "April",
    "Mai",
    "Juni",
    "Juli",
    "August",
    "September",
    "Oktober",
    "November",
    "Dezember",
];

pub fn day_label(t: DateTime<Utc>) -> String {
    let d = t.with_timezone(&Local).date_naive();
    let today = Local::now().date_naive();
    if d == today {
        return "Heute".into();
    }
    if Some(d) == today.pred_opt() {
        return "Gestern".into();
    }
    format!(
        "{}, {}. {}",
        WEEKDAYS[d.weekday().num_days_from_monday() as usize],
        d.day(),
        MONTHS[d.month0() as usize]
    )
}

pub fn clock(t: DateTime<Utc>) -> String {
    t.with_timezone(&Local).format("%H:%M").to_string()
}

pub fn full(t: DateTime<Utc>) -> String {
    t.with_timezone(&Local)
        .format("%d.%m.%Y, %H:%M")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn relative_angaben() {
        let now = Utc::now();
        assert_eq!(ago(None), "–");
        assert_eq!(ago(Some(now)), "gerade eben");
        assert_eq!(ago(Some(now - Duration::minutes(5))), "vor 5 Min.");
        assert_eq!(ago(Some(now - Duration::hours(3))), "vor 3 Std.");
        assert_eq!(ago(Some(now - Duration::hours(30))), "gestern");
        assert_eq!(
            ago(Some(now - Duration::days(1) - Duration::days(3))),
            "vor 4 Tagen"
        );
        assert_eq!(ago(Some(now - Duration::days(8))), "vor 1 Woche");
    }
}
