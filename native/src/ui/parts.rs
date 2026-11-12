//! Kleine, wiederverwendete Darstellungsbausteine.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::sidebar::{SidebarItem, SidebarMenu};
use gpui_kit::component::{
    ActiveTheme as _, Collapsible, Icon, Sizable as _, avatar::Avatar, h_flex, tag::Tag, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::typography::TypeScale as _;
use crate::model::{Label, Person, PipelineState};

/// Statusfarbe einer Pipeline – Bedeutung, nie Dekoration.
pub fn state_color(state: PipelineState, cx: &App) -> Hsla {
    match state {
        PipelineState::Passed => cx.theme().success,
        PipelineState::Failed => cx.theme().danger,
        PipelineState::Running => cx.theme().warning,
        PipelineState::None => cx.theme().muted_foreground,
    }
}

/// Pipeline-Symbol: Form und Farbe tragen den Zustand gemeinsam (nicht nur Farbe).
pub fn pipeline_icon(state: PipelineState, cx: &App) -> AnyElement {
    let color = state_color(state, cx);
    match state {
        // Bewusst statisch: Dauer-Spinner pro Zeile zwingen GPUI zu einem Neuaufbau je Frame.
        PipelineState::Running => Icon::new(Lucide::Hourglass)
            .small()
            .text_color(color)
            .into_any_element(),
        PipelineState::Passed => Icon::new(Lucide::CircleCheck)
            .small()
            .text_color(color)
            .into_any_element(),
        PipelineState::Failed => Icon::new(Lucide::CircleX)
            .small()
            .text_color(color)
            .into_any_element(),
        PipelineState::None => Icon::new(Lucide::CircleDashed)
            .small()
            .text_color(color)
            .into_any_element(),
    }
}

/// `owner/name` mit gedämpftem Owner.
pub fn repo_name(full: &str, cx: &App) -> Div {
    let (owner, name) = full.split_once('/').unwrap_or(("", full));
    h_flex()
        .min_w_0()
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{owner}/")),
        )
        .child(div().truncate().child(name.to_string()))
}

pub fn avatar(person: Option<&Person>) -> Avatar {
    let avatar = Avatar::new().xsmall();
    match person {
        Some(p) if !p.avatar_url.is_empty() => avatar
            .src(format!("{}&s=48", p.avatar_url))
            .name(p.login.clone()),
        Some(p) => avatar.name(p.login.clone()),
        None => avatar.name("ghost"),
    }
}

// Badges in der PR-Liste: genau drei Stile (PRR-55).
// 1. `status_badge`: Symbol in Zustandsfarbe, Text sekundär, keine Fläche (Review-Zustand, Draft).
// 2. `accent_badge`: getönte Fläche, nur für das, was Handeln verlangt („Konflikt“, „Dein Review“).
// 3. `label_tag`: GitHub-Label, rund, Farbe je Theme auf 4,5:1 gebracht.

/// Zustand als Symbol + Text in Sekundärfarbe; nur das Symbol trägt Farbe.
pub fn status_badge(icon: Icon, text: impl Into<SharedString>, color: Hsla, cx: &App) -> Div {
    h_flex()
        .flex_shrink_0()
        .gap_1()
        .text_small()
        .text_color(cx.theme().muted_foreground)
        .child(icon.xsmall().text_color(color))
        .child(text.into())
}

/// Hervorgehobener Zustand: getönte Fläche, Schrift und Symbol in `color`. Statusfarben
/// erreichen auf der 10-%-Tönung 4,5:1 (geprüft in `theme.rs`).
pub fn accent_badge(icon: Icon, text: impl Into<SharedString>, color: Hsla) -> Tag {
    Tag::custom(color.opacity(0.1), color, color.opacity(0.35))
        .xsmall()
        .text_small()
        .child(h_flex().gap_1().child(icon.xsmall()).child(text.into()))
}

/// GitHub-Label in seiner eigenen Farbe – die Farbe ist hier Datum, keine Dekoration. Der
/// Farbton bleibt, die Helligkeit der Schrift wird je Theme so weit verschoben, dass sie auf
/// der Label-Fläche 4,5:1 erreicht.
pub fn label_tag(label: &Label, cx: &App) -> Tag {
    let color: Hsla = u32::from_str_radix(&label.color, 16)
        .map(rgb)
        .unwrap_or(rgb(0x888888))
        .into();
    let (bg, fg) = label_colors(color, cx.theme().group_box, cx.theme().is_dark());
    Tag::custom(bg, fg, fg.opacity(0.3))
        .xsmall()
        .text_small()
        .rounded_full()
        .child(label.name.clone())
}

