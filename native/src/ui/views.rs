//! Einträge für „Gemergt“ (Tages-Timeline) und „Releases“ (mit gerenderten Release Notes).

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    tag::Tag,
    text::{TextView, TextViewStyle},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Workspace;
use super::parts::{avatar, card_segment, meta_row, repo_name};
use super::theme;
use super::typography::TypeScale as _;
use crate::model::{MergedPr, Release};
use crate::time::{ago, clock, full};

/// Höhe, auf die aufgeklappte Release Notes gekappt werden, bis „Mehr anzeigen“ geklickt ist.
const NOTES_MAX_HEIGHT: f32 = 16.;
/// Höhe des Verlaufs, der gekappte Notes nach unten ausblendet.
const NOTES_FADE_HEIGHT: f32 = 2.5;

/// Stil der Release Notes: Überschriften nie größer als der Release-Name (Grundschrift,
/// halbfett), Absätze und Listen enger als im Standard (1 rem).
fn notes_style(cx: &App) -> TextViewStyle {
    let name = cx.theme().font_size;
    TextViewStyle::default()
        .paragraph_gap(rems(0.5))
        .heading_font_size(move |level, _| match level {
            1 | 2 => name * 0.9375,
            _ => name * 0.875,
        })
}

impl Workspace {
    pub(super) fn render_merged_row(
        &self,
        m: &MergedPr,
        first: bool,
        last: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let url = m.url.clone();
        let muted = cx.theme().muted_foreground;
        card_segment(first, last, cx)
            .child(
                h_flex()
                    .id(SharedString::from(format!("merged-{}", m.id)))
                    .gap_3()
                    .px_4()
                    .py_3()
                    .cursor_pointer()
                    .hover(|s| s.bg(cx.theme().list_hover))
                    .on_click(move |_, _, cx| cx.open_url(&url))
                    .child(
                        Icon::new(Lucide::GitMerge)
                            .small()
                            .text_color(theme::merged(cx)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .min_w_0()
                                    .child(div().font_medium().truncate().child(m.title.clone()))
                                    .child(
                                        div()
                                            .text_small()
                                            .text_color(muted)
                                            .child(format!("#{}", m.number)),
                                    ),
                            )
                            .child(
                                meta_row(cx)
                                    .child(repo_name(&m.repo, cx))
                                    .child(
                                        h_flex().gap_1p5().child(avatar(m.author.as_ref())).child(
                                            m.author
                                                .as_ref()
                                                .map_or("ghost".into(), |a| a.login.clone()),
                                        ),
                                    )
                                    .when_some(
                                        m.merged_by.clone().filter(|b| {
                                            Some(b) != m.author.as_ref().map(|a| &a.login)
                                        }),
                                        |this, by| this.child(format!("gemergt von {by}")),
                                    )
                                    .child(
                                        div()
                                            .font_family(cx.theme().mono_font_family.clone())
                                            .child(format!("→ {}", m.base_ref)),
                                    ),
                            ),
                    )
                    .child(
                        v_flex()
                            .items_end()
                            .text_small()
                            .text_color(muted)
                            .child(clock(m.merged_at))
                            .child(ago(Some(m.merged_at))),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_release(
        &self,
        r: &Release,
        default_open: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = default_open != self.toggled_releases.contains(&r.id);
        let has_body = !r.description_html.trim().is_empty();
        let id = r.id.clone();
        let url = r.url.clone();
        let muted = cx.theme().muted_foreground;

        card_segment(true, true, cx)
            .child(
                h_flex()
                    .id(SharedString::from(format!("release-{}", r.id)))
                    .gap_3()
                    .px_4()
                    .py_3()
                    .when(has_body, |this| {
                        this.hover(|s| s.bg(cx.theme().list_hover))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.toggled_releases.remove(&id) {
                                    this.toggled_releases.insert(id.clone());
                                }
                                this.remeasure_key(&format!("release:{id}"));
                                cx.notify();
                            }))
                    })
                    .child(
                        Icon::new(Lucide::Tag)
                            .small()
                            .text_color(cx.theme().primary),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                h_flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .child(div().font_semibold().child(r.name.clone()))
                                    .when(r.name != r.tag_name, |this| {
                                        this.child(
                                            div()
                                                .text_small()
                                                .text_color(muted)
                                                .font_family(cx.theme().mono_font_family.clone())
                                                .child(r.tag_name.clone()),
                                        )
                                    })
                                    .when(r.is_latest, |this| {
                                        this.child(
                                            Tag::success().xsmall().text_small().child("Latest"),
                                        )
                                    })
                                    .when(r.is_prerelease, |this| {
                                        this.child(
                                            Tag::warning()
                                                .xsmall()
                                                .text_small()
                                                .child("Pre-Release"),
                                        )
                                    }),
                            )
                            .child(
                                meta_row(cx)
                                    .child(repo_name(&r.repo, cx))
                                    .when_some(r.author.clone(), |this, a| {
                                        this.child(format!("von {a}"))
                                    })
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "release-time-{}",
                                                r.id
                                            )))
                                            .child(ago(r.published_at))
                                            .when_some(r.published_at, |this, t| {
                                                let text: SharedString = full(t).into();
                                                this.tooltip(move |window, cx| {
                                                    gpui_kit::component::tooltip::Tooltip::new(
                                                        text.clone(),
                                                    )
                                                    .build(window, cx)
                                                })
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(
                                Button::new(SharedString::from(format!("release-open-{}", r.id)))
                                    .ghost()
                                    .small()
                                    .icon(IconName::ExternalLink)
                                    .tooltip("Auf GitHub öffnen")
                                    .on_click(move |_, _, cx| cx.open_url(&url)),
                            ),
                    )
                    .when(has_body, |this| {
                        this.child(
                            Icon::new(if open {
                                IconName::ChevronUp
                            } else {
                                IconName::ChevronDown
                            })
                            .small()
                            .text_color(muted),
                        )
                    }),
            )
            .when(open && has_body, |this| {
                this.child(self.render_notes(r, cx))
            })
            .into_any_element()
    }

    /// Aufgeklappte Release Notes, gekappt auf [`NOTES_MAX_HEIGHT`] mit „Mehr anzeigen“.
    ///
    /// Ob die Notes über die Kappung hinausgehen, hängt von der Fensterbreite ab und lässt sich
    /// erst nach dem Layout sagen: Die volle Höhe des Inhalts wird beim Zeichnen gemessen und
    /// in `long_notes` gemerkt; erst dann erscheinen Verlauf und Knopf.
    fn render_notes(&self, r: &Release, cx: &mut Context<Self>) -> Div {
        let full = self.full_notes.contains(&r.id);
        let long = self.long_notes.contains(&r.id);
        let clipped = long && !full;
        let card = cx.theme().group_box;
        let workspace = cx.entity().downgrade();
        let id = r.id.clone();

        let clip = div()
            .relative()
            .when(!full, |d| d.max_h(rems(NOTES_MAX_HEIGHT)).overflow_hidden())
            .on_children_prepainted(move |bounds, window, cx| {
                let Some(content) = bounds.first() else {
                    return;
                };
                let is_long =
                    content.size.height > rems(NOTES_MAX_HEIGHT).to_pixels(window.rem_size());
                if is_long == long {
                    return;
                }
                let (workspace, id) = (workspace.clone(), id.clone());
                // Nicht mitten im Zeichnen ändern: erst danach, dann neu messen und zeichnen.
                cx.defer(move |cx| {
                    workspace
                        .update(cx, |this, cx| {
                            let changed = if is_long {
                                this.long_notes.insert(id.clone())
                            } else {
                                this.long_notes.remove(&id)
                            };
                            if changed {
                                this.remeasure_key(&format!("release:{id}"));
                                cx.notify();
                            }
                        })
                        .ok();
                });
            })
            .child(
                div().flex_shrink_0().child(
                    // descriptionHTML ist von GitHub gerendertes und bereinigtes HTML
                    TextView::html(
                        SharedString::from(format!("notes-{}", r.id)),
                        r.description_html.clone(),
                    )
                    .style(notes_style(cx)),
                ),
            )
            .when(clipped, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(rems(NOTES_FADE_HEIGHT))
                        .bg(linear_gradient(
                            180.,
                            linear_color_stop(card.opacity(0.), 0.),
                            linear_color_stop(card, 1.),
                        )),
                )
            });

        let id = r.id.clone();
        v_flex()
            .border_t_1()
            .border_color(cx.theme().border)
            .px_5()
            .py_4()
            .gap_2()
            .text_body()
            .child(clip)
            .when(long, |this| {
                this.child(
                    div().child(
                        Button::new(SharedString::from(format!("release-more-{}", r.id)))
                            .ghost()
                            .xsmall()
                            .label(if full {
                                "Weniger anzeigen"
                            } else {
                                "Mehr anzeigen"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.full_notes.remove(&id) {
                                    this.full_notes.insert(id.clone());
                                }
                                this.remeasure_key(&format!("release:{id}"));
                                cx.notify();
                            })),
                    ),
                )
            })
    }
}
