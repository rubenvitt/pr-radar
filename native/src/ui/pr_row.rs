//! Listenansicht offener PRs: Zeile mit Status, Metadaten, Merge-Steuerung und aufklappbaren Checks.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    tooltip::Tooltip,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Workspace;
use super::parts::{
    accent_badge, avatar, card_segment, label_tag, meta_row, pipeline_icon, repo_name, state_color,
    status_badge,
};
use super::typography::TypeScale as _;
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
            return Self::Unavailable(tr!("Repository not loaded", "Repository nicht geladen"));
        };
        let methods = pr.merge_methods.clone();
        if methods.is_empty() {
            Self::Unavailable(tr!(
                "No merge method allowed (repo settings and rulesets)",
                "Keine Merge-Methode erlaubt (Repo-Einstellungen und Rulesets)"
            ))
        } else if !repo.viewer_can_merge {
            Self::Unavailable(tr!("No write access", "Keine Schreibrechte"))
        } else if pr.is_draft {
            Self::Unavailable(tr!(
                "Draft PRs cannot be merged",
                "Draft-PRs können nicht gemergt werden"
            ))
        } else if pr.ready_to_merge() {
            Self::MergeNow(methods)
        } else if !repo.auto_merge_allowed {
            Self::Unavailable(tr!(
                "Auto-merge is not allowed in this repo (Settings → Allow auto-merge)",
                "Auto-Merge ist in diesem Repo nicht erlaubt (Settings → Allow auto-merge)"
            ))
        } else {
            Self::Enable(methods)
        }
    }
}

/// Breite der Statusspalte (Pipeline-Symbol, Check-Zähler darunter, auch „12/12“).
const STATUS_COLUMN: Rems = rems(2.25);

