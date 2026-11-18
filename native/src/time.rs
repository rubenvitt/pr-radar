//! Zeitangaben in der Sprache der Oberfläche: relativ („5 min ago“ / „vor 5 Min.“),
//! Tageslabel („Today“ / „Heute“), Uhrzeit.

use jiff::{Timestamp, ZonedDateTime, tz::TimeZone};

use crate::l10n::{Lang, lang};

pub fn ago(t: Option<Timestamp>) -> String {
    let Some(t) = t else { return "–".into() };
    let secs = Timestamp::now().duration_since(t).as_secs().max(0);
    let de = lang() == Lang::De;
    // (Anzahl, Einheit Englisch Einzahl/Mehrzahl, Einheit Deutsch Einzahl/Mehrzahl)
    let (n, en_unit, de_unit) = match secs {
        s if s < 45 => return tr!("just now", "gerade eben").into(),
        s if s < 3600 => ((s + 30) / 60, ("min", "min"), ("Min.", "Min.")),
        s if s < 86_400 => ((s + 1800) / 3600, ("hr", "hr"), ("Std.", "Std.")),
        s if s < 2 * 86_400 => return tr!("yesterday", "gestern").into(),
        s if s < 7 * 86_400 => (s / 86_400, ("day", "days"), ("Tag", "Tagen")),
        s if s < 30 * 86_400 => (s / (7 * 86_400), ("week", "weeks"), ("Woche", "Wochen")),
        s if s < 365 * 86_400 => (s / (30 * 86_400), ("month", "months"), ("Monat", "Monaten")),
        s => (s / (365 * 86_400), ("year", "years"), ("Jahr", "Jahren")),
    };
    let pick = |(one, many): (&'static str, &'static str)| if n == 1 { one } else { many };
    if de {
        format!("vor {n} {}", pick(de_unit))
    } else {
        format!("{n} {} ago", pick(en_unit))
    }
}

const WEEKDAYS_EN: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const WEEKDAYS_DE: [&str; 7] = [
    "Montag",
    "Dienstag",
    "Mittwoch",
    "Donnerstag",
    "Freitag",
    "Samstag",
    "Sonntag",
];
const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const MONTHS_DE: [&str; 12] = [
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

pub fn day_label(t: Timestamp) -> String {
    let d = t.to_zoned_datetime(TimeZone::system()).date();
    let today = ZonedDateTime::now().date();
    if d == today {
        return tr!("Today", "Heute").into();
    }
    if today.yesterday().ok() == Some(d) {
        return tr!("Yesterday", "Gestern").into();
    }
    let weekday = d.weekday().to_monday_zero_offset() as usize;
    let month = (d.month() - 1) as usize;
    match lang() {
        Lang::En => format!("{}, {} {}", WEEKDAYS_EN[weekday], MONTHS_EN[month], d.day()),
        Lang::De => format!(
            "{}, {}. {}",
            WEEKDAYS_DE[weekday],
            d.day(),
            MONTHS_DE[month]
        ),
    }
}

pub fn clock(t: Timestamp) -> String {
    t.to_zoned_datetime(TimeZone::system())
        .strftime("%H:%M")
        .to_string()
}

pub fn full(t: Timestamp) -> String {
    let format = match lang() {
        Lang::En => "%b %-d, %Y, %H:%M",
        Lang::De => "%d.%m.%Y, %H:%M",
    };
    t.to_zoned_datetime(TimeZone::system())
        .strftime(format)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::l10n::with_lang;
    use jiff::SignedDuration as Duration;

    #[test]
    fn relative_angaben_englisch() {
        let now = Timestamp::now();
        with_lang(Lang::En, || {
            assert_eq!(ago(None), "–");
            assert_eq!(ago(Some(now)), "just now");
            assert_eq!(ago(Some(now - Duration::from_mins(5))), "5 min ago");
            assert_eq!(ago(Some(now - Duration::from_hours(3))), "3 hr ago");
            assert_eq!(ago(Some(now - Duration::from_hours(30))), "yesterday");
            assert_eq!(ago(Some(now - Duration::from_hours(24 * 4))), "4 days ago");
            assert_eq!(ago(Some(now - Duration::from_hours(24 * 8))), "1 week ago");
            assert_eq!(
                ago(Some(now - Duration::from_hours(24 * 400))),
                "1 year ago"
            );
        });
    }

    #[test]
    fn relative_angaben_deutsch() {
        let now = Timestamp::now();
        with_lang(Lang::De, || {
            assert_eq!(ago(None), "–");
            assert_eq!(ago(Some(now)), "gerade eben");
            assert_eq!(ago(Some(now - Duration::from_mins(5))), "vor 5 Min.");
            assert_eq!(ago(Some(now - Duration::from_hours(3))), "vor 3 Std.");
            assert_eq!(ago(Some(now - Duration::from_hours(30))), "gestern");
            assert_eq!(
                ago(Some(
                    now - Duration::from_hours(24) - Duration::from_hours(24 * 3)
                )),
                "vor 4 Tagen"
            );
            assert_eq!(ago(Some(now - Duration::from_hours(24 * 8))), "vor 1 Woche");
        });
    }

    #[test]
    fn tageslabel_in_beiden_sprachen() {
        let now = Timestamp::now();
        assert_eq!(with_lang(Lang::En, || day_label(now)), "Today");
        assert_eq!(with_lang(Lang::De, || day_label(now)), "Heute");
        // Mitten im Tag, damit Zeitzonen das Datum nicht verschieben.
        let t = jiff::civil::date(2026, 10, 13)
            .at(12, 0, 0, 0)
            .to_zoned_datetime(TimeZone::system())
            .unwrap()
            .timestamp();
        assert_eq!(with_lang(Lang::En, || day_label(t)), "Tuesday, October 13");
        assert_eq!(
            with_lang(Lang::De, || day_label(t)),
            "Dienstag, 13. Oktober"
        );
        assert_eq!(with_lang(Lang::En, || full(t)), "Oct 13, 2026, 12:00");
        assert_eq!(with_lang(Lang::De, || full(t)), "13.10.2026, 12:00");
    }
}
