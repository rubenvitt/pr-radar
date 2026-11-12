//! Hauptfenster: Titelleiste, Seitenleiste (Ansichten, Filter, Repos) und Inhaltsbereich.

mod alerts;
mod content;
mod flow;
mod motion;
mod parts;
mod pr_row;
mod settings;
pub mod theme;
pub mod typography;
mod views;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Selectable as _, Sizable as _, StyledExt as _, TitleBar,
    WindowExt as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    h_flex,
    input::{self, Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    sidebar::{Sidebar, SidebarCollapsible, SidebarFooter, SidebarMenu, SidebarMenuItem},
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::config::{Prefs, Quick, Tab, View};
use crate::model::{MergeMethod, OpenPr, PipelineState, Snapshot, TransitionKind};
use crate::radar::{PollState, Radar, RadarEvent};
use crate::site;
use crate::time::ago;
use parts::{SidebarSection, state_color};
use pr_row::MergeAction;
use settings::RepoSettings;
use typography::TypeScale as _;

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
        Quit,
        CloseWindow,
        Minimize,
        Zoom,
        ToggleFullScreen,
        Hide,
        HideOthers,
        ShowAll,
        ShowMainWindow,
        About,
        OpenHelp,
        OpenLicense,
        OpenWebsite,
        OpenSupport,
        OpenPrivacy,
        ReportIssue,
        OpenThirdPartyNotices,
        RevealLog,
        ToggleLoginItem,
        CheckForUpdates
    ]
);

const REPO_URL: &str = "https://github.com/rubenvitt/pr-radar";

const CONTEXT: &str = "Workspace";

/// Breite der Seitenleiste – in px, damit sie beim Ein-/Ausblenden gleitet.
const SIDEBAR_W: Pixels = px(240.);
/// Darunter liegt die Seitenleiste über dem Inhalt statt daneben.
const NARROW_W: Pixels = px(720.);
/// Darunter rücken Steuerelemente in eigene Zeilen (gemessen an der Inhaltsbreite).
const COMPACT_W: Pixels = px(600.);
/// Breite des Repositories-Sheets: fest in px, damit sie nicht mit der Grundschrift schrumpft
/// (früher 26 rem = 416 px bei 16 px, bei 13 px wären es nur 338 px).
const SHEET_W: Pixels = px(400.);

pub fn init(radar: Entity<Radar>, cx: &mut App) {
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
        // macOS-Standardkürzel: global, damit sie auch im Suchfeld und im
        // Repositories-Sheet greifen (das Sheet liegt außerhalb des Workspace).
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        // ⌘Z/⇧⌘Z/⌘X/⌘C/⌘V/⌘A bindet das Eingabefeld selbst (Kontext „Input“);
        // das Bearbeiten-Menü unten zeigt sie nur an und löst dieselben Aktionen aus.
    ]);

    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &OpenHelp, cx| cx.open_url(&format!("{REPO_URL}#readme")));
    cx.on_action(|_: &OpenLicense, cx| cx.open_url(&format!("{REPO_URL}/blob/main/LICENSE.md")));
    cx.on_action(|_: &OpenWebsite, cx| cx.open_url(&site::url(site::Page::Home)));
    cx.on_action(|_: &OpenSupport, cx| cx.open_url(&site::url(site::Page::Support)));
    cx.on_action(|_: &OpenPrivacy, cx| cx.open_url(&site::url(site::Page::Privacy)));
    // Support läuft über GitHub Issues; die Vorlagen-Auswahl fragt App und Version ab.
    cx.on_action(|_: &ReportIssue, cx| cx.open_url(&format!("{REPO_URL}/issues/new/choose")));
    cx.on_action(|_: &OpenThirdPartyNotices, cx| open_third_party_notices(cx));
    cx.on_action(|_: &RevealLog, cx| cx.reveal_path(crate::logging::log_path()));
    cx.on_action(|_: &CloseWindow, cx| {
        with_active_window(cx, |window, cx| {
            // Erst ein offenes Sheet (Repositories) schließen, dann das Fenster.
            if window.has_active_sheet(cx) {
                window.close_sheet(cx);
            } else {
                window.remove_window();
            }
        })
    });
    cx.on_action(|_: &Minimize, cx| with_active_window(cx, |window, _| window.minimize_window()));
    cx.on_action(|_: &Zoom, cx| with_active_window(cx, |window, _| window.zoom_window()));
    cx.on_action(|_: &ToggleFullScreen, cx| {
        with_active_window(cx, |window, _| window.toggle_fullscreen())
    });
    cx.on_action(|_: &About, cx| with_active_window(cx, open_about));
    cx.on_action(|_: &CheckForUpdates, _| crate::updater::check_for_updates());
    // Systembenachrichtigungen auch ohne offenes Fenster (nach ⌘W).
    alerts::init(radar.clone(), cx);
    cx.set_global(MainRadar(radar));
    cx.on_action(|_: &ShowMainWindow, cx| show_main_window(cx));

    #[cfg(target_os = "macos")]
    cx.on_action(|_: &ToggleLoginItem, cx| crate::menubar::toggle_login_item(cx));
    refresh_menus(cx);
}

