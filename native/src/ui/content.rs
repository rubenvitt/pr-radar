//! Inhaltsbereich als virtualisierte Liste: Nur sichtbare Einträge werden gerendert und
//! gelayoutet, jeder für sich. Ein Layout über alle PRs je Frame war der Grund für die Trägheit.

use std::sync::Arc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{
    ActiveTheme as _, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    scroll::ScrollableElement as _,
    spinner::Spinner,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::parts::{EmptyState, pipeline_icon, repo_name};
use super::typography::TypeScale as _;
use super::{Derived, OpenSettings, Refresh, Workspace, quick_label};
use crate::config::{Prefs, Quick, Tab, View};
use crate::model::Snapshot;
use crate::radar::PollState;
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
            return EmptyState::new(
                Lucide::GitPullRequest,
                tr!("No repositories yet", "Noch keine Repositories"),
            )
            .hint(tr!(
                "PR Radar shows open pull requests, pipelines, merges and releases \
                 from your GitHub repositories in one place. Choose which ones to watch.",
                "PR Radar zeigt offene Pull Requests, Pipelines, Merges und Releases \
                 aus deinen GitHub-Repositories an einem Ort. Wähle aus, welche du beobachten willst."
            ))
                .action(
                    Button::new("add-repos")
                        .primary()
                        .icon(IconName::Plus)
                        .label(tr!("Add Repositories…", "Repositories hinzufügen…"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_settings(&OpenSettings, window, cx)
                        })),
                )
                .into_any_element();
        }
        let Some(d) = d else {
            if radar.status().needs_login {
                let mut state = EmptyState::new(
                    Lucide::CircleUser,
                    tr!("Not signed in to GitHub", "Nicht bei GitHub angemeldet"),
                )
                .action(
                    Button::new("sign-in")
                        .primary()
                        .label(tr!("Sign In…", "Anmelden…"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_settings(&OpenSettings, window, cx)
                        })),
                );
                if let Some(e) = radar.status().error.clone() {
                    state = state.hint(e);
                }
                return state.into_any_element();
            }
            let polling = radar.status().state == PollState::Polling;
            return match radar.status().error.clone() {
                Some(e) => EmptyState::new(
                    Lucide::CircleX,
                    tr!("GitHub unreachable", "GitHub nicht erreichbar"),
                )
                .color(cx.theme().danger)
                .hint(e)
                .action(
                    Button::new("retry")
                        .primary()
                        .icon(IconName::RefreshCw)
                        .label(tr!("Try Again", "Erneut versuchen"))
                        .loading(polling)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.refresh(&Refresh, window, cx)),
                        ),
                )
                .into_any_element(),
                None => v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .text_color(cx.theme().muted_foreground)
                    .child(Spinner::new().large())
                    .child(tr!("Loading data from GitHub…", "Lade Daten von GitHub …"))
                    .into_any_element(),
            };
        };

        self.sync_list(d);
        if self.list_model.rows.is_empty() {
            return self.render_empty(d, cx);
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

    /// Die Liste ist leer: sagen, warum, und eine Aktion anbieten, die weiterhilft.
    fn render_empty(&self, d: &Derived, cx: &mut Context<Self>) -> AnyElement {
        // Nicht erreichbare Repos (im Repo-Filter) können offene PRs verbergen.
        let failed = d
            .snapshot
            .repos
            .iter()
            .filter(|r| {
                r.error.is_some()
                    && (d.prefs.repo_filter.is_empty()
                        || d.prefs.repo_filter.contains(&r.full_name))
            })
            .count();
        let filters = active_filters(&d.prefs, &self.query);
        if !filters.is_empty() {
            let what = match d.prefs.tab {
                Tab::Open => tr!("No open pull requests", "Keine offenen Pull Requests"),
                Tab::Merged => tr!("No merged pull requests", "Keine gemergten Pull Requests"),
                Tab::Releases => tr!("No releases", "Keine Releases"),
            };
            let filters = filters.join(" · ");
            let hint = if d.prefs.tab == Tab::Open && failed > 0 {
                trf!(
                    "{what} for {filters}, but {} – see the sidebar.",
                    "{what} für {filters}, aber {} – siehe Seitenleiste.",
                    not_loaded(failed)
                )
            } else {
                trf!("{what} for {filters}.", "{what} für {filters}.")
            };
            return EmptyState::new(IconName::Search, tr!("No matches", "Keine Treffer"))
                .hint(hint)
                .action(
                    Button::new("reset-filters")
                        .primary()
                        .label(tr!("Reset Filters", "Filter zurücksetzen"))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.reset_filters(window, cx)),
                        ),
                )
                .into_any_element();
        }

        let repos = in_repos(self.radar.read(cx).repos().len());
        let to_tab = |id: &'static str, label: &'static str, tab: Tab| {
            Button::new(id)
                .label(label)
                .on_click(cx.listener(move |this, _, _, cx| this.show_tab(tab, cx)))
        };
        let state = match d.prefs.tab {
            // Dann kein Erfolg melden.
            Tab::Open if failed > 0 => {
                EmptyState::new(IconName::Inbox, tr!("Nothing open", "Nichts offen")).hint(trf!(
                    "No pull request is open {repos}, but {} – see the sidebar.",
                    "{repos} ist kein Pull Request offen, aber {} – siehe Seitenleiste.",
                    not_loaded(failed)
                ))
            }
            Tab::Open => EmptyState::new(Lucide::CircleCheck, tr!("All done", "Alles erledigt"))
                .color(cx.theme().success)
                .hint(trf!(
                    "No pull requests are open {repos} right now.",
                    "{repos} ist gerade kein Pull Request offen."
                )),
            Tab::Merged => EmptyState::new(
                Lucide::GitMerge,
                tr!("Nothing merged yet", "Noch nichts gemergt"),
            )
            .hint(trf!(
                "No pull requests have been merged {repos} yet.",
                "{repos} wurde bisher kein Pull Request gemergt."
            )),
            Tab::Releases => EmptyState::new(Lucide::Tag, tr!("No releases", "Keine Releases"))
                .hint(trf!(
                    "There are no releases {repos} yet.",
                    "{repos} gibt es noch kein Release."
                )),
        };
        let action = match d.prefs.tab {
            Tab::Open => to_tab(
                "show-merged",
                tr!("View Recently Merged", "Zuletzt gemergt ansehen"),
                Tab::Merged,
            ),
            Tab::Merged | Tab::Releases => to_tab(
                "show-open",
                tr!("View Open Pull Requests", "Offene Pull Requests ansehen"),
                Tab::Open,
            ),
        };
        state.action(action).into_any_element()
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
                .text_heading()
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
                    .min_w_0()
                    .text_heading()
                    .cursor_pointer()
                    .child(repo_name(repo, cx))
                    .on_click(move |_, _, cx| cx.open_url(&url)),
            )
            .child(
                div()
                    .text_small()
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
                            .flex_shrink_0()
                            .gap_1p5()
                            .text_small()
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

/// Aktive Filter als lesbare Teile, z. B. `„Konflikt“`, `Repo „infra“`, `Suche „fix“`.
/// Der Schnellfilter zählt nur bei den offenen PRs.
fn active_filters(prefs: &Prefs, query: &str) -> Vec<String> {
    let mut parts = Vec::new();
    if prefs.tab == Tab::Open && prefs.quick != Quick::All {
        let label = quick_label(prefs.quick);
        parts.push(trf!("“{label}”", "„{label}“"));
    }
    if !prefs.repo_filter.is_empty() {
        let names: Vec<String> = prefs
            .repo_filter
            .iter()
            .map(|r| {
                let name = r.split_once('/').map_or(r.as_str(), |(_, n)| n);
                trf!("“{name}”", "„{name}“")
            })
            .collect();
        let noun = if names.len() == 1 { "Repo" } else { "Repos" };
        parts.push(format!("{noun} {}", names.join(", ")));
    }
    let query = query.trim();
    if !query.is_empty() {
        parts.push(trf!("search “{query}”", "Suche „{query}“"));
    }
    parts
}

/// „In deinem Repository“ / „In deinen 3 Repositories“ (englisch klein: „in your repository“)
fn in_repos(n: usize) -> String {
    if n == 1 {
        tr!("in your repository", "In deinem Repository").into()
    } else {
        trf!("in your {n} repositories", "In deinen {n} Repositories")
    }
}

/// „eins konnte nicht abgefragt werden“ / „2 konnten nicht abgefragt werden“
fn not_loaded(n: usize) -> String {
    if n == 1 {
        tr!(
            "one could not be fetched",
            "eins konnte nicht abgefragt werden"
        )
        .into()
    } else {
        trf!(
            "{n} could not be fetched",
            "{n} konnten nicht abgefragt werden"
        )
    }
}

#[cfg(test)]
mod tests {
    // Kein `use super::*`: Der Glob aus gpui_kit bringt ein eigenes `test`-Makro mit.
    use super::{active_filters, in_repos, not_loaded};
    use crate::config::{Prefs, Quick, Tab};
    use crate::l10n::{Lang, with_lang};

    fn prefs(tab: Tab, quick: Quick, repos: &[&str]) -> Prefs {
        Prefs {
            tab,
            quick,
            repo_filter: repos.iter().map(|r| r.to_string()).collect(),
            ..Prefs::default()
        }
    }

    #[test]
    fn ohne_filter_ist_nichts_aktiv() {
        assert!(active_filters(&prefs(Tab::Open, Quick::All, &[]), "  ").is_empty());
    }

    #[test]
    fn nennt_schnellfilter_repos_und_suche() {
        let p = prefs(Tab::Open, Quick::Conflict, &["acme/infra"]);
        assert_eq!(
            with_lang(Lang::De, || active_filters(&p, " fix ")),
            ["„Konflikt“", "Repo „infra“", "Suche „fix“"]
        );
        assert_eq!(
            with_lang(Lang::En, || active_filters(&p, " fix ")),
            ["“Conflict”", "Repo “infra”", "search “fix”"]
        );
        let p = prefs(Tab::Open, Quick::All, &["acme/infra", "acme/api"]);
        assert_eq!(
            with_lang(Lang::De, || active_filters(&p, "")),
            ["Repos „infra“, „api“"]
        );
    }

    #[test]
    fn schnellfilter_zaehlt_nur_bei_offenen() {
        let p = prefs(Tab::Merged, Quick::Conflict, &[]);
        assert!(active_filters(&p, "").is_empty());
    }

    #[test]
    fn repo_anzahl_im_satz() {
        with_lang(Lang::De, || {
            assert_eq!(in_repos(1), "In deinem Repository");
            assert_eq!(in_repos(3), "In deinen 3 Repositories");
            assert_eq!(not_loaded(1), "eins konnte nicht abgefragt werden");
            assert_eq!(not_loaded(2), "2 konnten nicht abgefragt werden");
        });
        with_lang(Lang::En, || {
            assert_eq!(in_repos(3), "in your 3 repositories");
            assert_eq!(not_loaded(2), "2 could not be fetched");
        });
    }
}
