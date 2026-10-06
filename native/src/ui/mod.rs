//! Hauptfenster: Titelleiste, Seitenleiste (Ansichten, Filter, Repos) und Inhaltsbereich.

mod content;
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
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    sidebar::{
        Sidebar, SidebarCollapsible, SidebarFooter, SidebarGroup, SidebarMenu, SidebarMenuItem,
    },
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::config::{Prefs, Quick, Tab, View};
use crate::model::{MergeMethod, OpenPr, PipelineState, Snapshot, TransitionKind};
use crate::radar::{PollState, Radar, RadarEvent};
use crate::time::ago;
use parts::state_color;
use pr_row::MergeAction;
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
        ToggleSidebar,
        HideSidebar,
        Quit
    ]
);

const CONTEXT: &str = "Workspace";

/// Breite der Seitenleiste – in px, damit sie beim Ein-/Ausblenden gleitet.
const SIDEBAR_W: Pixels = px(240.);
/// Darunter liegt die Seitenleiste über dem Inhalt statt daneben.
const NARROW_W: Pixels = px(720.);
/// Darunter rücken Steuerelemente in eigene Zeilen (gemessen an der Inhaltsbreite).
const COMPACT_W: Pixels = px(600.);

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
        KeyBinding::new("cmd-b", ToggleSidebar, Some(CONTEXT)),
        KeyBinding::new("escape", HideSidebar, keys),
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
            MenuItem::action("Seitenleiste ein-/ausblenden", ToggleSidebar),
            MenuItem::separator(),
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
    /// Vom Nutzer ausgelöstes Neuladen läuft (nur dann dreht der Spinner)
    manual_refresh: bool,
    /// Virtualisierte Inhaltsliste
    list: ListState,
    list_model: content::ListModel,
    settings: Entity<RepoSettings>,
    /// Schmales Fenster: Seitenleiste liegt als Overlay über dem Inhalt
    narrow: bool,
    /// Overlay-Seitenleiste ist offen (nur im schmalen Fenster, nicht gespeichert)
    sidebar_overlay: bool,
    /// Wenig Platz für den Inhalt: Zeilen und Werkzeugleiste umbrechen
    compact: bool,
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
            cx.observe_in(&radar, window, |this, radar, window, cx| {
                if radar.read(cx).status().state != PollState::Polling {
                    this.manual_refresh = false;
                }
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
            manual_refresh: false,
            list: ListState::new(0, ListAlignment::Top, px(600.)),
            list_model: content::ListModel::default(),
            settings,
            narrow: false,
            sidebar_overlay: false,
            compact: false,
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
            RadarEvent::MergeAllFinished {
                merged,
                auto_merge,
                failed,
            } => {
                let mut parts = Vec::new();
                if *merged > 0 {
                    parts.push(format!("{merged} gemergt"));
                }
                if *auto_merge > 0 {
                    parts.push(format!("{auto_merge} per Auto-Merge"));
                }
                if !failed.is_empty() {
                    parts.push(format!("{} fehlgeschlagen", failed.len()));
                }
                let summary = parts.join(" · ");
                let notification = if failed.is_empty() {
                    Notification::success(summary)
                } else {
                    Notification::error(format!("{summary}\n{}", failed.join("\n")))
                };
                window.push_notification(notification.title("Alle mergen"), cx);
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
        self.manual_refresh = true;
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
        self.sidebar_overlay = false;
        self.update_prefs(cx, |p| p.tab = tab);
    }

    /// Breit: Seitenleiste dauerhaft ein-/ausblenden. Schmal: Overlay öffnen/schließen.
    fn toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        if self.narrow {
            self.sidebar_overlay = !self.sidebar_overlay;
            cx.notify();
        } else {
            self.update_prefs(cx, |p| p.sidebar_hidden = !p.sidebar_hidden);
        }
    }

    fn hide_sidebar(&mut self, _: &HideSidebar, _: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar_overlay {
            self.sidebar_overlay = false;
            cx.notify();
        } else {
            cx.propagate();
        }
    }

    fn sidebar_collapsed(&self, prefs: &Prefs) -> bool {
        if self.narrow {
            !self.sidebar_overlay
        } else {
            prefs.sidebar_hidden
        }
    }

    fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        let settings = self.settings.clone();
        settings.update(cx, |s, cx| s.reset(window, cx));
        window.open_sheet(cx, move |sheet, window, _| {
            sheet
                .title("Repositories")
                .size((window.rem_size() * 26.).min(window.viewport_size().width - px(16.)))
                .child(settings.clone())
        });
    }

    fn toggle_expanded(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(id) {
            self.expanded.insert(id.to_string());
        }
        self.remeasure_key(&format!("pr:{id}"));
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

        let collapsed = self.sidebar_collapsed(radar.prefs());
        TitleBar::new().child(
            h_flex()
                .w_full()
                .min_w_0()
                .pr_2()
                .gap_3()
                .child(
                    Button::new("sidebar")
                        .ghost()
                        .small()
                        .icon(if collapsed {
                            IconName::PanelLeftOpen
                        } else {
                            IconName::PanelLeftClose
                        })
                        .tooltip_with_action(
                            if collapsed {
                                "Seitenleiste einblenden"
                            } else {
                                "Seitenleiste ausblenden"
                            },
                            &ToggleSidebar,
                            Some(CONTEXT),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.toggle_sidebar(&ToggleSidebar, window, cx)
                        })),
                )
                .when(!self.narrow, |this| {
                    this.child(div().text_sm().font_semibold().child("PR Radar"))
                })
                .child(
                    h_flex()
                        .id("live-status")
                        .min_w_0()
                        .gap_1p5()
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .border_1()
                        .border_color(cx.theme().border)
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(motion::heartbeat(
                            div().rounded_full().bg(dot),
                            dot,
                            status
                                .last_success
                                .filter(|_| status.state != PollState::Error)
                                .map(|t| t.timestamp_millis()),
                            cx,
                        ))
                        .child(div().truncate().child(label))
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
                        // Hintergrund-Polls drehen keinen Spinner – jeder Animationsframe baut das Fenster neu auf.
                        .loading(self.manual_refresh && status.state == PollState::Polling)
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
        let collapsed = self.sidebar_collapsed(&prefs);
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
                    this.sidebar_overlay = false;
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
            .w(SIDEBAR_W)
            .collapsible(SidebarCollapsible::Offcanvas)
            .collapsed(collapsed)
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

    /// „Alle mergen“: alle sichtbaren, sofort mergebaren PRs. Mit mehreren erlaubten Methoden
    /// wählt das Menü die bevorzugte; PRs, die sie nicht erlauben, nehmen ihre erste.
    fn render_merge_all(
        &self,
        derived: Option<&Derived>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let d = derived?;
        let radar = self.radar.read(cx);
        let candidates: Vec<(String, Vec<MergeMethod>)> = d
            .open_list
            .iter()
            .map(|&i| &d.snapshot.open[i])
            .filter(|pr| !radar.is_pending(&pr.id))
            .filter_map(|pr| match MergeAction::of(pr, &d.snapshot) {
                MergeAction::MergeNow(methods) => Some((pr.id.clone(), methods)),
                _ => None,
            })
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let count = candidates.len();
        let mut offered: Vec<MergeMethod> = Vec::new();
        for (_, methods) in &candidates {
            for m in methods {
                if !offered.contains(m) {
                    offered.push(*m);
                }
            }
        }
        // Wahl nur nötig, wenn ein PR mehrere Methoden erlaubt
        let choice = candidates.iter().any(|(_, methods)| methods.len() > 1);
        let radar = self.radar.clone();
        let run = move |preferred: Option<MergeMethod>, cx: &mut App| {
            let items = candidates
                .iter()
                .map(|(id, methods)| {
                    let m = preferred
                        .filter(|p| methods.contains(p))
                        .unwrap_or(methods[0]);
                    (id.clone(), m)
                })
                .collect();
            radar.update(cx, |r, cx| r.merge_all(items, cx));
        };

        let compact = self.compact;
        let label = format!("Alle mergen ({count})");
        let button = Button::new("merge-all")
            .small()
            .outline()
            .success()
            .icon(Lucide::GitMerge)
            .when(!compact, |b| b.label(label.clone()))
            .when(compact, |b| b.label(count.to_string()));
        if !choice {
            return Some(
                button
                    .tooltip(format!(
                        "{count} sichtbare PRs jetzt mergen – wartet GitHub noch, wird erneut versucht"
                    ))
                    .on_click(move |_, _, cx| run(None, cx))
                    .into_any_element(),
            );
        }
        let run = std::rc::Rc::new(run);
        Some(
            button
                .dropdown_caret(true)
                .tooltip(label)
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                    let menu = menu.label(format!("{count} PRs mergen mit"));
                    offered.iter().fold(menu, |menu, &m| {
                        let run = run.clone();
                        menu.item(
                            PopupMenuItem::new(m.label())
                                .on_click(move |_, _, cx| run(Some(m), cx)),
                        )
                    })
                })
                .into_any_element(),
        )
    }

    fn render_toolbar(
        &self,
        prefs: &Prefs,
        derived: Option<&Derived>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let title = match prefs.tab {
            Tab::Open => "Offene Pull Requests",
            Tab::Merged => "Zuletzt gemergt",
            Tab::Releases => "Releases",
        };
        let compact = self.compact;
        let search = div()
            .map(|d| if compact { d.w_full() } else { d.w_64() })
            .child(
                Input::new(&self.search).small().cleanable(true).prefix(
                    Icon::new(IconName::Search)
                        .small()
                        .text_color(cx.theme().muted_foreground),
                ),
            );
        let merge_all = (prefs.tab == Tab::Open)
            .then(|| self.render_merge_all(derived, cx))
            .flatten();
        let controls = h_flex().gap_2().flex_shrink_0().children(merge_all).when(
            prefs.tab == Tab::Open,
            |this| {
                let flow = prefs.view == View::Flow;
                this.child(
                    ButtonGroup::new("view")
                        .small()
                        .outline()
                        .child(
                            Button::new("view-list")
                                .icon(Lucide::List)
                                .when(!compact, |b| b.label("Liste"))
                                .when(compact, |b| b.tooltip("Liste"))
                                .selected(!flow),
                        )
                        .child(
                            Button::new("view-flow")
                                .icon(Lucide::Workflow)
                                .when(!compact, |b| b.label("Flow"))
                                .when(compact, |b| b.tooltip("Flow"))
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
            },
        );
        let heading = h_flex().gap_2().min_w_0().child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_base()
                .font_semibold()
                .child(title),
        );

        // Schmal: Titel und Umschalter oben, Suche in voller Breite darunter.
        if compact {
            return v_flex()
                .gap_2()
                .px_4()
                .py_2()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(heading.child(controls))
                .child(search);
        }
        v_flex()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(heading.child(search).child(controls))
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (snapshot, prefs) = {
            let radar = self.radar.read(cx);
            (radar.snapshot(), radar.prefs().clone())
        };

        let width = window.viewport_size().width;
        self.narrow = width < NARROW_W;
        if !self.narrow {
            self.sidebar_overlay = false;
        }
        let docked = !self.narrow && !prefs.sidebar_hidden;
        let content_w = if docked { width - SIDEBAR_W } else { width };
        let compact = content_w < COMPACT_W;
        if compact != self.compact {
            self.compact = compact;
            self.list.remeasure();
        }

        let derived = snapshot.map(|s| Derived::new(s, prefs.clone(), &self.query));
        let error = self
            .radar
            .read(cx)
            .status()
            .error
            .clone()
            .filter(|_| derived.is_some());
        let animate = motion::enabled(cx);
        let content = self.render_content(derived.as_ref(), cx);

        v_flex()
            .size_full()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::focus_search))
            .on_action(cx.listener(Self::refresh))
            .on_action(cx.listener(Self::toggle_view))
            .on_action(cx.listener(Self::toggle_grouped))
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::hide_sidebar))
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
                    .relative()
                    .items_stretch()
                    .when(!self.narrow, |this| {
                        this.child(self.render_sidebar(derived.as_ref(), cx))
                    })
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(self.render_toolbar(&prefs, derived.as_ref(), cx))
                            .when_some(error, |this, e| {
                                this.child(
                                    div().px_4().pt_4().child(
                                        gpui_kit::component::alert::Alert::error("sync-error", e)
                                            .title("Aktualisierung fehlgeschlagen"),
                                    ),
                                )
                            })
                            .child(div().flex_1().min_h_0().child(motion::enter(
                                SharedString::from(format!(
                                    "content-{:?}-{:?}",
                                    prefs.tab, prefs.view
                                )),
                                content,
                                animate,
                            ))),
                    )
                    // Schmal: Seitenleiste gleitet über den Inhalt, Klick daneben schließt sie.
                    .when(self.narrow, |this| {
                        this.when(self.sidebar_overlay, |this| {
                            this.child(
                                div()
                                    .id("sidebar-backdrop")
                                    .absolute()
                                    .inset_0()
                                    .bg(black().opacity(0.25))
                                    .occlude()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            this.sidebar_overlay = false;
                                            cx.notify();
                                        }),
                                    ),
                            )
                        })
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .h_full()
                                // Klicks in der Seitenleiste erreichen den Hintergrund nicht.
                                .occlude()
                                .when(self.sidebar_overlay, |d| d.shadow_lg())
                                .child(self.render_sidebar(derived.as_ref(), cx)),
                        )
                    }),
            )
    }
}
