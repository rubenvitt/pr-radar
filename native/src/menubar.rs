//! Menüleisten-Symbol mit der Zahl roter Pipelines, Dock-Badge und „Beim Anmelden starten“.
//!
//! Gezählt werden alle offenen PRs mit roter Pipeline, unabhängig von Suche und Filtern
//! im Fenster (wie der Fenstertitel). Das Menü listet die neuesten davon; ein Klick
//! öffnet den PR auf GitHub.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use crate::model::{OpenPr, Snapshot};

/// So viele rote PRs stehen höchstens direkt im Menü.
const MAX_LISTED: usize = 10;
/// Längere PR-Titel werden im Menü gekürzt.
const MAX_TITLE: usize = 60;

/// Was Symbol, Badge und Menü gerade zeigen.
#[derive(Clone, Debug, Default, PartialEq)]
struct Summary {
    /// Noch kein erfolgreicher Abruf
    loading: bool,
    failed: usize,
    /// Die neuesten roten PRs: (Menütext, URL)
    listed: Vec<(String, String)>,
}

impl Summary {
    fn new(snapshot: Option<&Snapshot>) -> Self {
        let Some(snapshot) = snapshot else {
            return Self {
                loading: true,
                ..Self::default()
            };
        };
        let mut failed: Vec<&OpenPr> = snapshot.failed().collect();
        failed.sort_by_key(|pr| std::cmp::Reverse(pr.updated_at));
        Self {
            loading: false,
            failed: failed.len(),
            listed: failed
                .iter()
                .take(MAX_LISTED)
                .map(|pr| (item_title(pr), pr.url.clone()))
                .collect(),
        }
    }

    /// Zahl am Symbol und im Dock; keine bei 0.
    fn badge(&self) -> Option<String> {
        match self.failed {
            0 => None,
            n if n > 99 => Some("99+".into()),
            n => Some(n.to_string()),
        }
    }

    fn headline(&self) -> String {
        if self.loading {
            return tr!("No data yet", "Noch keine Daten").into();
        }
        match self.failed {
            0 => tr!("No failed pipelines", "Keine roten Pipelines").into(),
            1 => tr!("1 failed pipeline", "1 rote Pipeline").into(),
            n => trf!("{n} failed pipelines", "{n} rote Pipelines"),
        }
    }

    fn tooltip(&self) -> String {
        format!("PR Radar – {}", self.headline())
    }
}