/// Setzt die Menüleiste der App (erneut, sobald sich ein Häkchen ändert).
pub fn refresh_menus(cx: &mut App) {
    cx.set_menus(vec![
        Menu::new("PR Radar").items([
            MenuItem::action(tr!("About PR Radar", "Über PR Radar"), About),
            MenuItem::action(
                tr!("Check for Updates…", "Nach Updates suchen …"),
                CheckForUpdates,
            )
            .disabled(!crate::updater::available()),
            MenuItem::separator(),
            MenuItem::action(tr!("Repositories…", "Repositories…"), OpenSettings),
            login_menu_item(),
            MenuItem::separator(),
            MenuItem::os_submenu(tr!("Services", "Dienste"), SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action(tr!("Hide PR Radar", "PR Radar ausblenden"), Hide),
            MenuItem::action(tr!("Hide Others", "Andere ausblenden"), HideOthers),
            MenuItem::action(tr!("Show All", "Alle einblenden"), ShowAll),
            MenuItem::separator(),
            MenuItem::action(tr!("Quit PR Radar", "PR Radar beenden"), Quit),
        ]),
        Menu::new(tr!("File", "Ablage")).items([
            MenuItem::action(tr!("Refresh", "Aktualisieren"), Refresh),
            MenuItem::separator(),
            MenuItem::action(tr!("Close", "Schließen"), CloseWindow),
        ]),
        Menu::new(tr!("Edit", "Bearbeiten")).items([
            MenuItem::os_action(tr!("Undo", "Widerrufen"), input::Undo, OsAction::Undo),
            MenuItem::os_action(tr!("Redo", "Wiederholen"), input::Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action(tr!("Cut", "Ausschneiden"), input::Cut, OsAction::Cut),
            MenuItem::os_action(tr!("Copy", "Kopieren"), input::Copy, OsAction::Copy),
            MenuItem::os_action(tr!("Paste", "Einsetzen"), input::Paste, OsAction::Paste),
            MenuItem::os_action(
                tr!("Select All", "Alles auswählen"),
                input::SelectAll,
                OsAction::SelectAll,
            ),
            MenuItem::separator(),
            MenuItem::action(tr!("Find", "Suchen"), FocusSearch),
        ]),
        Menu::new(tr!("View", "Ansicht")).items([
            MenuItem::action(
                tr!("Show/Hide Sidebar", "Seitenleiste ein-/ausblenden"),
                ToggleSidebar,
            ),
            MenuItem::separator(),
            MenuItem::action(tr!("Open", "Offen"), ShowOpen),
            MenuItem::action(tr!("Merged", "Gemergt"), ShowMerged),
            MenuItem::action(tr!("Releases", "Releases"), ShowReleases),
            MenuItem::separator(),
            MenuItem::action(
                tr!("Toggle List / Flow", "Liste / Flow umschalten"),
                ToggleView,
            ),
            MenuItem::action(tr!("Group by Repo", "Nach Repo gruppieren"), ToggleGrouped),
            MenuItem::separator(),
            MenuItem::action(
                tr!("Enter/Exit Full Screen", "Vollbildmodus ein/aus"),
                ToggleFullScreen,
            ),
        ]),
        Menu::new(tr!("Window", "Fenster")).items([
            MenuItem::action(tr!("Minimize", "Im Dock ablegen"), Minimize),
            MenuItem::action(tr!("Zoom", "Zoomen"), Zoom),
            MenuItem::separator(),
            MenuItem::action("PR Radar", ShowMainWindow),
        ]),
        Menu::new(tr!("Help", "Hilfe")).items([
            MenuItem::action(tr!("PR Radar Website", "PR-Radar-Website"), OpenWebsite),
            MenuItem::action(tr!("Support", "Support"), OpenSupport),
            MenuItem::action(tr!("Report a Problem…", "Problem melden …"), ReportIssue),
            MenuItem::separator(),
            MenuItem::action(tr!("Privacy Policy", "Datenschutz"), OpenPrivacy),
            MenuItem::separator(),
            MenuItem::action(tr!("PR Radar on GitHub", "PR Radar auf GitHub"), OpenHelp),
            MenuItem::action(
                tr!("Source Code License", "Lizenz des Quellcodes"),
                OpenLicense,
            ),
            MenuItem::action(
                tr!("Third-Party Licenses", "Drittanbieter-Lizenzen"),
                OpenThirdPartyNotices,
            ),
            MenuItem::separator(),
            MenuItem::action(
                tr!("Show Log in Finder", "Protokoll im Finder zeigen"),
                RevealLog,
            ),
        ]),
    ]);
}

#[cfg(target_os = "macos")]
fn login_menu_item() -> MenuItem {
    MenuItem::action(
        tr!("Open at Login", "Beim Anmelden starten"),
        ToggleLoginItem,
    )
    .checked(crate::menubar::login_item_checked())
}

#[cfg(not(target_os = "macos"))]
fn login_menu_item() -> MenuItem {
    MenuItem::action(
        tr!("Open at Login", "Beim Anmelden starten"),
        ToggleLoginItem,
    )
    .disabled(true)
}

/// Zeigt einen Fehler im Hauptfenster (öffnet es bei Bedarf).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn show_error(title: &'static str, message: String, cx: &mut App) {
    show_main_window(cx);
    with_active_window(cx, move |window, cx| {
        window.push_notification(Notification::error(message).title(title), cx);
    });
}

/// Vibrancy wie in macOS-Seitenleisten: Das Fenster ist durchscheinend mit unscharfem
/// Hintergrund, nur die Seitenleiste lässt ihn (leicht getönt) durch; Titelleiste und
/// Inhalt bleiben deckend. Andere Systeme behalten das deckende Fenster.
const VIBRANCY: bool = cfg!(target_os = "macos");
/// Deckkraft der Seitenleiste über dem unscharfen Fensterhintergrund.
const SIDEBAR_TINT: f32 = 0.88;

/// Schnellfilter mit Akzenttext: nur der aktive, nur in „Offen“ (dort wirkt er) und nicht
/// die Vorgabe „Alle“, die gar nicht filtert.
fn quick_marked(prefs: &Prefs, quick: Quick) -> bool {
    prefs.tab == Tab::Open && prefs.quick == quick && quick != Quick::All
}

/// Zähler in der Seitenleiste: dezenter Text rechts.
fn count_text(n: usize, cx: &App) -> Div {
    div()
        .text_small()
        .text_color(cx.theme().muted_foreground)
        .child(n.to_string())
}

/// Zähler als Suffix einer Seitenleisten-Zeile; 0 erscheint gar nicht.
fn count_badge(n: usize) -> Option<impl Fn(&mut Window, &mut App) -> Div + 'static> {
    (n > 0).then_some(move |_: &mut Window, cx: &mut App| count_text(n, cx))
}

