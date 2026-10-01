//! Listenansicht offener PRs: Zeile mit Status, Metadaten, Merge-Steuerung und aufklappbaren Checks.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Workspace;
use super::parts::{avatar, label_tag, meta_row, pipeline_icon, repo_name, state_color};
use crate::model::{CheckState, MergeMethod, OpenPr, ReviewDecision, Snapshot};
use crate::time::ago;

/// Was der Merge-Button gerade tun kann – eine Quelle für Label, Zustand und Erklärung.
pub enum MergeAction {
    /// Auto-Merge ist an; Klick schaltet es aus.
    Disable {
        method: MergeMethod,
        by: Option<String>,
    },
    /// PR ist sofort mergebar.
    MergeNow(Vec<MergeMethod>),
    /// Auto-Merge kann aktiviert werden.
    Enable(Vec<MergeMethod>),
    /// Nicht möglich – mit Grund.
    Unavailable(&'static str),
}

impl MergeAction {
    pub fn of(pr: &OpenPr, snapshot: &Snapshot) -> Self {
        if let Some(am) = &pr.auto_merge {
            return Self::Disable {
                method: am.method,
                by: am.enabled_by.clone(),
            };
        }
        let Some(repo) = snapshot.repo(&pr.repo) else {
            return Self::Unavailable("Repository nicht geladen");
        };
        let methods = pr.merge_methods.clone();
        if methods.is_empty() {
            Self::Unavailable("Keine Merge-Methode erlaubt (Repo-Einstellungen und Rulesets)")
        } else if !repo.viewer_can_merge {
            Self::Unavailable("Keine Schreibrechte")
        } else if pr.is_draft {
            Self::Unavailable("Draft-PRs können nicht gemergt werden")
        } else if pr.ready_to_merge() {
            Self::MergeNow(methods)
        } else if !repo.auto_merge_allowed {
            Self::Unavailable(
                "Auto-Merge ist in diesem Repo nicht erlaubt (Settings → Allow auto-merge)",
            )
        } else {
            Self::Enable(methods)
        }
    }
}

impl Workspace {
    /// Karte mit PR-Zeilen (Liste) oder Flow-Knoten.
    pub(super) fn render_pr_card(
        &self,
        prs: &[&OpenPr],
        snapshot: &Snapshot,
        show_repo: bool,
        flow: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows: Vec<AnyElement> = prs
            .iter()
            .map(|pr| {
                if flow {
                    self.render_flow_row(pr, snapshot, show_repo, cx)
                        .into_any_element()
                } else {
                    self.render_pr_row(pr, snapshot, show_repo, cx)
                        .into_any_element()
                }
            })
            .collect();
        let card = v_flex()
            .rounded(cx.theme().radius_lg)
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().group_box)
            .overflow_hidden()
            .children(rows);
        if !flow {
            return card.into_any_element();
        }
        // Der Flow braucht Mindestbreite; in schmalen Fenstern scrollt nur er horizontal.
        div()
            .id(SharedString::from(format!(
                "flow-scroll-{}",
                prs.first().map_or("", |p| p.id.as_str())
            )))
            .w_full()
            .overflow_x_scroll()
            .child(card.min_w(rems(58.)))
            .into_any_element()
    }

