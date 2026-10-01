//! Hauptfenster: Titelleiste, Seitenleiste (Ansichten, Filter, Repos) und Inhaltsbereich.

mod flow;
mod motion;
mod parts;
mod pr_row;
mod settings;
mod views;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Selectable as _, Sizable as _, TitleBar, WindowExt as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    notification::Notification,
    scroll::ScrollableElement as _,
    sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarMenu, SidebarMenuItem},
    spinner::Spinner,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::config::{Prefs, Quick, Tab, View};
use crate::model::{OpenPr, PipelineState, Snapshot, TransitionKind};
use crate::radar::{PollState, Radar, RadarEvent};
use crate::time::ago;
use parts::{empty_state, pipeline_icon, repo_name, state_color};
use settings::RepoSettings;

actions!(
    pr_radar,
    [
        FocusSearch,
        Refresh,
        ToggleView,
        ToggleGrouped,
        ShowOpen,
        ShowMerged,
        ShowReleases,
        OpenSettings,
        Quit
    ]
);

const CONTEXT: &str = "Workspace";

pub fn init(cx: &mut App) {
    // Einzeltasten nur, solange kein Eingabefeld den Fokus hat.
    let keys = Some("Workspace && !Input");
    cx.bind_keys([
        KeyBinding::new("/", FocusSearch, keys),
        KeyBinding::new("cmd-f", FocusSearch, Some(CONTEXT)),
        KeyBinding::new("r", Refresh, keys),
        KeyBinding::new("cmd-r", Refresh, Some(CONTEXT)),
        KeyBinding::new("v", ToggleView, keys),
        KeyBinding::new("g", ToggleGrouped, keys),
        KeyBinding::new("1", ShowOpen, keys),
        KeyBinding::new("2", ShowMerged, keys),
        KeyBinding::new("3", ShowReleases, keys),
        KeyBinding::new("cmd-1", ShowOpen, Some(CONTEXT)),
        KeyBinding::new("cmd-2", ShowMerged, Some(CONTEXT)),
        KeyBinding::new("cmd-3", ShowReleases, Some(CONTEXT)),
        KeyBinding::new("cmd-,", OpenSettings, Some(CONTEXT)),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.set_menus(vec![
        Menu::new("PR Radar").items([
            MenuItem::action("Repositories…", OpenSettings),
            MenuItem::separator(),
            MenuItem::action("PR Radar beenden", Quit),
        ]),
        Menu::new("Ansicht").items([
            MenuItem::action("Offen", ShowOpen),
            MenuItem::action("Gemergt", ShowMerged),
            MenuItem::action("Releases", ShowReleases),
            MenuItem::separator(),
            MenuItem::action("Liste / Flow umschalten", ToggleView),
            MenuItem::action("Nach Repo gruppieren", ToggleGrouped),
            MenuItem::separator(),
            MenuItem::action("Suchen", FocusSearch),
            MenuItem::action("Aktualisieren", Refresh),
        ]),
    ]);
}

pub struct Workspace {
    radar: Entity<Radar>,
    search: Entity<InputState>,
    query: String,
    focus: FocusHandle,
    /// Aufgeklappte PR-Zeilen (Checks sichtbar)
    expanded: HashSet<String>,
    /// Releases, deren Aufklapp-Zustand vom Standard abweicht
    toggled_releases: HashSet<String>,
    /// Kürzlich geänderte PRs → Durchlauf-Nummer des Aufleuchtens
    flashes: HashMap<String, u64>,
    flash_epoch: u64,
    settings: Entity<RepoSettings>,
    _subscriptions: Vec<Subscription>,
    _ticker: Task<()>,
}

/// Für einen Render-Durchlauf abgeleitete Listen.
struct Derived {
    snapshot: Arc<Snapshot>,
    prefs: Prefs,
    /// Offene PRs nach Repo-Filter und Suche (Basis der Zähler)
    open_base: Vec<usize>,
    /// … zusätzlich nach Schnellfilter
    open_list: Vec<usize>,
    merged: Vec<usize>,
    releases: Vec<usize>,
}

impl Derived {
    fn new(snapshot: Arc<Snapshot>, prefs: Prefs, query: &str) -> Self {
        let q = query.trim().to_lowercase();
        let repo_ok = |repo: &str| {
            prefs.repo_filter.is_empty() || prefs.repo_filter.iter().any(|r| r == repo)
        };
        let text_ok =
            |parts: &[&str]| q.is_empty() || parts.iter().any(|p| p.to_lowercase().contains(&q));

        let open_base: Vec<usize> = (0..snapshot.open.len())
            .filter(|&i| {
                let p = &snapshot.open[i];
                let author = p.author.as_ref().map(|a| a.login.as_str()).unwrap_or("");
                repo_ok(&p.repo)
                    && text_ok(&[
                        &p.title,
                        &p.repo,
                        author,
                        &p.head_ref,
                        &format!("#{}", p.number),
                    ])
            })
            .collect();
        let viewer = snapshot.viewer.clone();
        let open_list = open_base
            .iter()
            .copied()
            .filter(|&i| quick_matches(prefs.quick, &snapshot.open[i], viewer.as_deref()))
            .collect();
        let merged = (0..snapshot.merged.len())
            .filter(|&i| {
                let m = &snapshot.merged[i];
                repo_ok(&m.repo) && text_ok(&[&m.title, &m.repo, &format!("#{}", m.number)])
            })
            .collect();
        let releases = (0..snapshot.releases.len())
            .filter(|&i| {
                let r = &snapshot.releases[i];
                repo_ok(&r.repo) && text_ok(&[&r.name, &r.tag_name, &r.repo])
            })
            .collect();
        Self {
            snapshot,
            prefs,
            open_base,
            open_list,
            merged,
            releases,
        }
    }

    fn count(&self, quick: Quick) -> usize {
        let viewer = self.snapshot.viewer.as_deref();
        self.open_base
            .iter()
            .filter(|&&i| quick_matches(quick, &self.snapshot.open[i], viewer))
            .count()
    }
}

fn quick_matches(quick: Quick, pr: &OpenPr, viewer: Option<&str>) -> bool {
    match quick {
        Quick::All => true,
        Quick::Running => pr.pipeline.state == PipelineState::Running,
        Quick::Failed => pr.pipeline.state == PipelineState::Failed,
        Quick::Passed => pr.pipeline.state == PipelineState::Passed,
        Quick::Auto => pr.auto_merge.is_some(),
        Quick::Conflict => pr.has_conflict(),
        Quick::Review => pr.requests_review_from(viewer),
        Quick::Mine => pr.is_by(viewer),
    }
}

impl Workspace {
    pub fn new(radar: Entity<Radar>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Suchen …")
                .clean_on_escape()
        });
        let settings = cx.new(|cx| RepoSettings::new(radar.clone(), window, cx));
        let focus = cx.focus_handle();
        focus.focus(window, cx);

        let subscriptions = vec![
            cx.subscribe_in(
                &search,
                window,
                |this, state, event, window, cx| match event {
                    InputEvent::Change => {
                        this.query = state.read(cx).value().to_string();
                        cx.notify();
                    }
                    InputEvent::PressEnter { .. } => this.focus.focus(window, cx),
                    _ => {}
                },
            ),
            cx.observe_in(&radar, window, |_, radar, window, cx| {
                let failed = radar
                    .read(cx)
                    .snapshot()
                    .map(|s| {
                        s.open
                            .iter()
                            .filter(|p| p.pipeline.state == PipelineState::Failed)
                            .count()
                    })
                    .unwrap_or(0);
                window.set_window_title(&match failed {
                    0 => "PR Radar".to_string(),
                    n => format!("PR Radar – {n} rot"),
                });
                cx.notify();
            }),
            cx.subscribe_in(&radar, window, Self::on_radar_event),
        ];

        // Relative Zeitangaben („vor 2 Min.“) aktuell halten.
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(15))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    return;
                }
            }
        });

        Self {
            radar,
            search,
            query: String::new(),
            focus,
            expanded: HashSet::new(),
            toggled_releases: HashSet::new(),
            flashes: HashMap::new(),
            flash_epoch: 0,
            settings,
            _subscriptions: subscriptions,
            _ticker: ticker,
        }
    }

    fn on_radar_event(
        &mut self,
        _: &Entity<Radar>,
        event: &RadarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            RadarEvent::Changed(ids) => {
                self.flash_epoch += 1;
                let epoch = self.flash_epoch;
                for id in ids {
                    self.flashes.insert(id.clone(), epoch);
                }
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(motion::FLASH + Duration::from_millis(200))
                        .await;
                    this.update(cx, |this, cx| {
                        this.flashes.retain(|_, e| *e != epoch);
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
                cx.notify();
            }
            RadarEvent::ActionFailed(message) => {
                window.push_notification(
                    Notification::error(message.clone()).title("Aktion fehlgeschlagen"),
                    cx,
                );
            }
            RadarEvent::Transitions(transitions) => {
                if !self.radar.read(cx).prefs().notify {
                    return;
                }
                for t in transitions {
                    let (title, notification) = match t.kind {
                        TransitionKind::Failed => {
                            ("Pipeline rot", Notification::error(t.pr.title.clone()))
                        }
                        TransitionKind::Passed => {
                            ("Pipeline grün", Notification::success(t.pr.title.clone()))
                        }
                        TransitionKind::Merged => {
                            ("Gemergt", Notification::info(t.pr.title.clone()))
                        }
                    };
                    let url = t.pr.url.clone();
                    window.push_notification(
                        notification
                            .id1::<Workspace>(SharedString::from(format!(
                                "{}-{:?}",
                                t.pr.id, t.kind
                            )))
                            .title(format!("{title} · {}#{}", t.pr.repo, t.pr.number))
                            .in_app_and_system()
                            .on_click(move |_, _, cx| cx.open_url(&url)),
                        cx,
                    );
                }
            }
        }
    }

    fn update_prefs(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Prefs)) {
        self.radar.update(cx, |radar, cx| radar.update_prefs(cx, f));
    }

    fn focus_search(&mut self, _: &FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |s, cx| s.focus(window, cx));
    }

    fn refresh(&mut self, _: &Refresh, _: &mut Window, cx: &mut Context<Self>) {
        self.radar.update(cx, |r, cx| r.refresh(cx));
    }

    fn toggle_view(&mut self, _: &ToggleView, _: &mut Window, cx: &mut Context<Self>) {
        self.update_prefs(cx, |p| {
            p.view = if p.view == View::Flow {
                View::List
            } else {
                View::Flow
            }
        });
    }

    fn toggle_grouped(&mut self, _: &ToggleGrouped, _: &mut Window, cx: &mut Context<Self>) {
        self.update_prefs(cx, |p| p.grouped = !p.grouped);
    }

    fn show_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.update_prefs(cx, |p| p.tab = tab);
    }

    fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        let settings = self.settings.clone();
        settings.update(cx, |s, cx| s.reset(window, cx));
        window.open_sheet(cx, move |sheet, _, _| {
            sheet
                .title("Repositories")
                .size(rems(26.))
                .child(settings.clone())
        });
    }

    fn toggle_expanded(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(id) {
            self.expanded.insert(id.to_string());
        }
        cx.notify();
    }

    /* ---------- Titelleiste ---------- */

    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let radar = self.radar.read(cx);
        let status = radar.status().clone();
        let notify = radar.prefs().notify;
        let loading = radar.snapshot().is_none();

        let (dot, label) = match status.state {
            PollState::Error => (cx.theme().danger, "Fehler".to_string()),
            PollState::Polling if loading => (cx.theme().warning, "Lädt …".to_string()),
            _ => (
                cx.theme().success,
                format!("Live · {}", ago(status.last_success)),
            ),
        };
        let tooltip: SharedString = match status.last_success {
            Some(t) => format!(
                "Letzte Aktualisierung {} · alle {} s",
                crate::time::clock(t),
                status.interval.as_secs()
            )
            .into(),
            None => "Noch nicht aktualisiert".into(),
        };

        TitleBar::new().child(
            h_flex()
                .w_full()
                .pr_2()
                .gap_3()
                .child(div().text_sm().font_semibold().child("PR Radar"))
                .child(
                    h_flex()
                        .id("live-status")
                        .gap_1p5()
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .border_1()
                        .border_color(cx.theme().border)
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(motion::pulse(
                            "live-dot",
                            div().size_1p5().rounded_full().bg(dot),
                            status.state != PollState::Error,
                            cx,
                        ))
                        .child(label)
                        .tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(tooltip.clone())
                                .build(window, cx)
                        }),
                )
                .child(div().flex_1())
                .child(
                    Button::new("refresh")
                        .ghost()
                        .small()
                        .icon(IconName::RefreshCw)
                        .loading(status.state == PollState::Polling)
                        .tooltip_with_action("Aktualisieren", &Refresh, Some(CONTEXT))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.refresh(&Refresh, window, cx)),
                        ),
                )
                .child(
                    Button::new("notify")
                        .ghost()
                        .small()
                        .icon(if notify {
                            Icon::new(IconName::Bell)
                        } else {
                            Icon::new(Lucide::BellOff)
                        })
                        .selected(notify)
                        .tooltip(if notify {
                            "Benachrichtigungen aus"
                        } else {
                            "Benachrichtigen bei roter/grüner Pipeline und Merge"
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.update_prefs(cx, |p| p.notify = !p.notify)
                        })),
                )
                .child(
                    Button::new("settings")
                        .ghost()
                        .small()
                        .icon(IconName::Settings2)
                        .tooltip_with_action("Repositories…", &OpenSettings, Some(CONTEXT))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_settings(&OpenSettings, window, cx)
                        })),
                ),
        )
    }

    /* ---------- Seitenleiste ---------- */

    fn render_sidebar(&self, d: Option<&Derived>, cx: &mut Context<Self>) -> impl IntoElement {
        let prefs = self.radar.read(cx).prefs().clone();
        let count = |n: usize| {
            move |_: &mut Window, cx: &mut App| {
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(n.to_string())
            }
        };

        let tab_item = |label: &'static str, icon: Icon, tab: Tab, n: usize| {
            SidebarMenuItem::new(label)
                .icon(icon)
                .active(prefs.tab == tab)
                .suffix(count(n))
                .on_click(cx.listener(move |this, _, _, cx| this.show_tab(tab, cx)))
        };
        let views = SidebarMenu::new().children([
            tab_item(
                "Offen",
                Lucide::GitPullRequest.into(),
                Tab::Open,
                d.map_or(0, |d| d.open_list.len()),
            ),
            tab_item(
                "Gemergt",
                Lucide::GitMerge.into(),
                Tab::Merged,
                d.map_or(0, |d| d.merged.len()),
            ),
            tab_item(
                "Releases",
                Lucide::Tag.into(),
                Tab::Releases,
                d.map_or(0, |d| d.releases.len()),
            ),
        ]);

        let quick_item = |label: &'static str, icon: Icon, quick: Quick| {
            SidebarMenuItem::new(label)
                .icon(icon)
                .active(prefs.tab == Tab::Open && prefs.quick == quick)
                .suffix(count(d.map_or(0, |d| d.count(quick))))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.update_prefs(cx, |p| {
                        p.tab = Tab::Open;
                        p.quick = quick;
                    })
                }))
        };
        let filters = SidebarMenu::new().children([
            quick_item("Alle", Lucide::Layers.into(), Quick::All),
            quick_item("Pipeline läuft", Lucide::Hourglass.into(), Quick::Running),
            quick_item("Rot", Lucide::CircleX.into(), Quick::Failed),
            quick_item("Grün", Lucide::CircleCheck.into(), Quick::Passed),
            quick_item("Auto-Merge", Lucide::Zap.into(), Quick::Auto),
            quick_item("Konflikt", Lucide::TriangleAlert.into(), Quick::Conflict),
            quick_item("Dein Review", Lucide::Inbox.into(), Quick::Review),
            quick_item("Von dir", Lucide::CircleUser.into(), Quick::Mine),
        ]);

        let snapshot = d.map(|d| d.snapshot.clone());
        let mut repos = vec![
            SidebarMenuItem::new("Alle Repositories")
                .icon(Lucide::LayoutDashboard)
                .active(prefs.repo_filter.is_empty())
                .on_click(
                    cx.listener(|this, _, _, cx| this.update_prefs(cx, |p| p.repo_filter.clear())),
                ),
        ];
        for full in self.radar.read(cx).repos().to_vec() {
            let info = snapshot.as_ref().and_then(|s| s.repo(&full).cloned());
            let open = snapshot
                .as_ref()
                .map_or(0, |s| s.open.iter().filter(|p| p.repo == full).count());
            let name = full
                .split_once('/')
                .map_or(full.as_str(), |(_, n)| n)
                .to_string();
            let state = info
                .as_ref()
                .map_or(PipelineState::None, |i| i.default_branch_pipeline);
            let failed = info.as_ref().is_some_and(|i| i.error.is_some());
            let key = full.clone();
            repos.push(
                SidebarMenuItem::new(name)
                    .icon(Lucide::BookMarked)
                    .active(prefs.repo_filter.contains(&full))
                    .suffix(move |_, cx: &mut App| {
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(open.to_string()),
                            )
                            .child(if failed {
                                Icon::new(Lucide::TriangleAlert)
                                    .xsmall()
                                    .text_color(cx.theme().danger)
                                    .into_any_element()
                            } else {
                                div()
                                    .size_2()
                                    .rounded_full()
                                    .bg(state_color(state, cx))
                                    .into_any_element()
                            })
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let key = key.clone();
                        this.update_prefs(cx, |p| {
                            if let Some(i) = p.repo_filter.iter().position(|r| *r == key) {
                                p.repo_filter.remove(i);
                            } else {
                                p.repo_filter.push(key);
                            }
                        })
                    })),
            );
        }

        let viewer = snapshot.as_ref().and_then(|s| s.viewer.clone());
        Sidebar::new("nav")
            .w(rems(15.))
            .child(SidebarGroup::new("Ansicht").child(views))
            .child(SidebarGroup::new("Filter").child(filters))
            .child(SidebarGroup::new("Repositories").child(SidebarMenu::new().children(repos)))
            .footer(
                SidebarFooter::new().child(
                    h_flex()
                        .gap_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(Icon::new(Lucide::CircleUser).small())
                        .child(viewer.unwrap_or_else(|| "–".into())),
                ),
            )
    }

    /* ---------- Werkzeugleiste ---------- */

    fn render_toolbar(&self, prefs: &Prefs, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match prefs.tab {
            Tab::Open => "Offene Pull Requests",
            Tab::Merged => "Zuletzt gemergt",
            Tab::Releases => "Releases",
        };
        h_flex()
            .gap_2()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().text_base().font_semibold().child(title))
            .child(div().flex_1())
            .child(
                div().w_64().child(
                    Input::new(&self.search).small().cleanable(true).prefix(
                        Icon::new(IconName::Search)
                            .small()
                            .text_color(cx.theme().muted_foreground),
                    ),
                ),
            )
            .when(prefs.tab == Tab::Open, |this| {
                let flow = prefs.view == View::Flow;
                this.child(
                    ButtonGroup::new("view")
                        .small()
                        .outline()
                        .child(
                            Button::new("view-list")
                                .icon(Lucide::List)
                                .label("Liste")
                                .selected(!flow),
                        )
                        .child(
                            Button::new("view-flow")
                                .icon(Lucide::Workflow)
                                .label("Flow")
                                .selected(flow),
                        )
                        .on_click(cx.listener(|this, clicked: &Vec<usize>, _, cx| {
                            let view = if clicked.contains(&1) {
                                View::Flow
                            } else {
                                View::List
                            };
                            this.update_prefs(cx, |p| p.view = view);
                        })),
                )
                .child(
                    Button::new("grouped")
                        .ghost()
                        .small()
                        .icon(Lucide::Layers)
                        .selected(prefs.grouped)
                        .tooltip_with_action(
                            if prefs.grouped {
                                "Flache Liste"
                            } else {
                                "Nach Repo gruppieren"
                            },
                            &ToggleGrouped,
                            Some(CONTEXT),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.toggle_grouped(&ToggleGrouped, window, cx)
                        })),
                )
            })
    }

    /* ---------- Inhalt ---------- */

    fn render_content(&self, d: Option<&Derived>, cx: &mut Context<Self>) -> AnyElement {
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
            let error = radar.status().error.clone();
            return match error {
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

        match d.prefs.tab {
            Tab::Open => self.render_open(d, cx),
            Tab::Merged => self.render_merged(d, cx),
            Tab::Releases => self.render_releases(d, cx),
        }
    }

    fn render_open(&self, d: &Derived, cx: &mut Context<Self>) -> AnyElement {
        if d.open_list.is_empty() {
            return empty_state(
                IconName::Inbox,
                "Nichts offen",
                Some("Keine Pull Requests für diesen Filter.".into()),
                cx,
            )
            .into_any_element();
        }
        let flow = d.prefs.view == View::Flow;
        if !d.prefs.grouped {
            let prs: Vec<&OpenPr> = d.open_list.iter().map(|&i| &d.snapshot.open[i]).collect();
            return self
                .render_pr_card(&prs, &d.snapshot, true, flow, cx)
                .into_any_element();
        }

        let mut groups: Vec<(String, Vec<&OpenPr>)> = Vec::new();
        for &i in &d.open_list {
            let pr = &d.snapshot.open[i];
            match groups.iter_mut().find(|(r, _)| *r == pr.repo) {
                Some((_, list)) => list.push(pr),
                None => groups.push((pr.repo.clone(), vec![pr])),
            }
        }
        v_flex()
            .gap_6()
            .children(groups.into_iter().map(|(repo, prs)| {
                v_flex()
                    .gap_2()
                    .child(self.render_repo_header(&repo, prs.len(), &d.snapshot, cx))
                    .child(self.render_pr_card(&prs, &d.snapshot, false, flow, cx))
            }))
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

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (snapshot, prefs) = {
            let radar = self.radar.read(cx);
            (radar.snapshot(), radar.prefs().clone())
        };
        let derived = snapshot.map(|s| Derived::new(s, prefs.clone(), &self.query));
        let error = self
            .radar
            .read(cx)
            .status()
            .error
            .clone()
            .filter(|_| derived.is_some());
        let animate = motion::enabled(cx);

        v_flex()
            .size_full()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::focus_search))
            .on_action(cx.listener(Self::refresh))
            .on_action(cx.listener(Self::toggle_view))
            .on_action(cx.listener(Self::toggle_grouped))
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(|this, _: &ShowOpen, _, cx| this.show_tab(Tab::Open, cx)))
            .on_action(cx.listener(|this, _: &ShowMerged, _, cx| this.show_tab(Tab::Merged, cx)))
            .on_action(
                cx.listener(|this, _: &ShowReleases, _, cx| this.show_tab(Tab::Releases, cx)),
            )
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .child(self.render_sidebar(derived.as_ref(), cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(self.render_toolbar(&prefs, cx))
                            .child(
                                v_flex()
                                    .id("content")
                                    .flex_1()
                                    .min_h_0()
                                    .px_4()
                                    .py_4()
                                    .gap_4()
                                    .when_some(error, |this, e| {
                                        this.child(
                                            gpui_kit::component::alert::Alert::error(
                                                "sync-error",
                                                e,
                                            )
                                            .title("Aktualisierung fehlgeschlagen"),
                                        )
                                    })
                                    .child(
                                        // Lesbare Zeilenlänge auch in sehr breiten Fenstern.
                                        div().w_full().max_w(rems(84.)).mx_auto().child(
                                            motion::enter(
                                                SharedString::from(format!(
                                                    "content-{:?}-{:?}",
                                                    prefs.tab, prefs.view
                                                )),
                                                self.render_content(derived.as_ref(), cx),
                                                animate,
                                            ),
                                        ),
                                    )
                                    .overflow_y_scrollbar(),
                            ),
                    ),
            )
    }
}