impl Workspace {
    /// Karte mit PR-Zeilen (Liste) oder Flow-Knoten.
    pub(super) fn render_pr_row(
        &self,
        pr: &OpenPr,
        snapshot: &Snapshot,
        show_repo: bool,
        first: bool,
        last: bool,
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
                this.child(div().text_small().child(repo_name(&pr.repo, cx)))
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
                    .text_small()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("#{}", pr.number)),
            )
            // Drei Badge-Stile: Zustand (Draft), Hervorhebung (Konflikt, Dein Review), Labels.
            .when(pr.is_draft, |this| {
                this.child(status_badge(
                    Icon::new(Lucide::GitPullRequestDraft),
                    "Draft",
                    cx.theme().muted_foreground,
                    cx,
                ))
            })
            .when(conflict, |this| {
                this.child(accent_badge(
                    Icon::new(IconName::TriangleAlert),
                    tr!("Conflict", "Konflikt"),
                    cx.theme().danger,
                ))
            })
            .when(pr.requests_review_from(viewer), |this| {
                this.child(accent_badge(
                    Icon::new(Lucide::Eye),
                    tr!("Your review", "Dein Review"),
                    cx.theme().primary,
                ))
            })
            .children(pr.labels.iter().map(|l| label_tag(l, cx)));

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
                    .child(
                        div()
                            .truncate()
                            .min_w_0()
                            .max_w_64()
                            .child(pr.head_ref.clone()),
                    )
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

        // Review-Zustand: Symbol in Zustandsfarbe, Text sekundär.
        let review = match pr.review_decision {
            Some(ReviewDecision::Approved) => Some(status_badge(
                Icon::new(Lucide::ShieldCheck),
                "Approved",
                cx.theme().success,
                cx,
            )),
            Some(ReviewDecision::ChangesRequested) => Some(status_badge(
                Icon::new(Lucide::MessageSquareWarning),
                tr!("Changes", "Änderungen"),
                cx.theme().danger,
                cx,
            )),
            _ => None,
        };

        // Statussymbol mit Check-Zähler darunter; feste Breite, damit die Titel fluchten.
        let checks_tip = (p.total > 0).then(|| {
            SharedString::from(trf!(
                "Checks: {} of {} passed",
                "Checks: {} von {} erfolgreich",
                p.passed,
                p.total
            ))
        });
        let status = v_flex()
            .id(SharedString::from(format!("status-{}", pr.id)))
            .flex_shrink_0()
            .w(STATUS_COLUMN)
            .items_center()
            .gap_0p5()
            .pt_0p5()
            .child(pipeline_icon(p.state, cx))
            .when(p.total > 0, |this| {
                this.child(
                    div()
                        .text_small()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{}/{}", p.passed, p.total)),
                )
            })
            .when_some(checks_tip, |this, tip| {
                this.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            });

        let compact = self.compact;
        let actions = h_flex()
            .id(SharedString::from(format!("actions-{}", pr.id)))
            .flex_shrink_0()
            .gap_2()
            // Klicks auf Steuerelemente sollen die Zeile nicht auf-/zuklappen.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(review)
            .child(self.merge_control(pr, snapshot, true, cx));
        // Schmal: Steuerung unter die Metadaten, damit der Titel Platz behält.
        let (inline_actions, side_actions) = if compact {
            (Some(actions.flex_wrap().gap_y_1().pt_1()), None)
        } else {
            (None, Some(actions))
        };

        card_segment(first, last, cx)
            .relative()
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
                    .child(status)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(title)
                            .child(meta)
                            .children(inline_actions),
                    )
                    .children(side_actions)
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
            // Konflikt: schmaler Streifen am linken Rand statt getönter Zeile.
            .when(conflict, |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(3.))
                        .bg(cx.theme().danger),
                )
            })
    }

    fn render_checks(&self, pr: &OpenPr, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .when(self.compact, |d| d.pl_4())
            // Unter dem Titel: Zeilenrand + Statusspalte + Abstand.
            .when(!self.compact, |d| {
                d.pl(rems(1.) + STATUS_COLUMN + rems(0.75))
            })
            .pr_4()
            .pb_3()
            .gap_0p5()
            .when(pr.pipeline.checks.is_empty(), |this| {
                this.child(div().text_small().text_color(muted).py_1().child(tr!(
                    "No checks for the latest commit.",
                    "Keine Checks für den letzten Commit."
                )))
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
                    .text_small()
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
                        this.child(div().text_small().text_color(muted).child(g))
                    })
                    .when(matches!(c.state, CheckState::Skipped), |this| {
                        this.child(
                            div()
                                .text_small()
                                .text_color(muted)
                                .child(tr!("skipped", "übersprungen")),
                        )
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
        self.merge_control(pr, snapshot, false, cx)
    }

    /// `quiet`: Voll (Rahmen, Farbe, Beschriftung) nur, wo eine Aktion ansteht oder läuft –
    /// „Mergen“ und aktives Auto-Merge. Auto-Merge aktivieren und nicht Mögliches erscheinen
    /// als schlichtes Symbol mit Tooltip, damit die Liste keine Spalte gleicher Buttons trägt.
    fn merge_control(
        &self,
        pr: &OpenPr,
        snapshot: &Snapshot,
        quiet: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.radar.read(cx).is_pending(&pr.id);
        let retrying = self.radar.read(cx).is_retrying(&pr.id);
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
                .tooltip(if busy {
                    tr!("Changing auto-merge…", "Auto-Merge wird geändert …").to_string()
                } else {
                    let by = by
                        .map(|b| trf!(", by {b}", ", von {b}"))
                        .unwrap_or_default();
                    trf!(
                        "Auto-merge on ({}{by}) – click to turn off",
                        "Auto-Merge aktiv ({}{by}) – klicken zum Deaktivieren",
                        method.label()
                    )
                })
                .when(!busy, |b| {
                    b.on_click(move |_, _, cx| {
                        radar.update(cx, |r, cx| r.set_auto_merge(&id, None, cx))
                    })
                })
                .into_any_element(),
            MergeAction::MergeNow(methods) | MergeAction::Enable(methods) => {
                let now = matches!(MergeAction::of(pr, snapshot), MergeAction::MergeNow(_));
                let full = now || !quiet;
                let button = Button::new(button_id)
                    .small()
                    .when(full, |b| b.outline())
                    .when(!full, |b| b.ghost())
                    .when(now, |b| {
                        b.success()
                            .icon(Lucide::GitMerge)
                            .label(tr!("Merge", "Mergen"))
                    })
                    .when(!now, |b| {
                        b.icon(Lucide::Zap)
                            .when(full, |b| b.label(tr!("Auto-merge", "Auto-Merge")))
                    });

                // Läuft: gleiche Optik, Spinner statt Symbol, keine weiteren Klicks.
                if busy {
                    return button
                        .loading(true)
                        .tooltip(if retrying {
                            tr!(
                                "GitHub does not allow the merge yet – retrying shortly…",
                                "GitHub lässt den Merge noch nicht zu – neuer Versuch gleich …"
                            )
                        } else if now {
                            tr!("Merging…", "Wird gemergt …")
                        } else {
                            tr!("Turning on auto-merge…", "Auto-Merge wird aktiviert …")
                        })
                        .into_any_element();
                }

                let run = move |radar: &Entity<crate::radar::Radar>,
                                id: &str,
                                m: MergeMethod,
                                cx: &mut App| {
                    radar.update(cx, |r, cx| {
                        if now {
                            r.merge(id, m, cx)
                        } else {
                            r.set_auto_merge(id, Some(m), cx)
                        }
                    });
                };

                // Nur eine erlaubte Methode: kein Menü, der Button führt sie direkt aus.
                if let [m] = methods[..] {
                    return button
                        .tooltip(if now {
                            trf!("Merge now ({})", "Jetzt mergen ({})", m.label())
                        } else {
                            trf!(
                                "Turn on auto-merge ({})",
                                "Auto-Merge aktivieren ({})",
                                m.label()
                            )
                        })
                        .on_click(move |_, _, cx| run(&radar, &id, m, cx))
                        .into_any_element();
                }
                button
                    .dropdown_caret(full)
                    .tooltip(if now {
                        tr!("Merge now…", "Jetzt mergen …")
                    } else {
                        tr!("Turn on auto-merge…", "Auto-Merge aktivieren …")
                    })
                    .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                        let menu = menu.label(if now {
                            tr!("Merge now", "Jetzt mergen")
                        } else {
                            tr!("Merge method", "Merge-Methode")
                        });
                        methods.iter().fold(menu, |menu, &m| {
                            let radar = radar.clone();
                            let id = id.clone();
                            menu.item(
                                PopupMenuItem::new(m.label())
                                    .on_click(move |_, _, cx| run(&radar, &id, m, cx)),
                            )
                        })
                    })
                    .into_any_element()
            }
            MergeAction::Unavailable(reason) => Button::new(button_id)
                .small()
                .when(quiet, |b| b.ghost())
                .when(!quiet, |b| {
                    b.outline().label(tr!("Auto-merge", "Auto-Merge"))
                })
                .icon(Lucide::Zap)
                .disabled(true)
                .tooltip(reason)
                .into_any_element(),
        }
    }
}
