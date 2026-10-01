//! Inhaltsbereich als virtualisierte Liste: Nur sichtbare Einträge werden gerendert und
//! gelayoutet, jeder für sich. Ein Layout über alle PRs je Frame war der Grund für die Trägheit.

use std::sync::Arc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, IconName, Sizable as _, button::Button, h_flex,
    scroll::ScrollableElement as _, spinner::Spinner, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::parts::{empty_state, pipeline_icon, repo_name};
use super::{Derived, OpenSettings, Workspace};
use crate::config::{Tab, View};
use crate::model::Snapshot;
use crate::time::day_label;

/// Ein Eintrag der Inhaltsliste. Zeilen einer Karte tragen `first`/`last`, damit sie
/// gemeinsam als eine Karte mit runden Ecken erscheinen.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Row {
    RepoHeader {
        repo: String,
        count: usize,
    },
    Pr {
        ix: usize,
        show_repo: bool,
        first: bool,
        last: bool,
    },
    Day {
        label: String,
    },
    Merged {
        ix: usize,
        first: bool,
        last: bool,
    },
    Release {
        ix: usize,
        default_open: bool,
    },
}

impl Row {
    /// Stabile Identität über Polls hinweg (Indizes in den Snapshot ändern sich).
    fn key(&self, s: &Snapshot) -> String {
        match self {
            Row::RepoHeader { repo, .. } => format!("repo:{repo}"),
            Row::Pr { ix, .. } => format!("pr:{}", s.open[*ix].id),
            Row::Day { label } => format!("day:{label}"),
            Row::Merged { ix, .. } => format!("merged:{}", s.merged[*ix].id),
            Row::Release { ix, .. } => format!("release:{}", s.releases[*ix].id),
        }
    }

    fn is_section_start(&self) -> bool {
        matches!(self, Row::RepoHeader { .. } | Row::Day { .. })
    }
}

/// Was die Liste aktuell zeigt – für den Abgleich mit `ListState`.
#[derive(Default)]
pub(super) struct ListModel {
    rows: Vec<Row>,
    keys: Vec<String>,
    snapshot: Option<Arc<Snapshot>>,
    /// Ansicht, Filter und Suche: ändern sie sich, beginnt die Liste oben.
    context: String,
}

fn card_rows(count: usize) -> impl Iterator<Item = (usize, bool, bool)> {
    (0..count).map(move |i| (i, i == 0, i + 1 == count))
}

fn build_rows(d: &Derived) -> Vec<Row> {
    let s = &d.snapshot;
    let mut rows = Vec::new();
    match d.prefs.tab {
        Tab::Open if !d.prefs.grouped => {
            for (i, first, last) in card_rows(d.open_list.len()) {
                rows.push(Row::Pr {
                    ix: d.open_list[i],
                    show_repo: true,
                    first,
                    last,
                });
            }
        }
        Tab::Open => {
            let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
            for &ix in &d.open_list {
                let repo = &s.open[ix].repo;
                match groups.iter_mut().find(|(r, _)| r == repo) {
                    Some((_, list)) => list.push(ix),
                    None => groups.push((repo.clone(), vec![ix])),
                }
            }
            for (repo, list) in groups {
                rows.push(Row::RepoHeader {
                    repo,
                    count: list.len(),
                });
                for (i, first, last) in card_rows(list.len()) {
                    rows.push(Row::Pr {
                        ix: list[i],
                        show_repo: false,
                        first,
                        last,
                    });
                }
            }
        }
        Tab::Merged => {
            let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
            for &ix in &d.merged {
                let label = day_label(s.merged[ix].merged_at);
                match groups.last_mut() {
                    Some((l, list)) if *l == label => list.push(ix),
                    _ => groups.push((label, vec![ix])),
                }
            }
            for (label, list) in groups {
                rows.push(Row::Day { label });
                for (i, first, last) in card_rows(list.len()) {
                    rows.push(Row::Merged {
                        ix: list[i],
                        first,
                        last,
                    });
                }
            }
        }
        Tab::Releases => {
            // Neuestes Release je Repo standardmäßig aufgeklappt.
            let mut seen = std::collections::HashSet::new();
            for &ix in &d.releases {
                let default_open = seen.insert(s.releases[ix].repo.clone());
                rows.push(Row::Release { ix, default_open });
            }
        }
    }
    rows
}