    fn render_pr_row(
        &self,
        pr: &OpenPr,
        snapshot: &Snapshot,
        show_repo: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let open = self.expanded.contains(&pr.id);
        let viewer = snapshot.viewer.as_deref();
        let conflict = pr.has_conflict();
        let p = &pr.pipeline;
        let id = pr.id.clone();
        let url = pr.url.clone();

        let title = h_flex()
            .gap_2()
            .min_w_0()
            .flex_wrap()
            .when(show_repo, |this| {
                this.child(div().text_xs().child(repo_name(&pr.repo, cx)))
            })
            .child(
                div()
                    .id(SharedString::from(format!("title-{}", pr.id)))
                    .font_medium()
                    .truncate()
                    .cursor_pointer()
                    .when(pr.is_draft, |this| {
                        this.text_color(cx.theme().muted_foreground)
                    })
                    .hover(|this| this.text_color(cx.theme().link_hover))
                    .child(pr.title.clone())
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        cx.open_url(&url);
                    }),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("#{}", pr.number)),
            )
            .when(pr.is_draft, |this| {
                this.child(Tag::secondary().xsmall().child("Draft"))
            })
            .when(conflict, |this| {
                this.child(
                    Tag::danger().xsmall().child(
                        h_flex()
                            .gap_1()
                            .child(Icon::new(IconName::TriangleAlert).xsmall())
                            .child("Konflikt"),
                    ),
                )
            })
            .when(pr.requests_review_from(viewer), |this| {
                this.child(Tag::primary().xsmall().child("Dein Review"))
            })
            .children(pr.labels.iter().map(label_tag));

        let meta = meta_row(cx)
            .child(
                h_flex().gap_1p5().child(avatar(pr.author.as_ref())).child(
                    pr.author
                        .as_ref()
                        .map_or("ghost".into(), |a| a.login.clone()),
                ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .min_w_0()
                    .font_family(cx.theme().mono_font_family.clone())
                    .child(Icon::new(Lucide::GitBranch).xsmall())
                    .child(div().truncate().max_w_64().child(pr.head_ref.clone()))
                    .child("→")
                    .child(pr.base_ref.clone()),
            )
            .child(ago(Some(pr.updated_at)))
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_color(cx.theme().success)
                            .child(format!("+{}", pr.additions)),
                    )
                    .child(
                        div()
                            .text_color(cx.theme().danger)
                            .child(format!("−{}", pr.deletions)),
                    ),
            );

        let review = match pr.review_decision {
            Some(ReviewDecision::Approved) => Some(
                Tag::success().xsmall().child(
                    h_flex()
                        .gap_1()
                        .child(Icon::new(Lucide::ShieldCheck).xsmall())
                        .child("Approved"),
                ),
            ),
            Some(ReviewDecision::ChangesRequested) => Some(
                Tag::danger().xsmall().outline().child(
                    h_flex()
                        .gap_1()
                        .child(Icon::new(Lucide::MessageSquareWarning).xsmall())
                        .child("Änderungen"),
                ),
            ),
            _ => None,
        };

        v_flex()
            .border_b_1()
            .border_color(cx.theme().border)
            .relative()
            .when(conflict, |this| this.bg(cx.theme().danger.opacity(0.06)))
            .when_some(
                self.flash(&pr.id, state_color(p.state, cx), cx),
                |this, flash| this.child(flash),
            )
            .child(
                h_flex()
                    .id(SharedString::from(format!("row-{}", pr.id)))
                    .items_start()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .hover(|this| this.bg(cx.theme().list_hover))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_expanded(&id, cx)))
                    .child(div().pt_0p5().child(pipeline_icon(p.state, cx)))
                    .child(v_flex().flex_1().min_w_0().gap_1().child(title).child(meta))
                    .child(
                        h_flex()
                            .id(SharedString::from(format!("actions-{}", pr.id)))
                            .flex_shrink_0()
                            .gap_2()
                            // Klicks auf Steuerelemente sollen die Zeile nicht auf-/zuklappen.
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .children(review)
                            .when(p.total > 0, |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(state_color(p.state, cx))
                                        .child(format!("{}/{}", p.passed, p.total)),
                                )
                            })
                            .child(self.render_merge_control(pr, snapshot, cx)),
                    )
                    .child(
                        Icon::new(if open {
                            IconName::ChevronUp
                        } else {
                            IconName::ChevronDown
                        })
                        .small()
                        .mt_1()
                        .text_color(cx.theme().muted_foreground),
                    ),
            )
            .when(open, |this| {
                let animate = super::motion::enabled(cx);
                this.child(super::motion::enter(
                    SharedString::from(format!("checks-{}", pr.id)),
                    self.render_checks(pr, cx),
                    animate,
                ))
            })
    }

    fn render_checks(&self, pr: &OpenPr, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .pl_12()
            .pr_4()
            .pb_3()
            .gap_0p5()
            .when(pr.pipeline.checks.is_empty(), |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .py_1()
                        .child("Keine Checks für den letzten Commit."),
                )
            })
            .children(pr.pipeline.checks.iter().enumerate().map(|(i, c)| {
                let url = c.url.clone();
                let state = match c.state {
                    CheckState::Skipped | CheckState::Neutral => None,
                    s => Some(s.pipeline()),
                };
                h_flex()
                    .id(SharedString::from(format!("check-{}-{i}", pr.id)))
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded(cx.theme().radius)
                    .text_sm()
                    .when(url.is_some(), |this| {
                        this.cursor_pointer().hover(|s| s.bg(cx.theme().list_hover))
                    })
                    .child(match state {
                        Some(s) => pipeline_icon(s, cx),
                        None => Icon::new(IconName::Minus)
                            .small()
                            .text_color(muted)
                            .into_any_element(),
                    })
                    .child(div().truncate().child(c.name.clone()))
                    .when_some(c.group.clone(), |this, g| {
                        this.child(div().text_xs().text_color(muted).child(g))
                    })
                    .when(matches!(c.state, CheckState::Skipped), |this| {
                        this.child(div().text_xs().text_color(muted).child("übersprungen"))
                    })
                    .child(div().flex_1())
                    .when(url.is_some(), |this| {
                        this.child(Icon::new(IconName::ExternalLink).xsmall().text_color(muted))
                    })
                    .when_some(url, |this, url| {
                        this.on_click(move |_, _, cx| cx.open_url(&url))
                    })
            }))
    }

    /// Merge-Button: Auto-Merge an/aus oder direkt mergen – nur mit im Repo erlaubten Methoden.
    pub(super) fn render_merge_control(
        &self,
        pr: &OpenPr,
        snapshot: &Snapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.radar.read(cx).is_pending(&pr.id);
        let id = pr.id.clone();
        let button_id = SharedString::from(format!("merge-{}", pr.id));
        let radar = self.radar.clone();

        match MergeAction::of(pr, snapshot) {
            MergeAction::Disable { method, by } => Button::new(button_id)
                .small()
                .primary()
                .outline()
                .icon(Lucide::Zap)
                .label(format!("Auto · {}", method.label()))
                .loading(busy)
                .tooltip(format!(
                    "Auto-Merge aktiv ({}{}) – klicken zum Deaktivieren",
                    method.label(),
                    by.map(|b| format!(", von {b}")).unwrap_or_default()
                ))
                .on_click(move |_, _, cx| radar.update(cx, |r, cx| r.set_auto_merge(&id, None, cx)))
                .into_any_element(),
            MergeAction::MergeNow(methods) | MergeAction::Enable(methods) if !busy => {
                let now = matches!(MergeAction::of(pr, snapshot), MergeAction::MergeNow(_));
                let button = Button::new(button_id)
                    .small()
                    .outline()
                    .dropdown_caret(true)
                    .when(now, |b| b.success().icon(Lucide::GitMerge).label("Mergen"))
                    .when(!now, |b| b.icon(Lucide::Zap).label("Auto-Merge"))
                    .tooltip(if now {
                        "Jetzt mergen …"
                    } else {
                        "Auto-Merge aktivieren …"
                    });
                button
                    .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                        let menu = menu.label(if now { "Jetzt mergen" } else { "Merge-Methode" });
                        methods.iter().fold(menu, |menu, &m| {
                            let radar = radar.clone();
                            let id = id.clone();
                            menu.item(PopupMenuItem::new(m.label()).on_click(move |_, _, cx| {
                                radar.update(cx, |r, cx| {
                                    if now {
                                        r.merge(&id, m, cx)
                                    } else {
                                        r.set_auto_merge(&id, Some(m), cx)
                                    }
                                });
                            }))
                        })
                    })
                    .into_any_element()
            }
            MergeAction::MergeNow(_) | MergeAction::Enable(_) => Button::new(button_id)
                .small()
                .outline()
                .label("Mergen")
                .loading(true)
                .into_any_element(),
            MergeAction::Unavailable(reason) => Button::new(button_id)
                .small()
                .outline()
                .icon(Lucide::Zap)
                .label("Auto-Merge")
                .disabled(true)
                .tooltip(reason)
                .into_any_element(),
        }
    }
}
