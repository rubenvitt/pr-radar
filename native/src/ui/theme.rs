//! Eigenes Farbschema für hell und dunkel (PRR-52).
//!
//! Grundlage ist das Standard-Theme von gpui-kit; `theme.json` überschreibt nur, was
//! PR Radar anders braucht:
//! - Akzent Blau statt Schwarz/Weiß (`primary`, Links, Fokus, Auswahl)
//! - Statusfarben (`success`, `warning`, `danger`) als Text-/Symbolfarben mit mindestens
//!   4,5:1 auf jeder Fläche; volle Flächen (Tags) bekommen eine passende Schrift
//! - „Gemergt“ in Lila (`base.magenta`, siehe [`merged`])
//! - Dunkel: Seitenleiste, Hintergrund, Karte und Hover in vier Stufen

use std::rc::Rc;

use gpui_kit::component::{ActiveTheme as _, Theme, ThemeRegistry, ThemeSet};
use gpui_kit::{App, Hsla};

const THEMES: &str = include_str!("theme.json");

/// Registriert das Schema und macht es zum Hell- und Dunkel-Theme. Danach
/// `Theme::sync_system_appearance` aufrufen, damit die passende Hälfte geladen wird.
pub fn init(cx: &mut App) {
    let set: ThemeSet = match serde_json::from_str(THEMES) {
        Ok(set) => set,
        Err(e) => {
            log::error!("Eigenes Theme nicht lesbar, Standard bleibt: {e}");
            return;
        }
    };
    if let Err(e) = ThemeRegistry::global_mut(cx).load_themes_from_str(THEMES) {
        log::warn!("Eigenes Theme nicht registriert: {e}");
    }
    let theme = Theme::global_mut(cx);
    for config in set.themes {
        if config.mode.is_dark() {
            theme.dark_theme = Rc::new(config);
        } else {
            theme.light_theme = Rc::new(config);
        }
    }
}

/// Farbe für „gemergt“ (GitHub-Konvention Lila), als Text und Symbol lesbar.
pub fn merged(cx: &App) -> Hsla {
    cx.theme().magenta
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::THEMES;

    fn channel(c: f64) -> f64 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    fn rgb(hex: &str) -> [f64; 3] {
        let hex = hex.trim_start_matches('#');
        assert!(hex.len() == 6, "nur deckende Farben vergleichen: #{hex}");
        let v = |i: usize| f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap()) / 255.0;
        [v(0), v(2), v(4)]
    }

    fn luminance([r, g, b]: [f64; 3]) -> f64 {
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    /// Kontrast nach WCAG 2.x.
    fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    /// `fg` mit Deckkraft `alpha` über `bg` – so malen Outline-Buttons ihre Fläche.
    fn tint(fg: [f64; 3], bg: [f64; 3], alpha: f64) -> [f64; 3] {
        [0, 1, 2].map(|i| fg[i] * alpha + bg[i] * (1.0 - alpha))
    }

    fn themes() -> Vec<(String, serde_json::Map<String, Value>)> {
        let set: Value = serde_json::from_str(THEMES).unwrap();
        set["themes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                (
                    t["name"].as_str().unwrap().to_string(),
                    t["colors"].as_object().unwrap().clone(),
                )
            })
            .collect()
    }

    fn color(colors: &serde_json::Map<String, Value>, key: &str) -> [f64; 3] {
        rgb(colors
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{key} fehlt")))
    }

    const SURFACES: [&str; 4] = [
        "background",
        "sidebar.background",
        "group_box.background",
        "list.hover.background",
    ];

    /// Statustext 4,5:1 auf jeder Fläche, auch auf der 10-%-Fläche eines Outline-Buttons.
    #[test]
    fn statusfarben_sind_als_text_lesbar() {
        for (name, colors) in themes() {
            for key in [
                "success.background",
                "warning.background",
                "danger.background",
                "primary.background",
                "base.magenta",
                "muted.foreground",
                "link.hover.foreground",
            ] {
                let fg = color(&colors, key);
                for surface in SURFACES {
                    let bg = color(&colors, surface);
                    let ratio = contrast(fg, bg);
                    assert!(ratio >= 4.5, "{name}: {key} auf {surface} nur {ratio:.2}:1");
                }
                for surface in ["background", "group_box.background"] {
                    let bg = tint(fg, color(&colors, surface), 0.1);
                    let ratio = contrast(fg, bg);
                    assert!(
                        ratio >= 4.5,
                        "{name}: {key} auf getönter {surface} nur {ratio:.2}:1"
                    );
                }
            }
        }
    }

    /// Volle Flächen (Tag „Approved“, „Pre-Release“, „Dein Review“) mit ihrer Schrift.
    #[test]
    fn volle_flaechen_tragen_ihre_schrift() {
        for (name, colors) in themes() {
            for tone in ["primary", "success", "warning", "danger", "info"] {
                let bg = color(&colors, &format!("{tone}.background"));
                let fg = color(&colors, &format!("{tone}.foreground"));
                let ratio = contrast(fg, bg);
                assert!(ratio >= 4.5, "{name}: {tone} nur {ratio:.2}:1");
            }
            let fg = color(&colors, "foreground");
            for surface in SURFACES {
                let ratio = contrast(fg, color(&colors, surface));
                assert!(ratio >= 7.0, "{name}: Text auf {surface} nur {ratio:.2}:1");
            }
        }
    }

    /// Kein Schwarz/Weiß mehr als Primärfarbe.
    #[test]
    fn primaerfarbe_ist_bunt() {
        for (name, colors) in themes() {
            let [r, g, b] = color(&colors, "primary.background");
            let spread = r.max(g).max(b) - r.min(g).min(b);
            assert!(spread > 0.3, "{name}: primary ist fast grau");
        }
    }

    /// Dunkel: Seitenleiste < Hintergrund < Karte < Hover, jede Stufe sichtbar.
    #[test]
    fn dunkle_flaechen_sind_gestaffelt() {
        let (_, colors) = themes()
            .into_iter()
            .find(|(n, _)| n.ends_with("Dark"))
            .unwrap();
        let steps = [
            "sidebar.background",
            "background",
            "group_box.background",
            "list.hover.background",
        ]
        .map(|k| luminance(color(&colors, k)));
        for pair in steps.windows(2) {
            let ratio = (pair[1] + 0.05) / (pair[0] + 0.05);
            assert!(ratio >= 1.1, "Stufe zu flach: {ratio:.3}");
        }
    }
}
