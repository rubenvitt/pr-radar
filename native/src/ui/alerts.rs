//! Systembenachrichtigungen (Pipeline rot/grün, gemergt) auf App-Ebene.
//!
//! Die App läuft nach ⌘W ohne Fenster weiter und pollt. Benachrichtigungen über
//! `window.push_notification(...)` bräuchten ein offenes Fenster, darum abonniert
//! dieses Modul den Radar direkt und spricht die Mitteilungszentrale selbst an.
//! Das Fenster zeigt weiter nur In-App-Toasts (`Workspace::on_radar_event`).
//!
//! Doppelte Meldungen vermeiden: Ist ein Fenster der App gerade aktiv, sieht man
//! den neuesten Toast, und dafür entfällt die Systembenachrichtigung. Sonst
//! (Fenster geschlossen, App im Hintergrund) kommt alles über das System.

use std::ops::Range;

use gpui_kit::*;

use crate::model::{Transition, TransitionKind};
use crate::radar::{Radar, RadarEvent};

/// Präfix unserer Benachrichtigungs-Tags. Danach folgen Art und URL des PR, damit
/// ein Klick den PR auch nach einem Neustart der App noch öffnen kann.
const TAG_PREFIX: &str = "pr-radar/transition/";

/// Text einer Benachrichtigung zu einem Übergang, für Toast und System gleich.
pub(super) struct Alert {
    pub title: String,
    pub body: String,
    /// Eindeutig je PR und Art: dieselbe Meldung ersetzt die vorige.
    pub tag: String,
    pub url: String,
}

impl Alert {
    /// Titel, Text, Tag und URL zu einem Übergang.
    pub fn new(t: &Transition) -> Self {
        let label = match t.kind {
            TransitionKind::Failed => "Pipeline rot",
            TransitionKind::Passed => "Pipeline grün",
            TransitionKind::Merged => "Gemergt",
        };
        Self {
            title: format!("{label} · {}#{}", t.pr.repo, t.pr.number),
            body: t.pr.title.clone(),
            tag: tag(t.kind, &t.pr.url),
            url: t.pr.url.clone(),
        }
    }
}

/// Tag der Systembenachrichtigung: Präfix, Art und URL des PR.
fn tag(kind: TransitionKind, url: &str) -> String {
    format!("{TAG_PREFIX}{kind:?}/{url}")
}

/// URL des PR aus einem Tag, `None` für fremde Tags.
fn url_from_tag(tag: &str) -> Option<&str> {
    let (_, url) = tag.strip_prefix(TAG_PREFIX)?.split_once('/')?;
    (!url.is_empty()).then_some(url)
}

/// Abonniert den Radar und übernimmt die Antworten auf Systembenachrichtigungen.
///
/// Muss nach `gpui_kit::init` laufen: GPUI kennt nur einen Antwort-Handler, und
/// dieser ersetzt den von gpui-component. Deshalb nutzt die App die
/// System-Zustellung von gpui-component (`.system()`, `.in_app_and_system()`)
/// nicht mehr, sondern schickt alle Systembenachrichtigungen von hier.
pub fn init(radar: Entity<Radar>, cx: &mut App) {
    cx.subscribe(&radar, |radar, event, cx| {
        let RadarEvent::Transitions(transitions) = event else {
            return;
        };
        if !radar.read(cx).prefs().notify {
            return;
        }
        for t in &transitions[for_system(transitions.len(), window_active(cx))] {
            let alert = Alert::new(t);
            cx.show_system_notification(SystemNotification {
                tag: alert.tag.into(),
                title: alert.title.into(),
                body: alert.body.into(),
                actions: Vec::new(),
            });
        }
    })
    .detach();

    cx.on_system_notification_response(|response, cx| {
        // Manche Plattformen behalten angeklickte Meldungen in der Mitteilungszentrale.
        cx.dismiss_system_notification(&response.tag);
        match url_from_tag(&response.tag) {
            // Nur den PR im Browser öffnen, PR Radar nicht nach vorn holen.
            Some(url) => cx.open_url(url),
            None => cx.activate(true),
        }
    });
}

/// Welche Übergänge (in Reihenfolge des Radars) als Systembenachrichtigung gehen.
///
/// Ohne aktives Fenster alle. Mit aktivem Fenster alle außer dem neuesten (dem
/// letzten): Der eingeklappte Toast-Stapel zeigt nur ihn ganz, ältere schauen nur
/// als Kante dahinter hervor und liefen sonst ungesehen ab. Klappt man den Stapel
/// auf, stehen sie doppelt da; ein Klick auf den Toast zieht die Systemmeldung zurück.
fn for_system(count: usize, window_active: bool) -> Range<usize> {
    if window_active {
        0..count.saturating_sub(1)
    } else {
        0..count
    }
}

/// Ob ein Fenster der App gerade den Fokus hat (dann reicht der Toast).
fn window_active(cx: &mut App) -> bool {
    cx.windows().into_iter().any(|handle| {
        handle
            .update(cx, |_, window, _| window.is_window_active())
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    // Kein `super::*`: `gpui_kit::*` brächte GPUIs eigenes `test`-Makro mit.
    use super::{for_system, tag, url_from_tag};
    use crate::model::TransitionKind;

    #[test]
    fn tag_traegt_die_url_und_ist_je_art_eindeutig() {
        let url = "https://github.com/acme/api/pull/42";
        let failed = tag(TransitionKind::Failed, url);
        let passed = tag(TransitionKind::Passed, url);
        assert_ne!(failed, passed);
        assert_eq!(url_from_tag(&failed), Some(url));
        assert_eq!(url_from_tag(&passed), Some(url));
    }

    #[test]
    fn fremde_tags_haben_keine_url() {
        assert_eq!(url_from_tag("gpui-component/notification/Id(1)"), None);
        assert_eq!(url_from_tag("pr-radar/transition/Failed"), None);
        assert_eq!(url_from_tag("pr-radar/transition/Failed/"), None);
    }

    #[test]
    fn ohne_aktives_fenster_geht_alles_ans_system() {
        assert_eq!(for_system(0, false), 0..0);
        assert_eq!(for_system(3, false), 0..3);
    }

    #[test]
    fn bei_aktivem_fenster_alles_ausser_dem_neuesten_toast() {
        assert_eq!(for_system(0, true), 0..0);
        assert_eq!(for_system(1, true), 0..0);
        // Nur der neueste (Index 2) ist im eingeklappten Stapel ganz zu sehen.
        assert_eq!(for_system(3, true), 0..2);
    }
}
