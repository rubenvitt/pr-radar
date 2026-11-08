//! Systembenachrichtigungen (Pipeline rot/grün, gemergt) auf App-Ebene.
//!
//! Die App läuft nach ⌘W ohne Fenster weiter und pollt. Benachrichtigungen über
//! `window.push_notification(...)` bräuchten ein offenes Fenster, darum abonniert
//! dieses Modul den Radar direkt und spricht die Mitteilungszentrale selbst an.
//! Das Fenster zeigt weiter nur In-App-Toasts (`Workspace::on_radar_event`).
//!
//! Doppelte Meldungen vermeiden: Ist ein Fenster der App gerade aktiv, sieht man
//! den Toast, und die Systembenachrichtigung entfällt. Sonst (Fenster geschlossen,
//! App im Hintergrund) kommt sie über das System.

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
        if !radar.read(cx).prefs().notify || window_active(cx) {
            return;
        }
        for t in transitions {
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
    use super::{tag, url_from_tag};
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
}