/// Repo-Status in fester Breite, damit Punkt und Warnsymbol bündig stehen: Punkt in der
/// Pipeline-Farbe des Standard-Branches, Warnsymbol bei einem Abruffehler.
fn repo_status(failed: bool, state: PipelineState, cx: &App) -> Div {
    h_flex()
        .flex_none()
        .w_3()
        .justify_center()
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
}

/// Öffnet das Hauptfenster.
pub fn open_main_window(radar: Entity<Radar>, cx: &mut App) -> anyhow::Result<()> {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(1240.), px(820.)), cx)),
        window_min_size: Some(size(px(360.), px(480.))),
        window_background: if VIBRANCY {
            WindowBackgroundAppearance::Blurred
        } else {
            WindowBackgroundAppearance::Opaque
        },
        ..TitleBar::window_options()
    };
    // Demo mit --dark/--light: Erscheinungsbild bleibt fest, statt dem System zu folgen.
    let forced = cx
        .try_global::<crate::demo::Options>()
        .is_some_and(|d| d.appearance.is_some());
    let (handle, _) = gpui_kit::open_window(options, cx, move |window, cx| {
        if !forced {
            window
                .observe_window_appearance(|window, cx| {
                    gpui_kit::component::Theme::sync_system_appearance(Some(window), cx)
                })
                .detach();
        }
        cx.new(|cx| Workspace::new(radar, window, cx))
    })?;
    if VIBRANCY {
        // Die Wurzel von gpui-kit malt sonst den Theme-Hintergrund übers ganze Fenster.
        handle.update(cx, |_, window, cx| {
            gpui_kit::base::Root::update(window, cx, |root, _, cx| {
                root.style().background = Some(transparent_black().into());
                cx.notify();
            })
        })?;
    }
    Ok(())
}

/// Hält den Radar über das Hauptfenster hinaus, damit es nach ⌘W wieder öffnen kann.
struct MainRadar(Entity<Radar>);

impl Global for MainRadar {}

/// Holt das Hauptfenster nach vorn oder öffnet es neu, falls es mit ⌘W geschlossen
/// wurde (Menü „Fenster“ oder Klick aufs Dock-Symbol).
pub fn show_main_window(cx: &mut App) {
    match cx.windows().first().copied() {
        Some(handle) => {
            handle
                .update(cx, |_, window, _| window.activate_window())
                .ok();
        }
        None => {
            let Some(radar) = cx.try_global::<MainRadar>().map(|r| r.0.clone()) else {
                return;
            };
            if let Err(e) = open_main_window(radar, cx) {
                log::error!("Fenster konnte nicht wieder geöffnet werden: {e:#}");
            }
        }
    }
    cx.activate(true);
}

/// Führt `f` mit dem aktiven Fenster aus. Globale Aktionen laufen, während das
/// Fenster noch verliehen ist, daher erst im nächsten Effekt-Durchlauf.
fn with_active_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    cx.defer(move |cx| {
        let Some(handle) = cx.active_window().or_else(|| cx.windows().first().copied()) else {
            return;
        };
        handle.update(cx, |_, window, cx| f(window, cx)).ok();
    });
}

/// Öffnet die Drittanbieter-Hinweise aus dem App-Bundle; unter `cargo run` gibt es
/// sie nicht, dann den Lizenzabschnitt im README.
fn open_third_party_notices(cx: &mut App) {
    match crate::notices::locate() {
        Some(path) => cx.open_with_system(&path),
        None => {
            log::warn!(
                "{} fehlt (nur im App-Bundle aus scripts/bundle.sh), öffne das README",
                crate::notices::FILE_NAME
            );
            cx.open_url(&format!("{REPO_URL}#lizenz"));
        }
    }
}