impl Workspace {
    /// Gleicht die Liste mit dem aktuellen Stand ab und misst nur neu, was sich geändert hat.
    pub(super) fn sync_list(&mut self, d: &Derived) {
        let rows = build_rows(d);
        let keys: Vec<String> = rows.iter().map(|r| r.key(&d.snapshot)).collect();
        let context = format!(
            "{:?}|{:?}|{:?}|{}|{:?}|{}",
            d.prefs.tab,
            d.prefs.view,
            d.prefs.quick,
            d.prefs.grouped,
            d.prefs.repo_filter,
            self.query
        );
        let model = &mut self.list_model;
        let same_data = model
            .snapshot
            .as_ref()
            .is_some_and(|s| Arc::ptr_eq(s, &d.snapshot));

        if model.context != context {
            self.list.reset(rows.len());
        } else if model.keys != keys {
            // Neue oder entfallene Einträge (z. B. nach einem Poll): nur den Mittelteil ersetzen,
            // damit die Scroll-Position erhalten bleibt.
            let prefix = model
                .keys
                .iter()
                .zip(&keys)
                .take_while(|(a, b)| a == b)
                .count();
            let max_suffix = model.keys.len().min(keys.len()) - prefix;
            let suffix = model
                .keys
                .iter()
                .rev()
                .zip(keys.iter().rev())
                .take(max_suffix)
                .take_while(|(a, b)| a == b)
                .count();
            self.list.splice(
                prefix..model.keys.len() - suffix,
                keys.len() - prefix - suffix,
            );
            self.list.remeasure();
        } else if !same_data || model.rows != rows {
            self.list.remeasure();
        }

        model.rows = rows;
        model.keys = keys;
        model.snapshot = Some(d.snapshot.clone());
        model.context = context;
    }

    /// Ein Eintrag hat seine Höhe geändert (auf-/zugeklappt).
    pub(super) fn remeasure_key(&self, key: &str) {
        let Some(s) = self.list_model.snapshot.as_ref() else {
            return;
        };
        if let Some(ix) = self.list_model.rows.iter().position(|r| r.key(s) == key) {
            self.list.remeasure_items(ix..ix + 1);
        }
    }