/// Mindestkontrast für Label-Schrift (WCAG AA, normaler Text).
const LABEL_CONTRAST: f32 = 4.5;
/// Anteil der Label-Farbe an der Label-Fläche.
const LABEL_TINT: f32 = 0.14;

/// Fläche (deckend: Label-Farbe zu 14 % über `surface`) und Schrift eines Labels. Die Schrift
/// behält Farbton und Sättigung; im Dunkelmodus wird sie aufgehellt, im Hellmodus abgedunkelt,
/// bis sie [`LABEL_CONTRAST`] erreicht.
pub fn label_colors(color: Hsla, surface: Hsla, dark: bool) -> (Hsla, Hsla) {
    let (c, s) = (color.to_rgb(), surface.to_rgb());
    let mix = |a: f32, b: f32| a * LABEL_TINT + b * (1.0 - LABEL_TINT);
    let bg: Hsla = Rgba {
        r: mix(c.r, s.r),
        g: mix(c.g, s.g),
        b: mix(c.b, s.b),
        a: 1.0,
    }
    .into();
    let step = if dark { 0.02 } else { -0.02 };
    let mut fg = Hsla { a: 1.0, ..color };
    // Höchstens 50 Schritte: danach ist l bei 0 bzw. 1, also Schwarz oder Weiß.
    for _ in 0..50 {
        if contrast(fg, bg) >= LABEL_CONTRAST {
            break;
        }
        fg.l = (fg.l + step).clamp(0.0, 1.0);
    }
    (bg, fg)
}

/// Kontrast nach WCAG 2.x zwischen zwei deckenden Farben.
pub fn contrast(a: Hsla, b: Hsla) -> f32 {
    fn channel(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    fn luminance(c: Hsla) -> f32 {
        let c = c.to_rgb();
        0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
    }
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Leerer Zustand: Symbol, Titel, ein Satz, warum hier nichts steht, und eine Aktion,
/// die weiterhilft. Das Symbol trägt den Akzent, sofern nicht ein Zustand (z. B. Erfolg)
/// eine eigene Farbe braucht.
#[derive(IntoElement)]
pub struct EmptyState {
    icon: Icon,
    title: SharedString,
    hint: Option<SharedString>,
    action: Option<AnyElement>,
    color: Option<Hsla>,
}

impl EmptyState {
    pub fn new(icon: impl Into<Icon>, title: impl Into<SharedString>) -> Self {
        Self {
            icon: icon.into(),
            title: title.into(),
            hint: None,
            action: None,
            color: None,
        }
    }

    /// Ein Satz, warum der Bereich leer ist.
    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Was man jetzt tun kann (meist ein Button).
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    /// Farbe des Symbols statt des Akzents.
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = self.color.unwrap_or(cx.theme().primary);
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .px_6()
            .py_16()
            .text_color(cx.theme().muted_foreground)
            .child(self.icon.with_size(px(32.)).text_color(color).mb_1())
            .child(
                div()
                    .text_color(cx.theme().foreground)
                    .font_medium()
                    .child(self.title),
            )
            .when_some(self.hint, |this, hint| {
                this.child(
                    div()
                        .max_w(rems(28.))
                        .text_small()
                        .text_center()
                        .child(hint),
                )
            })
            .when_some(self.action, |this, action| {
                this.child(div().mt_2().child(action))
            })
    }
}

/// Schmale Metadatenzeile; bricht in engen Fenstern um.
pub fn meta_row(cx: &App) -> Div {
    h_flex()
        .flex_wrap()
        .gap_x_2()
        .gap_y_0p5()
        .text_small()
        .text_color(cx.theme().muted_foreground)
        .min_w_0()
}

/// Ausschnitt einer Karte: In der virtualisierten Liste ist jede Zeile ein eigener Eintrag,
/// die Zeilen einer Gruppe ergeben zusammen eine Karte mit runden Ecken.
pub fn card_segment(first: bool, last: bool, cx: &App) -> Div {
    let r = cx.theme().radius_lg;
    v_flex()
        .bg(cx.theme().group_box)
        .border_color(cx.theme().border)
        .border_l_1()
        .border_r_1()
        .border_b_1()
        .overflow_hidden()
        .when(first, |d| d.border_t_1().rounded_tl(r).rounded_tr(r))
        .when(last, |d| d.rounded_bl(r).rounded_br(r))
}

/// Zeilenraster der Seitenleiste wie in macOS-Apps (PRR-54): 28 px von Zeile zu Zeile.
/// gpui-component setzt die Zeile selbst fest auf `h_7` (1,75 rem ≈ 22,75 px bei 13 px
/// Grundschrift); der Rest ist Abstand, gerechnet mit der rem-Größe des Fensters. Die Hervorhebung bleibt so wie bei macOS etwas
/// niedriger als das Raster.
const SIDEBAR_ROW: Pixels = px(28.);
fn sidebar_row_gap(window: &Window) -> Pixels {
    SIDEBAR_ROW - window.rem_size() * 1.75
}
/// Luft vor jedem Gruppenkopf, damit „Filter“ nicht an „Releases“ klebt.
const SIDEBAR_SECTION_GAP: Pixels = px(12.);

/// Abschnitt der Seitenleiste mit Kopf in der Stufe „Gruppenkopf“. Ersetzt `SidebarGroup`,
/// dessen Kopf fest `text_xs` setzt (bei 13 px Grundschrift 9,75 px).
#[derive(Clone)]
pub struct SidebarSection {
    label: SharedString,
    menu: SidebarMenu,
    collapsed: bool,
}

impl SidebarSection {
    pub fn new(label: impl Into<SharedString>, menu: SidebarMenu) -> Self {
        Self {
            label: label.into(),
            menu,
            collapsed: false,
        }
    }
}

impl Collapsible for SidebarSection {
    fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    fn is_collapsed(&self) -> bool {
        self.collapsed
    }
}

impl SidebarItem for SidebarSection {
    fn render(
        self,
        id: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let id = id.into();
        v_flex()
            .relative()
            .pb(SIDEBAR_SECTION_GAP)
            .when(!self.collapsed, |this| {
                this.child(
                    h_flex()
                        .flex_shrink_0()
                        .px_2()
                        .h(px(24.))
                        .items_end()
                        .pb_1()
                        .text_heading()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.label),
                )
            })
            // Gleiches ID-Schema wie `SidebarGroup` (`{gruppe}-{kind}`), damit Einträge
            // weiter unter `0-0-1` usw. erreichbar sind (UI-Tests).
            .child(
                self.menu
                    .gap(sidebar_row_gap(window))
                    .collapsed(self.collapsed)
                    .render(format!("{id}-0"), window, cx),
            )
    }
}