fn open_about(window: &mut Window, cx: &mut App) {
    window.open_alert_dialog(cx, |dialog, _, _| {
        dialog
            .title("PR Radar")
            .description(trf!(
                "Version {}\n\nPull requests, pipelines, auto-merge, merges and releases across \
                 multiple GitHub repos.\n\nLicense: Functional Source License 1.1, ALv2 Future License.",
                "Version {}\n\nPull Requests, Pipelines, Auto-Merge, Merges und Releases über \
                 mehrere GitHub-Repos.\n\nLizenz: Functional Source License 1.1, ALv2 Future License.",
                env!("CARGO_PKG_VERSION")
            ))
    });
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
    /// Releases, deren Notes höher als die Kappung sind (gemessen beim Zeichnen)
    long_notes: HashSet<String>,
    /// Releases, deren Notes per „Mehr anzeigen“ vollständig zu sehen sind
    full_notes: HashSet<String>,
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

/// Name eines Schnellfilters, wie er in der Seitenleiste steht.
fn quick_label(quick: Quick) -> &'static str {
    match quick {
        Quick::All => tr!("All", "Alle"),
        Quick::Running => tr!("Pipeline running", "Pipeline läuft"),
        Quick::Failed => tr!("Failed", "Rot"),
        Quick::Passed => tr!("Passed", "Grün"),
        Quick::Auto => tr!("Auto-merge", "Auto-Merge"),
        Quick::Conflict => tr!("Conflict", "Konflikt"),
        Quick::Review => tr!("Your review", "Dein Review"),
        Quick::Mine => tr!("By you", "Von dir"),
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
                .placeholder(tr!("Search…", "Suchen …"))
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
                    .map(|s| s.failed().count())
                    .unwrap_or(0);
                window.set_window_title(&match failed {
                    0 => "PR Radar".to_string(),
                    n => trf!("PR Radar – {n} failed", "PR Radar – {n} rot"),
                });
                cx.notify();
            }),
            cx.subscribe_in(&radar, window, Self::on_radar_event),
            // Hängt der Fokus an keinem Element mehr, zum nächsten noch sichtbaren
            // fokussierbaren Vorfahren (etwa das offene Sheet, wenn dort ein fokussierter
            // Knopf verschwindet), sonst in den Workspace. Ohne das greifen ⌘F, ⌘1–3 usw.
            // nicht, weil sie nur im Kontext „Workspace“ gebunden sind.
            cx.on_focus_lost(window, |this, window, cx| {
                let target = window
                    .focus_lost_restore_target(cx)
                    .unwrap_or_else(|| this.focus.clone());
                target.focus(window, cx);
            }),
        ];

        // Defekte config.json: einmal deutlich melden (nach dem Aufbau des Fensters).
        if let Some(warning) = radar.update(cx, |radar, _| radar.take_config_warning()) {
            cx.defer_in(window, move |_, window, cx| {
                window.push_notification(
                    Notification::warning(warning)
                        .title(tr!("Settings reset", "Konfiguration zurückgesetzt"))
                        .autohide(false),
                    cx,
                );
            });
        }

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
            long_notes: HashSet::new(),
            full_notes: HashSet::new(),
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
            RadarEvent::DemoNotice(message) => {
                window.push_notification(
                    Notification::info(message.clone()).title(tr!("Demo Mode", "Demo-Modus")),
                    cx,
                );
            }
            RadarEvent::ActionFailed(message) => {
                window.push_notification(
                    Notification::error(message.clone())
                        .title(tr!("Action failed", "Aktion fehlgeschlagen")),
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
                    parts.push(trf!("{merged} merged", "{merged} gemergt"));
                }
                if *auto_merge > 0 {
                    parts.push(trf!(
                        "{auto_merge} via auto-merge",
                        "{auto_merge} per Auto-Merge"
                    ));
                }
                if !failed.is_empty() {
                    parts.push(trf!("{} failed", "{} fehlgeschlagen", failed.len()));
                }
                let summary = parts.join(" · ");
                let notification = if failed.is_empty() {
                    Notification::success(summary)
                } else {
                    Notification::error(format!("{summary}\n{}", failed.join("\n")))
                };
                window.push_notification(notification.title(tr!("Merge all", "Alle mergen")), cx);
            }
            RadarEvent::Transitions(transitions) => {
                if !self.radar.read(cx).prefs().notify {
                    return;
                }
                // Nur der Toast im Fenster; die Systembenachrichtigung schickt
                // `alerts` auf App-Ebene, auch wenn kein Fenster offen ist.
                for t in transitions {
                    let alert = alerts::Alert::new(t);
                    let notification = match t.kind {
                        TransitionKind::Failed => Notification::error(alert.body),
                        TransitionKind::Passed => Notification::success(alert.body),
                        TransitionKind::Merged => Notification::info(alert.body),
                    };
                    let (url, tag) = (alert.url, alert.tag);
                    window.push_notification(
                        notification
                            .id1::<Workspace>(SharedString::from(tag.clone()))
                            .title(alert.title)
                            .on_click(move |_, _, cx| {
                                cx.dismiss_system_notification(&tag);
                                cx.open_url(&url);
                            }),
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

    /// Schnellfilter, Repo-Auswahl und Suche zurücksetzen; die Ansicht bleibt.
    fn reset_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |s, cx| s.set_value("", window, cx));
        self.query.clear();
        self.update_prefs(cx, |p| {
            p.quick = Quick::All;
            p.repo_filter.clear();
        });
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
        // Erst öffnen, dann das Eingabefeld fokussieren: Das Sheet merkt sich beim Öffnen
        // den aktuellen Fokus und gibt ihn beim Schließen zurück. Andersherum wäre das
        // Eingabefeld (danach nicht mehr sichtbar) das Ziel, und kein Kürzel griffe mehr.
        window.open_sheet(cx, {
            let settings = settings.clone();
            move |sheet, window, _| {
                sheet
                    .title(tr!("Repositories", "Repositories"))
                    .size(SHEET_W.min(window.viewport_size().width - px(16.)))
                    .child(settings.clone())
            }
        });
        settings.update(cx, |s, cx| s.reset(window, cx));
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
        // Ohne Repos fragt PR Radar nichts ab – dann auch kein „Live“.
        let no_repos = radar.repos().is_empty();
        let demo = radar.is_demo();

        let (dot, label) = match status.state {
            _ if demo => (
                cx.theme().info,
                tr!("Sample data", "Beispieldaten").to_string(),
            ),
            _ if no_repos => (
                cx.theme().muted_foreground,
                tr!("No repositories", "Keine Repositories").to_string(),
            ),
            PollState::Error => (cx.theme().danger, tr!("Error", "Fehler").to_string()),
            PollState::Polling if loading => {
                (cx.theme().warning, tr!("Loading…", "Lädt …").to_string())
            }
            _ => (
                cx.theme().success,
                trf!("Live · {}", "Live · {}", ago(status.last_success)),
            ),
        };
        let tooltip: SharedString = match status.last_success {
            _ if demo => tr!(
                "Demo mode: fixed sample data, no connection to GitHub",
                "Demo-Modus: feste Beispieldaten, keine Verbindung zu GitHub"
            )
            .into(),
            _ if no_repos => tr!(
                "Add repositories for PR Radar to watch",
                "Füge Repositories hinzu, damit PR Radar sie beobachtet"
            )
            .into(),
            Some(t) => trf!(
                "Last updated {} · every {} s",
                "Letzte Aktualisierung {} · alle {} s",
                crate::time::clock(t),
                status.interval.as_secs()
            )
            .into(),
            None => tr!("Not updated yet", "Noch nicht aktualisiert").into(),
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
                                tr!("Show Sidebar", "Seitenleiste einblenden")
                            } else {
                                tr!("Hide Sidebar", "Seitenleiste ausblenden")
                            },
                            &ToggleSidebar,
                            Some(CONTEXT),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.toggle_sidebar(&ToggleSidebar, window, cx)
                        })),
                )
                .when(demo, |this| {
                    this.child(
                        Tag::info()
                            .xsmall()
                            .text_small()
                            .rounded_full()
                            .flex_none()
                            .child("Demo"),
                    )
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
                        .text_small()
                        .text_color(cx.theme().muted_foreground)
                        .child(motion::heartbeat(
                            div().rounded_full().bg(dot),
                            dot,
                            status
                                .last_success
                                .filter(|_| status.state != PollState::Error && !no_repos)
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
                        .tooltip_with_action(
                            tr!("Refresh", "Aktualisieren"),
                            &Refresh,
                            Some(CONTEXT),
                        )
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
                            tr!("Turn off notifications", "Benachrichtigungen aus")
                        } else {
                            tr!(
                                "Notify when a pipeline fails or passes and on merge",
                                "Benachrichtigen bei roter/grüner Pipeline und Merge"
                            )
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
                        .tooltip_with_action(
                            tr!("Repositories…", "Repositories…"),
                            &OpenSettings,
                            Some(CONTEXT),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_settings(&OpenSettings, window, cx)
                        })),
                ),
        )
    }

    /* ---------- Seitenleiste ---------- */

    /// Seitenleiste. Genau eine Zeile ist flächig hervorgehoben: die aktuelle Ansicht.
    /// Aktive Schnell- und Repo-Filter tragen nur Akzenttext; die Vorgaben („Alle“,
    /// „Alle Repositories“) bleiben schlicht, solange nichts gefiltert ist (PRR-54).
    ///
    /// `floating`: Die Leiste liegt im schmalen Fenster über dem Inhalt und braucht dann
    /// einen deckenden Hintergrund, sonst schiene der Inhalt durch.
    fn render_sidebar(
        &self,
        d: Option<&Derived>,
        floating: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let prefs = self.radar.read(cx).prefs().clone();
        // Ohne Repos gibt es nichts zu zählen oder zu filtern: Zähler und Filter weglassen.
        let no_repos = self.radar.read(cx).repos().is_empty();
        let collapsed = self.sidebar_collapsed(&prefs);
        let accent = cx.theme().primary;
        // Aktiver Filter: Akzenttext statt Fläche; der Screenreader hört „ausgewählt“.
        let filter_selected = move |item: SidebarMenuItem, label: &str, selected: bool| {
            item.when(selected, |item| {
                item.font_medium()
                    .text_color(accent)
                    .accessibility_label(trf!("{}, selected", "{}, ausgewählt", label))
            })
        };

        let tab_item = |label: &'static str, icon: Icon, tab: Tab, n: usize| {
            SidebarMenuItem::new(label)
                .text_body()
                .icon(icon)
                .active(prefs.tab == tab)
                .when(!no_repos, |item| {
                    item.when_some(count_badge(n), |i, b| i.suffix(b))
                })
                .on_click(cx.listener(move |this, _, _, cx| this.show_tab(tab, cx)))
        };
        let views = SidebarMenu::new().children([
            tab_item(
                tr!("Open", "Offen"),
                Lucide::GitPullRequest.into(),
                Tab::Open,
                d.map_or(0, |d| d.open_list.len()),
            ),
            tab_item(
                tr!("Merged", "Gemergt"),
                Lucide::GitMerge.into(),
                Tab::Merged,
                d.map_or(0, |d| d.merged.len()),
            ),
            tab_item(
                tr!("Releases", "Releases"),
                Lucide::Tag.into(),
                Tab::Releases,
                d.map_or(0, |d| d.releases.len()),
            ),
        ]);

        let quick_item = |icon: Icon, quick: Quick| {
            let label = quick_label(quick);
            let selected = quick_marked(&prefs, quick);
            filter_selected(
                SidebarMenuItem::new(label).text_body().icon(icon),
                label,
                selected,
            )
            .when_some(count_badge(d.map_or(0, |d| d.count(quick))), |i, b| {
                i.suffix(b)
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.sidebar_overlay = false;
                this.update_prefs(cx, |p| {
                    p.tab = Tab::Open;
                    p.quick = quick;
                })
            }))
        };
        let filters = SidebarMenu::new().children([
            quick_item(Lucide::Layers.into(), Quick::All),
            quick_item(Lucide::Hourglass.into(), Quick::Running),
            quick_item(Lucide::CircleX.into(), Quick::Failed),
            quick_item(Lucide::CircleCheck.into(), Quick::Passed),
            quick_item(Lucide::Zap.into(), Quick::Auto),
            quick_item(Lucide::TriangleAlert.into(), Quick::Conflict),
            quick_item(Lucide::Inbox.into(), Quick::Review),
            quick_item(Lucide::CircleUser.into(), Quick::Mine),
        ]);

        let snapshot = d.map(|d| d.snapshot.clone());
        let mut repos =
            vec![if no_repos {
                SidebarMenuItem::new(tr!("Add…", "Hinzufügen…"))
                    .text_body()
                    .icon(IconName::Plus)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_settings(&OpenSettings, window, cx)
                    }))
            } else {
                SidebarMenuItem::new(tr!("All Repositories", "Alle Repositories"))
                    .text_body()
                    .icon(Lucide::LayoutDashboard)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.update_prefs(cx, |p| p.repo_filter.clear())
                    }))
            }];
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
            let selected = prefs.repo_filter.contains(&full);
            repos.push(
                filter_selected(
                    SidebarMenuItem::new(name.clone())
                        .text_body()
                        .icon(Lucide::BookMarked),
                    &name,
                    selected,
                )
                .suffix(move |_, cx: &mut App| {
                    h_flex()
                        .gap_2()
                        .when(open > 0, |this| this.child(count_text(open, cx)))
                        .child(repo_status(failed, state, cx))
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
            .when(VIBRANCY, |this| {
                // Kräftig getönt mit nur einem Hauch Glas: Bei 60 % schien der Schreibtisch
                // zu stark durch (Rückmeldung zu PRR-54). Schwebend (schmales Fenster)
                // ganz deckend, sonst schiene der Inhalt durch.
                let bg = cx.theme().sidebar;
                this.bg(if floating {
                    bg
                } else {
                    bg.opacity(SIDEBAR_TINT)
                })
            })
            .collapsible(SidebarCollapsible::Offcanvas)
            .collapsed(collapsed)
            .child(SidebarSection::new(tr!("View", "Ansicht"), views))
            .when(!no_repos, |this| {
                this.child(SidebarSection::new(tr!("Filter", "Filter"), filters))
            })
            .child(SidebarSection::new(
                tr!("Repositories", "Repositories"),
                SidebarMenu::new().children(repos),
            ))
            .footer(
                SidebarFooter::new().child(
                    h_flex()
                        .gap_2()
                        .text_small()
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
        let label = trf!("Merge all ({count})", "Alle mergen ({count})");
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
                    .tooltip(trf!(
                        "Merge {count} visible PRs now – retried if GitHub is still waiting",
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
                    let menu = menu.label(trf!("Merge {count} PRs with", "{count} PRs mergen mit"));
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
            Tab::Open => tr!("Open Pull Requests", "Offene Pull Requests"),
            Tab::Merged => tr!("Recently Merged", "Zuletzt gemergt"),
            Tab::Releases => tr!("Releases", "Releases"),
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
                                .when(!compact, |b| b.label(tr!("List", "Liste")))
                                .when(compact, |b| b.tooltip(tr!("List", "Liste")))
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
                                tr!("Flat List", "Flache Liste")
                            } else {
                                tr!("Group by Repo", "Nach Repo gruppieren")
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
                .text_title()
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
            .when(!VIBRANCY, |this| this.bg(cx.theme().background))
            .text_color(cx.theme().foreground)
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .items_stretch()
                    .when(!self.narrow, |this| {
                        this.child(self.render_sidebar(derived.as_ref(), false, cx))
                    })
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .bg(cx.theme().background)
                            .child(self.render_toolbar(&prefs, derived.as_ref(), cx))
                            .when_some(error, |this, e| {
                                this.child(
                                    div().px_4().pt_4().child(
                                        gpui_kit::component::alert::Alert::error("sync-error", e)
                                            .title(tr!(
                                                "Update failed",
                                                "Aktualisierung fehlgeschlagen"
                                            )),
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
                                .child(self.render_sidebar(derived.as_ref(), true, cx)),
                        )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui_kit::component::WindowExt as _;
    use gpui_kit::http_client::BlockedHttpClient;
    use gpui_kit::test::{TestAppContextExt as _, TestWindowExt as _};
    use gpui_kit::{
        AnyWindowHandle, AppContext as _, Bounds, Entity, Focusable as _, Point, TestAppContext,
        WindowBounds, WindowOptions, px, size,
    };

    // Kein `use super::*`: Der Glob brächte GPUIs `test`-Makro mit und verdeckte `#[test]`.
    use super::{Radar, Tab, Workspace, init};
    use crate::github::GitHub;
    use std::sync::Arc;

    const WAIT: Duration = Duration::from_secs(1);

    /// Hauptfenster wie in der App (Kürzel aus `init`), aber ohne Datei und Netz.
    fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>, Entity<Radar>) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            let github = Arc::new(GitHub::new(Arc::new(BlockedHttpClient::new())));
            let radar = cx.new(|_| Radar::for_tests(github));
            init(radar.clone(), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(1000.), px(700.)),
                })),
                ..Default::default()
            };
            let (handle, workspace) = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Workspace::new(radar.clone(), window, cx))
            })
            .expect("Testfenster");
            (handle, workspace, radar)
        })
    }

    fn press(cx: &mut TestAppContext, handle: AnyWindowHandle, key: &str) {
        cx.update_window(handle, |_, window, cx| window.press(key, cx))
            .unwrap();
        cx.run_until_parked();
    }

    fn tab(cx: &mut TestAppContext, radar: &Entity<Radar>) -> Tab {
        cx.read(|cx| radar.read(cx).prefs().tab)
    }

    fn workspace_focused(
        cx: &mut TestAppContext,
        handle: AnyWindowHandle,
        workspace: &Entity<Workspace>,
    ) -> bool {
        cx.update_window(handle, |_, window, cx| {
            workspace.read(cx).focus.contains_focused(window, cx)
        })
        .unwrap()
    }

    #[gpui_kit::test]
    async fn kuerzel_greifen_nach_dem_repositories_sheet(cx: &mut TestAppContext) {
        let (handle, workspace, radar) = open(cx);
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        press(cx, handle, "cmd-2");
        assert_eq!(tab(cx, &radar), Tab::Merged);

        // ⌘, öffnet das Sheet, Escape schließt es wieder.
        press(cx, handle, "cmd-,");
        cx.wait_for(handle, WAIT, |window, cx| window.has_active_sheet(cx))
            .await;
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.press("escape", cx);
        })
        .unwrap();
        cx.wait_for(handle, WAIT, |window, cx| !window.has_active_sheet(cx))
            .await;

        assert!(
            workspace_focused(cx, handle, &workspace),
            "nach dem Schließen des Sheets muss der Fokus wieder im Workspace liegen"
        );
        press(cx, handle, "cmd-3");
        assert_eq!(tab(cx, &radar), Tab::Releases);
        press(cx, handle, "cmd-1");
        assert_eq!(tab(cx, &radar), Tab::Open);
    }

    #[gpui_kit::test]
    async fn erster_start_und_repo_auswahl_rendern(cx: &mut TestAppContext) {
        use crate::github::{RepoChoice, TokenSource};
        use crate::radar::RepoChoices;
        let (handle, _, radar) = open(cx);
        // Schritt 1: nicht angemeldet; Schritt 2: angemeldet, Liste geladen.
        for (choices, source) in [
            (RepoChoices::Failed("nicht angemeldet".into()), None),
            (RepoChoices::Loading, Some(TokenSource::OAuth)),
            (
                RepoChoices::Loaded(Arc::new(vec![
                    RepoChoice {
                        full: "me/app".into(),
                        private: true,
                        description: Some("Die App".into()),
                    },
                    RepoChoice {
                        full: "org/lib".into(),
                        private: false,
                        description: None,
                    },
                ])),
                Some(TokenSource::OAuth),
            ),
        ] {
            cx.update(|cx| {
                radar.update(cx, |r, cx| {
                    r.set_repo_choices_for_tests(choices, source, cx)
                })
            });
            cx.update_window(handle, |_, window, cx| window.render_frame(cx))
                .unwrap();
        }
        // Im Sheet: Liste mit Suchfeld; die Auswahl bleibt geladen und wird nicht neu geholt.
        press(cx, handle, "cmd-,");
        cx.wait_for(handle, WAIT, |window, cx| window.has_active_sheet(cx))
            .await;
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        let loaded = cx.update(|cx| {
            matches!(radar.read(cx).repo_choices(), RepoChoices::Loaded(list) if list.len() == 2)
        });
        assert!(
            loaded,
            "geöffnetes Sheet darf eine geladene Auswahl nicht verwerfen"
        );
    }

    #[gpui_kit::test]
    async fn sheet_fokussiert_das_eingabefeld(cx: &mut TestAppContext) {
        let (handle, workspace, _) = open(cx);
        press(cx, handle, "cmd-,");
        cx.wait_for(handle, WAIT, |window, cx| window.has_active_sheet(cx))
            .await;
        let input_focused = cx
            .update_window(handle, |_, window, cx| {
                let settings = workspace.read(cx).settings.clone();
                settings.read(cx).input_focused(window, cx)
            })
            .unwrap();
        assert!(
            input_focused,
            "das Eingabefeld im Sheet soll den Fokus haben"
        );
    }

    #[gpui_kit::test]
    async fn klick_in_die_seitenleiste_gibt_den_fokus_an_den_workspace(cx: &mut TestAppContext) {
        let (handle, workspace, radar) = open(cx);
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        // ⌘F: Fokus im Suchfeld, dort tippen Einzeltasten Text statt Kürzel auszulösen.
        press(cx, handle, "cmd-f");
        let search_focused = cx
            .update_window(handle, |_, window, cx| {
                workspace
                    .read(cx)
                    .search
                    .focus_handle(cx)
                    .is_focused(window)
            })
            .unwrap();
        assert!(search_focused);

        // Klick auf „Gemergt“ in der Seitenleiste holt den Fokus aus dem Suchfeld.
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.within("nav").click("0-0-1", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(tab(cx, &radar), Tab::Merged);
        assert!(workspace_focused(cx, handle, &workspace));
        press(cx, handle, "3");
        assert_eq!(tab(cx, &radar), Tab::Releases);
        press(cx, handle, "cmd-1");
        assert_eq!(tab(cx, &radar), Tab::Open);
    }

    /// PRR-54: Nur die Ansicht ist flächig hervorgehoben (`active`); ein Schnellfilter
    /// bekommt Akzenttext nur, wenn er tatsächlich filtert.
    #[test]
    fn schnellfilter_markierung() {
        use super::quick_marked;
        use crate::config::{Prefs, Quick};
        let mut prefs = Prefs::default();
        assert!(!quick_marked(&prefs, Quick::All), "Vorgabe bleibt schlicht");
        prefs.quick = Quick::Failed;
        assert!(quick_marked(&prefs, Quick::Failed));
        assert!(!quick_marked(&prefs, Quick::Passed));
        prefs.tab = Tab::Merged;
        assert!(
            !quick_marked(&prefs, Quick::Failed),
            "Schnellfilter gelten nur für „Offen“"
        );
    }

    /// PRR-54: Zeilenraster 28 px wie in macOS-Seitenleisten.
    #[gpui_kit::test]
    async fn seitenleiste_hat_28_px_zeilen(cx: &mut TestAppContext) {
        let (handle, _, _) = open(cx);
        let (first, second) = cx
            .update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let nav = window.within("nav");
                (nav.find("0-0-0").bounds(), nav.find("0-0-1").bounds())
            })
            .unwrap();
        let pitch = second.origin.y - first.origin.y;
        assert!(
            (pitch - px(28.)).abs() < px(0.5),
            "Abstand von Zeile zu Zeile {pitch:?} statt 28 px"
        );
    }

    #[gpui_kit::test]
    async fn verschwundener_knopf_im_sheet_laesst_den_fokus_im_sheet(cx: &mut TestAppContext) {
        let (handle, workspace, radar) = open(cx);
        cx.update(|cx| {
            radar.update(cx, |r, cx| {
                r.set_repos_for_tests(vec!["octo/demo".into()], cx)
            })
        });
        press(cx, handle, "cmd-,");
        cx.wait_for(handle, WAIT, |window, cx| window.has_active_sheet(cx))
            .await;
        // Per Tastatur (Tab) zum Entfernen-Knopf, ein Mausklick fokussiert Knöpfe nicht.
        let mut on_button = false;
        for _ in 0..5 {
            press(cx, handle, "tab");
            on_button = cx
                .update_window(handle, |_, window, cx| {
                    window.render_frame(cx);
                    window.find("remove-octo/demo").focused() == Some(true)
                })
                .unwrap();
            if on_button {
                break;
            }
        }
        assert!(on_button, "Entfernen-Knopf per Tab nicht erreichbar");

        // Die Zeile samt fokussiertem Knopf verschwindet (wie nach dem Entfernen).
        cx.update(|cx| radar.update(cx, |r, cx| r.set_repos_for_tests(Vec::new(), cx)));
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        cx.run_until_parked();
        let in_sheet = cx
            .update_window(handle, |_, window, cx| {
                window.has_active_sheet(cx)
                    && window.focused(cx).is_some()
                    && !workspace.read(cx).focus.contains_focused(window, cx)
            })
            .unwrap();
        assert!(
            in_sheet,
            "der Fokus soll im offenen Sheet bleiben, nicht dahinter"
        );

        // Escape schließt das Sheet weiterhin, danach greifen die Kürzel wieder.
        press(cx, handle, "escape");
        cx.wait_for(handle, WAIT, |window, cx| !window.has_active_sheet(cx))
            .await;
        assert!(workspace_focused(cx, handle, &workspace));
        press(cx, handle, "cmd-2");
        assert_eq!(tab(cx, &radar), Tab::Merged);
    }

    #[gpui_kit::test]
    async fn verlorener_fokus_kehrt_in_den_workspace_zurueck(cx: &mut TestAppContext) {
        let (handle, workspace, radar) = open(cx);
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        // Fokus auf ein Element, das gleich nicht mehr gerendert wird (etwa ein
        // geschlossenes Popup): danach hängt der Fokus an keinem Element.
        cx.update_window(handle, |_, window, cx| {
            let gone = cx.focus_handle();
            gone.focus(window, cx);
            window.render_frame(cx);
        })
        .unwrap();
        cx.wait_for(handle, WAIT, |window, cx| {
            workspace.read(cx).focus.contains_focused(window, cx)
        })
        .await;
        press(cx, handle, "cmd-2");
        assert_eq!(tab(cx, &radar), Tab::Merged);
    }
}
