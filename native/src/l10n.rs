//! Sprache der Oberfläche: Englisch (Standard) oder Deutsch.
//!
//! Die Sprache folgt den bevorzugten Sprachen des Systems (inkl. „Sprache pro App“ in den
//! Systemeinstellungen); `PR_RADAR_LANG=de|en` überschreibt sie, etwa für `cargo run`.
//! Texte stehen direkt am Aufruf in beiden Sprachen, damit keine Übersetzung fehlen kann:
//!
//! ```ignore
//! tr!("Refresh", "Aktualisieren")              // &'static str
//! trf!("{n} failed", "{n} rot")                // String, Platzhalter wie bei format!
//! trf!("Merge {} PRs", "{} PRs mergen", count) // String mit Argumenten
//! ```

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    De,
}

static LANG: AtomicU8 = AtomicU8::new(Lang::En as u8);

#[cfg(test)]
thread_local! {
    /// Sprache nur für den laufenden Test-Thread (Tests laufen parallel).
    static OVERRIDE: std::cell::Cell<Option<Lang>> = const { std::cell::Cell::new(None) };
}

/// Aktuelle Sprache der Oberfläche.
pub fn lang() -> Lang {
    #[cfg(test)]
    if let Some(lang) = OVERRIDE.with(|o| o.get()) {
        return lang;
    }
    if LANG.load(Ordering::Relaxed) == Lang::De as u8 {
        Lang::De
    } else {
        Lang::En
    }
}

/// Wählt die Sprache beim Start: `PR_RADAR_LANG`, sonst die Systemsprachen.
pub fn init() {
    let lang = std::env::var("PR_RADAR_LANG")
        .ok()
        .and_then(|v| parse(&v))
        .unwrap_or_else(|| from_preferred(sys_locale::get_locales()));
    LANG.store(lang as u8, Ordering::Relaxed);
}

/// Erste unterstützte Sprache aus der Liste der bevorzugten Sprachen; sonst Englisch.
pub fn from_preferred<S: AsRef<str>>(preferred: impl IntoIterator<Item = S>) -> Lang {
    preferred
        .into_iter()
        .find_map(|tag| parse(tag.as_ref()))
        .unwrap_or(Lang::En)
}

/// „de“, „de-DE“, „de_CH.UTF-8“ → Deutsch; „en…“ → Englisch; alles andere: keine Angabe.
fn parse(tag: &str) -> Option<Lang> {
    let primary = tag
        .split(['-', '_', '.', '@'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match primary.as_str() {
        "de" => Some(Lang::De),
        "en" => Some(Lang::En),
        _ => None,
    }
}

/// Führt `f` im Test-Thread mit fester Sprache aus.
#[cfg(test)]
pub fn with_lang<T>(lang: Lang, f: impl FnOnce() -> T) -> T {
    let prev = OVERRIDE.with(|o| o.replace(Some(lang)));
    let out = f();
    OVERRIDE.with(|o| o.set(prev));
    out
}

/// Fester Text in der aktuellen Sprache, Englisch zuerst: `tr!("Refresh", "Aktualisieren")`.
/// Ergibt ein `&'static str`. Für Platzhalter `trf!` nehmen; `{` lässt `tr!` nicht zu.
macro_rules! tr {
    ($en:literal, $de:literal $(,)?) => {{
        const _: () = assert!(
            !$crate::l10n::has_brace($en) && !$crate::l10n::has_brace($de),
            "Platzhalter gehören in trf!"
        );
        match $crate::l10n::lang() {
            $crate::l10n::Lang::En => $en,
            $crate::l10n::Lang::De => $de,
        }
    }};
}

/// Text mit Platzhaltern wie bei `format!`, Englisch zuerst: `trf!("{n} failed", "{n} rot")`.
macro_rules! trf {
    ($en:literal, $de:literal $(, $args:expr)* $(,)?) => {
        match $crate::l10n::lang() {
            $crate::l10n::Lang::En => format!($en $(, $args)*),
            $crate::l10n::Lang::De => format!($de $(, $args)*),
        }
    };
}

#[doc(hidden)]
pub const fn has_brace(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' {
            return true;
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprache_aus_systemliste() {
        assert_eq!(from_preferred(["de-DE", "en-US"]), Lang::De);
        assert_eq!(from_preferred(["en-GB", "de-DE"]), Lang::En);
        assert_eq!(from_preferred(["fr-FR", "de_CH.UTF-8"]), Lang::De);
        assert_eq!(from_preferred(["fr-FR", "ja"]), Lang::En);
        assert_eq!(from_preferred(Vec::<String>::new()), Lang::En);
    }

    #[test]
    fn makros_folgen_der_sprache() {
        let n = 3;
        assert_eq!(with_lang(Lang::En, || tr!("Open", "Offen")), "Open");
        assert_eq!(with_lang(Lang::De, || tr!("Open", "Offen")), "Offen");
        assert_eq!(
            with_lang(Lang::En, || trf!("{n} failed", "{n} rot")),
            "3 failed"
        );
        assert_eq!(with_lang(Lang::De, || trf!("{} rot", "{} rot", n)), "3 rot");
    }
}