#[cfg(test)]
mod tests {
    // Kein `use super::*`: Der Glob brächte `gpui::test` mit, das `#[test]` überdeckt.
    use super::{LABEL_CONTRAST, contrast, label_colors};
    use gpui_kit::{Hsla, rgb};

    /// GitHub-Standardlabels und typische Eigenfarben, darunter die Problemfälle aus dem
    /// Audit: dunkles Blau/Lila (dunkel), Gelb (hell).
    const LABELS: [u32; 18] = [
        0xd73a4a, 0x0075ca, 0xcfd3d7, 0xa2eeef, 0x7057ff, 0x008672, 0xe4e669, 0xd876e3, 0xffffff,
        0x000000, 0xfbca04, 0x0e8a16, 0x1d76db, 0x5319e7, 0xb60205, 0x0052cc, 0xfef2c0, 0x5e2ca5,
    ];

    /// Kartenflächen aus `theme.json` (hell, dunkel).
    const SURFACES: [(u32, bool); 2] = [(0xffffff, false), (0x2a2a2e, true)];

    #[test]
    fn labels_sind_in_beiden_themes_lesbar() {
        for (surface, dark) in SURFACES {
            for label in LABELS {
                let (bg, fg) = label_colors(rgb(label).into(), rgb(surface).into(), dark);
                let ratio = contrast(fg, bg);
                assert!(
                    ratio >= LABEL_CONTRAST,
                    "#{label:06x} auf #{surface:06x}: nur {ratio:.2}:1"
                );
            }
        }
    }

    #[test]
    fn lesbare_labels_behalten_ihre_farbe() {
        // Kräftiges Rot ist hell schon lesbar und bleibt unverändert.
        let red: Hsla = rgb(0xb60205).into();
        let (_, fg) = label_colors(red, rgb(0xffffff).into(), false);
        assert!((fg.l - red.l).abs() < f32::EPSILON);
        assert!((fg.h - red.h).abs() < f32::EPSILON);
    }

    #[test]
    fn label_flaeche_ist_deckend() {
        let (bg, _) = label_colors(rgb(0x7057ff).into(), rgb(0x2a2a2e).into(), true);
        assert!((bg.a - 1.0).abs() < f32::EPSILON);
    }
}