/// „owner/repo #12 · Titel“, Titel bei Bedarf mit „…“ gekürzt.
fn item_title(pr: &OpenPr) -> String {
    let title = pr.title.trim();
    let title = if title.chars().count() > MAX_TITLE {
        let cut: String = title.chars().take(MAX_TITLE - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        title.to_string()
    };
    format!("{} #{} · {}", pr.repo, pr.number, title)
}

#[cfg(target_os = "macos")]
pub use mac::{init, login_item_checked, toggle_login_item};

#[cfg(target_os = "macos")]
mod mac {
    use futures::StreamExt as _;
    use gpui_kit::{App, Entity, Global};

    use super::Summary;
    use crate::config::{Quick, Tab};
    use crate::macos::{self, Entry, LoginState, StatusItem};
    use crate::radar::Radar;

    const TAG_SHOW_FAILED: isize = 1;
    const TAG_OPEN_WINDOW: isize = 2;
    const TAG_LOGIN: isize = 3;
    const TAG_QUIT: isize = 4;
    /// Ab hier: Index in `MenuBar::urls`.
    const TAG_PR: isize = 100;

    struct MenuBar {
        item: StatusItem,
        radar: Entity<Radar>,
        shown: Option<Summary>,
        urls: Vec<String>,
    }

    impl Global for MenuBar {}

    /// Legt das Menüleisten-Symbol an und hält es mit dem Radar aktuell.
    pub fn init(radar: Entity<Radar>, cx: &mut App) {
        let (clicks, mut rx) = futures::channel::mpsc::unbounded();
        let Some(item) = StatusItem::new(clicks) else {
            log::warn!("Menüleisten-Symbol nicht angelegt (nicht auf dem Haupt-Thread)");
            return;
        };
        cx.set_global(MenuBar {
            item,
            radar: radar.clone(),
            shown: None,
            urls: Vec::new(),
        });
        update(cx);
        cx.observe(&radar, |_, cx| update(cx)).detach();
        cx.spawn(async move |cx| {
            while let Some(tag) = rx.next().await {
                cx.update(|cx| clicked(tag, cx));
            }
        })
        .detach();
    }

    fn update(cx: &mut App) {
        let Some(bar) = cx.try_global::<MenuBar>() else {
            return;
        };
        let summary = Summary::new(bar.radar.read(cx).snapshot().as_deref());
        if bar.shown.as_ref() == Some(&summary) {
            return;
        }

        let badge = summary.badge();
        bar.item.set_badge(badge.as_deref(), &summary.tooltip());
        macos::set_dock_badge(badge.as_deref());
        bar.item.set_entries(entries(&summary));

        let bar = cx.global_mut::<MenuBar>();
        bar.urls = summary.listed.iter().map(|(_, url)| url.clone()).collect();
        bar.shown = Some(summary);
    }

    fn entries(summary: &Summary) -> Vec<Entry> {
        let mut entries = vec![Entry::Label(summary.headline())];
        for (i, (title, url)) in summary.listed.iter().enumerate() {
            entries.push(Entry::Item {
                title: title.clone(),
                tag: TAG_PR + i as isize,
                tooltip: Some(url.clone()),
            });
        }
        if summary.failed > 0 {
            let more = summary.failed - summary.listed.len();
            let title = match more {
                0 => tr!("Show failed PRs in PR Radar", "Rote PRs in PR Radar zeigen").to_string(),
                n => trf!(
                    "Show all {} failed PRs ({n} more)",
                    "Alle {} roten PRs zeigen ({n} weitere)",
                    summary.failed
                ),
            };
            entries.push(Entry::Item {
                title,
                tag: TAG_SHOW_FAILED,
                tooltip: None,
            });
        }
        entries.extend([
            Entry::Separator,
            Entry::Item {
                title: tr!("Open PR Radar", "PR Radar öffnen").into(),
                tag: TAG_OPEN_WINDOW,
                tooltip: None,
            },
            Entry::LoginItem {
                title: tr!("Open at Login", "Beim Anmelden starten").into(),
                tag: TAG_LOGIN,
            },
            Entry::Separator,
            Entry::Item {
                title: tr!("Quit PR Radar", "PR Radar beenden").into(),
                tag: TAG_QUIT,
                tooltip: None,
            },
        ]);
        entries
    }

    fn clicked(tag: isize, cx: &mut App) {
        match tag {
            TAG_SHOW_FAILED => {
                if let Some(bar) = cx.try_global::<MenuBar>() {
                    let radar = bar.radar.clone();
                    radar.update(cx, |radar, cx| {
                        radar.update_prefs(cx, |p| {
                            p.tab = Tab::Open;
                            p.quick = Quick::Failed;
                            // Sonst fehlen rote PRs aus ausgeblendeten Repos.
                            p.repo_filter.clear();
                        })
                    });
                }
                crate::ui::show_main_window(cx);
            }
            TAG_OPEN_WINDOW => crate::ui::show_main_window(cx),
            TAG_LOGIN => toggle_login_item(cx),
            TAG_QUIT => cx.quit(),
            tag if tag >= TAG_PR => {
                let url = cx
                    .try_global::<MenuBar>()
                    .and_then(|bar| bar.urls.get((tag - TAG_PR) as usize).cloned());
                if let Some(url) = url {
                    cx.open_url(&url);
                }
            }
            _ => {}
        }
    }

    /// Häkchen für „Beim Anmelden starten“ im App-Menü.
    pub fn login_item_checked() -> bool {
        macos::login_state() == LoginState::On
    }

    /// Schaltet „Beim Anmelden starten“ um. Verlangt macOS eine Freigabe in den
    /// Systemeinstellungen, öffnet es die passende Seite.
    pub fn toggle_login_item(cx: &mut App) {
        let result = match macos::login_state() {
            LoginState::On => macos::set_login(false),
            LoginState::Off => macos::set_login(true).map(|()| {
                if macos::login_state() == LoginState::NeedsApproval {
                    macos::open_login_settings();
                }
            }),
            LoginState::NeedsApproval => {
                macos::open_login_settings();
                Ok(())
            }
            LoginState::Unavailable => Err(LoginState::Unavailable
                .hint()
                .unwrap_or_default()
                .to_string()),
        };
        if let Err(e) = result {
            log::warn!("Beim Anmelden starten: {e}");
            crate::ui::show_error(tr!("Open at Login", "Beim Anmelden starten"), e, cx);
        }
        crate::ui::refresh_menus(cx);
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::model::{Mergeable, Pipeline, PipelineState};

    fn pr(number: u64, state: PipelineState, minute: u32) -> OpenPr {
        OpenPr {
            id: format!("PR_{number}"),
            repo: "acme/app".into(),
            number,
            title: format!("PR {number}"),
            url: format!("https://github.com/acme/app/pull/{number}"),
            author: None,
            is_draft: false,
            updated_at: Utc.with_ymd_and_hms(2026, 11, 8, 12, minute, 0).unwrap(),
            head_ref: "feature".into(),
            head_oid: "abc".into(),
            base_ref: "main".into(),
            additions: 0,
            deletions: 0,
            review_decision: None,
            mergeable: Mergeable::Mergeable,
            merge_state_status: "CLEAN".into(),
            auto_merge: None,
            labels: Vec::new(),
            review_requests: Vec::new(),
            pipeline: Pipeline {
                state,
                total: 1,
                passed: 0,
                failed: 0,
                running: 0,
                checks: Vec::new(),
            },
            merge_methods: Vec::new(),
        }
    }

    fn snapshot(open: Vec<OpenPr>) -> Snapshot {
        Snapshot {
            open,
            ..Snapshot::default()
        }
    }

    #[test]
    fn ohne_snapshot_noch_keine_daten_und_kein_badge() {
        let s = Summary::new(None);
        assert!(s.loading);
        assert_eq!(s.badge(), None);
        assert_eq!(s.headline(), "No data yet");
        let de = crate::l10n::with_lang(crate::l10n::Lang::De, || s.headline());
        assert_eq!(de, "Noch keine Daten");
    }

    #[test]
    fn zaehlt_nur_rote_pipelines_neueste_zuerst() {
        let s = Summary::new(Some(&snapshot(vec![
            pr(1, PipelineState::Failed, 1),
            pr(2, PipelineState::Passed, 2),
            pr(3, PipelineState::Running, 3),
            pr(4, PipelineState::Failed, 4),
        ])));
        assert_eq!(s.failed, 2);
        assert_eq!(s.badge().as_deref(), Some("2"));
        assert_eq!(s.headline(), "2 failed pipelines");
        let urls: Vec<&str> = s.listed.iter().map(|(_, u)| u.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://github.com/acme/app/pull/4",
                "https://github.com/acme/app/pull/1"
            ]
        );
    }

    #[test]
    fn alles_gruen_ohne_badge() {
        let s = Summary::new(Some(&snapshot(vec![pr(1, PipelineState::Passed, 1)])));
        assert_eq!(s.badge(), None);
        assert_eq!(s.headline(), "No failed pipelines");
        assert_eq!(s.tooltip(), "PR Radar – No failed pipelines");
    }

    #[test]
    fn listet_hoechstens_zehn_und_kappt_badge() {
        let open = (0..120).map(|i| pr(i, PipelineState::Failed, 0)).collect();
        let s = Summary::new(Some(&snapshot(open)));
        assert_eq!(s.failed, 120);
        assert_eq!(s.listed.len(), MAX_LISTED);
        assert_eq!(s.badge().as_deref(), Some("99+"));
        let one = Summary::new(Some(&snapshot(vec![pr(1, PipelineState::Failed, 0)])));
        assert_eq!(one.headline(), "1 failed pipeline");
    }

    #[test]
    fn kuerzt_lange_titel() {
        let mut p = pr(7, PipelineState::Failed, 0);
        p.title = "x".repeat(80);
        let title = item_title(&p);
        assert!(title.starts_with("acme/app #7 · "));
        assert!(title.ends_with('…'));
        assert_eq!(
            title.chars().count(),
            "acme/app #7 · ".chars().count() + MAX_TITLE
        );

        p.title = "  Kurzer Titel ".into();
        assert_eq!(item_title(&p), "acme/app #7 · Kurzer Titel");
    }
}
