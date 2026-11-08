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

/// GitHub-Label in seiner eigenen Farbe – die Farbe ist hier Datum, keine Dekoration.
pub fn label_tag(label: &Label) -> Tag {
    let color = u32::from_str_radix(&label.color, 16)
        .map(rgb)
        .unwrap_or(rgb(0x888888));
    let color: Hsla = color.into();
    Tag::custom(color.opacity(0.14), color, color.opacity(0.4))
        .xsmall()
        .text_small()
        .rounded_full()
        .child(label.name.clone())
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
            .when(!self.collapsed, |this| {
                this.child(
                    h_flex()
                        .flex_shrink_0()
                        .px_2()
                        .h_8()
                        .text_heading()
                        .text_color(cx.theme().sidebar_foreground.opacity(0.7))
                        .child(self.label),
                )
            })
            // Gleiches ID-Schema wie `SidebarGroup` (`{gruppe}-{kind}`), damit Einträge
            // weiter unter `0-0-1` usw. erreichbar sind (UI-Tests).
            .child(
                self.menu
                    .collapsed(self.collapsed)
                    .render(format!("{id}-0"), window, cx),
            )
    }
}
