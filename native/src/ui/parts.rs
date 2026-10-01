//! Kleine, wiederverwendete Darstellungsbausteine.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, avatar::Avatar, h_flex, tag::Tag, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

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
        .rounded_full()
        .child(label.name.clone())
}

/// Leerer Zustand mit Symbol, Titel und optionalem Hinweis.
pub fn empty_state(
    icon: impl Into<Icon>,
    title: impl Into<SharedString>,
    hint: Option<SharedString>,
    cx: &App,
) -> Div {
    v_flex()
        .flex_1()
        .items_center()
        .justify_center()
        .gap_2()
        .py_16()
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(icon).large())
        .child(
            div()
                .text_color(cx.theme().foreground)
                .font_medium()
                .child(title.into()),
        )
        .when_some(hint, |this, hint| this.child(div().text_sm().child(hint)))
}

/// Schmale Metadatenzeile: Teile mit „·“ getrennt.
pub fn meta_row(cx: &App) -> Div {
    h_flex()
        .gap_2()
        .text_xs()
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
