//! „Gemergt“ als Tages-Timeline und „Releases“ mit gerenderten Release Notes.

use std::collections::HashSet;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    tag::Tag,
    text::TextView,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::parts::{avatar, empty_state, meta_row, repo_name};
use super::{Derived, Workspace};
use crate::time::{ago, clock, day_label, full};

impl Workspace {
    pub(super) fn render_merged(&self, d: &Derived, cx: &mut Context<Self>) -> AnyElement {
        if d.merged.is_empty() {
            return empty_state(Lucide::GitMerge, "Noch nichts gemergt", None, cx)
                .into_any_element();
        }
        let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
        for &i in &d.merged {
            let label = day_label(d.snapshot.merged[i].merged_at);
            match groups.last_mut() {
                Some((l, list)) if *l == label => list.push(i),
                _ => groups.push((label, vec![i])),
            }
        }

        v_flex()
            .gap_6()
            .children(groups.into_iter().map(|(label, items)| {
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .px_1()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child(label),
                    )
                    .child(
                        v_flex()
                            .rounded(cx.theme().radius_lg)
                            .border_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().group_box)
                            .overflow_hidden()
                            .children(items.into_iter().map(|i| {
                                let m = &d.snapshot.merged[i];
                                let url = m.url.clone();
                                h_flex()
                                    .id(SharedString::from(format!("merged-{}", m.id)))
                                    .gap_3()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(cx.theme().border)
                                    .cursor_pointer()
                                    .hover(|s| s.bg(cx.theme().list_hover))
                                    .on_click(move |_, _, cx| cx.open_url(&url))
                                    .child(
                                        Icon::new(Lucide::GitMerge)
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
                                                    .gap_2()
                                                    .min_w_0()
                                                    .child(
                                                        div()
                                                            .font_medium()
                                                            .truncate()
                                                            .child(m.title.clone()),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .text_color(cx.theme().muted_foreground)
                                                            .child(format!("#{}", m.number)),
                                                    ),
                                            )
                                            .child(
                                                meta_row(cx)
                                                    .child(repo_name(&m.repo, cx))
                                                    .child(
                                                        h_flex()
                                                            .gap_1p5()
                                                            .child(avatar(m.author.as_ref()))
                                                            .child(
                                                                m.author
                                                                    .as_ref()
                                                                    .map_or("ghost".into(), |a| {
                                                                        a.login.clone()
                                                                    }),
                                                            ),
                                                    )
                                                    .when_some(
                                                        m.merged_by.clone().filter(|b| {
                                                            Some(b)
                                                                != m.author
                                                                    .as_ref()
                                                                    .map(|a| &a.login)
                                                        }),
                                                        |this, by| {
                                                            this.child(format!("gemergt von {by}"))
                                                        },
                                                    )
                                                    .child(
                                                        div()
                                                            .font_family(
                                                                cx.theme().mono_font_family.clone(),
                                                            )
                                                            .child(format!("→ {}", m.base_ref)),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        v_flex()
                                            .items_end()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(clock(m.merged_at))
                                            .child(ago(Some(m.merged_at))),
                                    )
                            })),
                    )
            }))
            .into_any_element()
    }

    pub(super) fn render_releases(&self, d: &Derived, cx: &mut Context<Self>) -> AnyElement {
        if d.releases.is_empty() {
            return empty_state(Lucide::Tag, "Keine Releases", None, cx).into_any_element();
        }
        // Neuestes Release je Repo standardmäßig aufgeklappt.
        let mut seen = HashSet::new();
        v_flex()
            .gap_3()
            .children(d.releases.iter().map(|&i| {
                let r = &d.snapshot.releases[i];
                let default_open = seen.insert(r.repo.clone());
                let open = default_open != self.toggled_releases.contains(&r.id);
                let has_body = !r.description_html.trim().is_empty();
                let id = r.id.clone();
                let url = r.url.clone();

                v_flex()
                    .rounded(cx.theme().radius_lg)
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().group_box)
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .id(SharedString::from(format!("release-{}", r.id)))
                            .gap_3()
                            .px_4()
                            .py_3()
                            .when(has_body, |this| {
                                this.hover(|s| s.bg(cx.theme().list_hover)).on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.toggled_releases.remove(&id) {
                                        this.toggled_releases.insert(id.clone());
                                    }
                                    cx.notify();
                                }))
                            })
                            .child(Icon::new(Lucide::Tag).small().text_color(cx.theme().primary))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1()
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .child(div().font_semibold().child(r.name.clone()))
                                            .when(r.name != r.tag_name, |this| {
                                                this.child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .font_family(cx.theme().mono_font_family.clone())
                                                        .child(r.tag_name.clone()),
                                                )
                                            })
                                            .when(r.is_latest, |this| this.child(Tag::success().xsmall().child("Latest")))
                                            .when(r.is_prerelease, |this| this.child(Tag::warning().xsmall().child("Pre-Release"))),
                                    )
                                    .child(
                                        meta_row(cx)
                                            .child(repo_name(&r.repo, cx))
                                            .when_some(r.author.clone(), |this, a| this.child(format!("von {a}")))
                                            .child(
                                                div()
                                                    .id(SharedString::from(format!("release-time-{}", r.id)))
                                                    .child(ago(r.published_at))
                                                    .when_some(r.published_at, |this, t| {
                                                        let text: SharedString = full(t).into();
                                                        this.tooltip(move |window, cx| {
                                                            gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx)
                                                        })
                                                    }),
                                            ),
                                    ),
                            )
                            .child(
                                div().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(
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
                                    Icon::new(if open { IconName::ChevronUp } else { IconName::ChevronDown })
                                        .small()
                                        .text_color(cx.theme().muted_foreground),
                                )
                            }),
                    )
                    .when(open && has_body, |this| {
                        this.child(
                            div()
                                .border_t_1()
                                .border_color(cx.theme().border)
                                .px_5()
                                .py_4()
                                .text_sm()
                                // descriptionHTML ist von GitHub gerendertes und bereinigtes HTML
                                .child(TextView::html(SharedString::from(format!("notes-{}", r.id)), r.description_html.clone())),
                        )
                    })
            }))
            .into_any_element()
    }
}