    pub(super) fn render_content(
        &mut self,
        d: Option<&Derived>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let radar = self.radar.read(cx);
        if radar.repos().is_empty() {
            return empty_state(Lucide::GitPullRequest, "Noch keine Repositories", None, cx)
                .child(
                    Button::new("add-repos")
                        .label("Repositories hinzufügen…")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_settings(&OpenSettings, window, cx)
                        })),
                )
                .into_any_element();
        }
        let Some(d) = d else {
            return match radar.status().error.clone() {
                Some(e) => empty_state(
                    Lucide::CircleX,
                    "GitHub nicht erreichbar",
                    Some(e.into()),
                    cx,
                )
                .into_any_element(),
                None => v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .text_color(cx.theme().muted_foreground)
                    .child(Spinner::new().large())
                    .child("Lade Daten von GitHub …")
                    .into_any_element(),
            };
        };

        self.sync_list(d);
        if self.list_model.rows.is_empty() {
            let (icon, title, hint): (Lucide, &str, Option<SharedString>) = match d.prefs.tab {
                Tab::Open => (
                    IconName::Inbox.into(),
                    "Nichts offen",
                    Some("Keine Pull Requests für diesen Filter.".into()),
                ),
                Tab::Merged => (Lucide::GitMerge, "Noch nichts gemergt", None),
                Tab::Releases => (Lucide::Tag, "Keine Releases", None),
            };
            return empty_state(icon, title, hint, cx).into_any_element();
        }

        let count = self.list_model.rows.len();
        let items = list(
            self.list.clone(),
            cx.processor(move |this, ix: usize, _window, cx| this.render_row(ix, count, cx)),
        )
        .size_full();

        // Der Flow braucht Mindestbreite; in schmalen Fenstern scrollt er horizontal.
        let body = if d.prefs.tab == Tab::Open && d.prefs.view == View::Flow {
            div()
                .id("flow-hscroll")
                .size_full()
                .overflow_x_scroll()
                .child(div().h_full().w_full().min_w(rems(60.)).child(items))
                .into_any_element()
        } else {
            items.into_any_element()
        };

        div()
            .id("content-list")
            .relative()
            .size_full()
            .child(body)
            .vertical_scrollbar(&self.list)
            .into_any_element()
    }

    fn render_row(&mut self, ix: usize, count: usize, cx: &mut Context<Self>) -> AnyElement {
        let (Some(row), Some(snapshot)) = (
            self.list_model.rows.get(ix).cloned(),
            self.list_model.snapshot.clone(),
        ) else {
            return div().into_any_element();
        };
        let flow = self.radar.read(cx).prefs().view == View::Flow;
        let el = match &row {
            Row::RepoHeader { repo, count } => self
                .render_repo_header(repo, *count, &snapshot, cx)
                .into_any_element(),
            Row::Pr {
                ix,
                show_repo,
                first,
                last,
            } => {
                let pr = &snapshot.open[*ix];
                if flow {
                    self.render_flow_row(pr, &snapshot, *show_repo, *first, *last, cx)
                        .into_any_element()
                } else {
                    self.render_pr_row(pr, &snapshot, *show_repo, *first, *last, cx)
                        .into_any_element()
                }
            }
            Row::Day { label } => div()
                .px_1()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child(label.clone())
                .into_any_element(),
            Row::Merged { ix, first, last } => {
                self.render_merged_row(&snapshot.merged[*ix], *first, *last, cx)
            }
            Row::Release { ix, default_open } => {
                self.render_release(&snapshot.releases[*ix], *default_open, cx)
            }
        };

        // Gleiche Inhaltsbreite wie zuvor: zentriert, lesbare Zeilenlänge, Abstand zwischen Abschnitten.
        h_flex()
            .w_full()
            .justify_center()
            .child(
                div()
                    .w_full()
                    .max_w(rems(84.))
                    .px_4()
                    .when(ix == 0, |d| d.pt_4())
                    .when(ix + 1 == count, |d| d.pb_4())
                    .when(row.is_section_start(), |d| d.pb_2())
                    .when(row.is_section_start() && ix > 0, |d| d.pt_6())
                    .when(matches!(row, Row::Release { .. }) && ix + 1 < count, |d| {
                        d.pb_3()
                    })
                    .child(el),
            )
            .into_any_element()
    }

    fn render_repo_header(
        &self,
        repo: &str,
        count: usize,
        snapshot: &Snapshot,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let info = snapshot.repo(repo);
        let url = info
            .map(|i| i.url.clone())
            .unwrap_or_else(|| format!("https://github.com/{repo}"));
        h_flex()
            .gap_2()
            .px_1()
            .child(
                h_flex()
                    .id(SharedString::from(format!("repo-{repo}")))
                    .text_sm()
                    .font_semibold()
                    .cursor_pointer()
                    .child(repo_name(repo, cx))
                    .on_click(move |_, _, cx| cx.open_url(&url)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(count.to_string()),
            )
            .child(div().flex_1())
            .when_some(
                info.and_then(|i| {
                    i.default_branch
                        .clone()
                        .map(|b| (b, i.default_branch_pipeline))
                }),
                |this, (branch, state)| {
                    this.child(
                        h_flex()
                            .gap_1p5()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(pipeline_icon(state, cx))
                            .child(
                                div()
                                    .font_family(cx.theme().mono_font_family.clone())
                                    .child(branch),
                            ),
                    )
                },
            )
    }
}
