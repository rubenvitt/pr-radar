//! Bewegung, die Zustandswechsel erklärt: Einblenden, Aufleuchten, Puls, fließende Kanten.
//! Alles entfällt, wenn macOS „Bewegung reduzieren“ meldet.

use std::time::Duration;

use gpui_kit::component::v_flex;
use gpui_kit::*;

use super::Workspace;

/// Dauer des Aufleuchtens geänderter Zeilen
pub const FLASH: Duration = Duration::from_millis(1600);

pub fn enabled(cx: &App) -> bool {
    !cx.reduce_motion()
}

/// Inhalt weich einblenden und leicht nach oben gleiten lassen (Tab-/Ansichtswechsel, Aufklappen).
pub fn enter(id: impl Into<ElementId>, child: impl IntoElement, animate: bool) -> AnyElement {
    let el = v_flex().size_full().child(child);
    if !animate {
        return el.into_any_element();
    }
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(220)).with_easing(ease_out_quint()),
        |el, t| el.opacity(t).mt(rems(0.5 * (1. - t))),
    )
    .into_any_element()
}

/// Herzschlag: ein Ring breitet sich einmal um den Punkt aus, wenn `beat` wechselt
/// (z. B. bei jedem erfolgreichen Poll). Einmalig statt Endlosschleife – die würde
/// jeden Frame die ganze Ansicht neu aufbauen.
pub fn heartbeat(dot: Div, color: Hsla, beat: Option<i64>, cx: &App) -> AnyElement {
    let base = 0.375; // Punktgröße in rem (size_1p5)
    let dot = div().relative().size(rems(base)).child(dot.size_full());
    match beat.filter(|_| enabled(cx)) {
        None => dot.into_any_element(),
        Some(beat) => dot
            .child(
                div()
                    .absolute()
                    .rounded_full()
                    .border_1()
                    .border_color(color)
                    .with_animation(
                        SharedString::from(format!("heartbeat-{beat}")),
                        Animation::new(Duration::from_millis(1100)).with_easing(ease_out_quint()),
                        move |ring, t| {
                            let size = base + 1.0 * t;
                            let offset = -(size - base) / 2.;
                            ring.size(rems(size))
                                .top(rems(offset))
                                .left(rems(offset))
                                .opacity(0.9 * (1. - t))
                        },
                    ),
            )
            .into_any_element(),
    }
}

impl Workspace {
    /// Überlagerung, die eine gerade geänderte Zeile kurz in ihrer Statusfarbe aufleuchten lässt.
    pub(super) fn flash(&self, pr_id: &str, color: Hsla, cx: &App) -> Option<AnyElement> {
        let epoch = *self.flashes.get(pr_id)?;
        if !enabled(cx) {
            return None;
        }
        Some(
            div()
                .absolute()
                .inset_0()
                .bg(color.opacity(0.22))
                .with_animation(
                    SharedString::from(format!("flash-{pr_id}-{epoch}")),
                    Animation::new(FLASH).with_easing(ease_in_out),
                    |el, t| el.opacity(1. - t),
                )
                .into_any_element(),
        )
    }
}

/// Punkt auf einer kubischen Bezier-Kurve.
pub fn bezier(
    p0: Point<Pixels>,
    p1: Point<Pixels>,
    p2: Point<Pixels>,
    p3: Point<Pixels>,
    t: f32,
) -> Point<Pixels> {
    let u = 1. - t;
    let (a, b, c, d) = (u * u * u, 3. * u * u * t, 3. * u * t * t, t * t * t);
    point(
        p0.x * a + p1.x * b + p2.x * c + p3.x * d,
        p0.y * a + p1.y * b + p2.y * c + p3.y * d,
    )
}
