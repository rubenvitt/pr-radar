//! `Radar` besitzt Konfiguration, letzten Snapshot und Poll-Status und pollt GitHub adaptiv.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Utc};
use gpui_kit::{AppContext as _, Context, EventEmitter, Task};

use crate::config::{Config, Prefs, parse_repo};
use crate::github::{GitHub, RateLimit, TokenSource};
use crate::model::{MergeMethod, Snapshot, Transition, diff_snapshots};

/// Normales Poll-Intervall
const NORMAL: Duration = Duration::from_secs(30);
/// Intervall, solange irgendwo eine Pipeline läuft
const ACTIVE: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollState {
    Idle,
    Polling,
    Error,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub state: PollState,
    pub last_success: Option<DateTime<Utc>>,
    pub interval: Duration,
    pub error: Option<String>,
    pub rate_limit: Option<RateLimit>,
    pub token_source: Option<TokenSource>,
}

pub enum RadarEvent {
    /// Pipeline rot/grün oder PR gemergt – für Desktop-Benachrichtigungen.
    Transitions(Vec<Transition>),
    /// PRs, deren Pipeline, Review, Mergebarkeit oder Auto-Merge sich geändert hat – zum Hervorheben.
    Changed(Vec<String>),
    /// Eine Nutzeraktion (Merge, Auto-Merge) ist fehlgeschlagen.
    ActionFailed(String),
}

pub struct Radar {
    github: Arc<GitHub>,
    config: Config,
    snapshot: Option<Arc<Snapshot>>,
    status: Status,
    /// PR-IDs, für die gerade eine Mutation läuft
    pending: HashSet<String>,
    poll_task: Option<Task<()>>,
}

impl EventEmitter<RadarEvent> for Radar {}

impl Radar {
    pub fn new(github: Arc<GitHub>, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            github,
            config: Config::load(),
            snapshot: None,
            status: Status {
                state: PollState::Idle,
                last_success: None,
                interval: NORMAL,
                error: None,
                rate_limit: None,
                token_source: None,
            },
            pending: HashSet::new(),
            poll_task: None,
        };
        this.refresh(cx);
        this
    }

    pub fn snapshot(&self) -> Option<Arc<Snapshot>> {
        self.snapshot.clone()
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn repos(&self) -> &[String] {
        &self.config.repos
    }

    pub fn prefs(&self) -> &Prefs {
        &self.config.prefs
    }

    pub fn is_pending(&self, pr_id: &str) -> bool {
        self.pending.contains(pr_id)
    }

    pub fn update_prefs(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Prefs)) {
        f(&mut self.config.prefs);
        // Einstellungen sind Komfort – ein Schreibfehler soll die Bedienung nicht stören.
        let _ = self.config.save();
        cx.notify();
    }

    /// Sofort neu laden und den Poll-Zyklus neu starten.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.poll_task = Some(cx.spawn(async move |this, cx| {
            loop {
                let Ok((github, repos)) = this.update(cx, |this, cx| {
                    this.status.state = PollState::Polling;
                    cx.notify();
                    (this.github.clone(), this.config.repos.clone())
                }) else {
                    return;
                };

                let fetched = cx
                    .background_spawn(async move { github.fetch(&repos).await })
                    .await;

                let Ok(wait) = this.update(cx, |this, cx| this.apply(fetched, cx)) else {
                    return;
                };
                cx.background_executor().timer(wait).await;
            }
        }));
        cx.notify();
    }

    /// Übernimmt ein Poll-Ergebnis und liefert die Wartezeit bis zum nächsten Poll.
    fn apply(
        &mut self,
        fetched: Result<crate::github::Fetched>,
        cx: &mut Context<Self>,
    ) -> Duration {
        self.status.token_source = self.github.token_source();
        match fetched {
            Ok(f) => {
                if let Some(prev) = &self.snapshot {
                    let transitions = diff_snapshots(prev, &f.snapshot);
                    if !transitions.is_empty() {
                        cx.emit(RadarEvent::Transitions(transitions));
                    }
                    let changed = changed_prs(prev, &f.snapshot);
                    if !changed.is_empty() {
                        cx.emit(RadarEvent::Changed(changed));
                    }
                }
                self.snapshot = Some(Arc::new(f.snapshot));
                self.status.rate_limit = f.rate_limit.or(self.status.rate_limit.take());
                self.status.state = PollState::Idle;
                self.status.error = None;
                self.status.last_success = Some(Utc::now());
            }
            Err(e) => {
                self.status.state = PollState::Error;
                self.status.error = Some(format!("{e:#}"));
            }
        }
        let running = self.snapshot.as_ref().is_some_and(|s| s.any_running());
        self.status.interval = if running { ACTIVE } else { NORMAL };
        cx.notify();
        if self.status.state == PollState::Error {
            self.status.interval.max(NORMAL)
        } else {
            self.status.interval
        }
    }

    pub fn add_repo(&mut self, input: &str, cx: &mut Context<Self>) -> Result<()> {
        let full = parse_repo(input).ok_or_else(|| {
            anyhow!("Ungültiges Repo – erwartet owner/name oder eine GitHub-URL.")
        })?;
        if self
            .config
            .repos
            .iter()
            .any(|r| r.eq_ignore_ascii_case(&full))
        {
            bail!("{full} ist schon in der Liste.");
        }
        self.config.repos.push(full);
        self.config.save()?;
        self.refresh(cx);
        Ok(())
    }

    pub fn remove_repo(&mut self, full: &str, cx: &mut Context<Self>) -> Result<()> {
        self.config.repos.retain(|r| r != full);
        self.config.prefs.repo_filter.retain(|r| r != full);
        self.config.save()?;
        self.refresh(cx);
        Ok(())
    }

    /// Auto-Merge an (`Some(method)`) oder aus (`None`).
    pub fn set_auto_merge(
        &mut self,
        pr_id: &str,
        method: Option<MergeMethod>,
        cx: &mut Context<Self>,
    ) {
        if let Some(m) = method
            && let Err(e) = self.check_method(pr_id, m)
        {
            cx.emit(RadarEvent::ActionFailed(e.to_string()));
            return;
        }
        let github = self.github.clone();
        let id = pr_id.to_string();
        self.run_mutation(pr_id, cx, async move {
            github.set_auto_merge(&id, method).await
        });
    }

    pub fn merge(&mut self, pr_id: &str, method: MergeMethod, cx: &mut Context<Self>) {
        if let Err(e) = self.check_method(pr_id, method) {
            cx.emit(RadarEvent::ActionFailed(e.to_string()));
            return;
        }
        let github = self.github.clone();
        let id = pr_id.to_string();
        self.run_mutation(pr_id, cx, async move { github.merge(&id, method).await });
    }

    /// Nur Methoden zulassen, die das Repo des PRs erlaubt.
    fn check_method(&self, pr_id: &str, method: MergeMethod) -> Result<()> {
        let snapshot = self
            .snapshot
            .as_ref()
            .ok_or_else(|| anyhow!("Noch keine Daten geladen."))?;
        let pr = snapshot
            .open
            .iter()
            .find(|p| p.id == pr_id)
            .ok_or_else(|| anyhow!("Pull Request nicht mehr offen."))?;
        if !snapshot.allowed_methods(pr).contains(&method) {
            bail!(
                "Merge-Methode {} ist in {} nicht erlaubt.",
                method.label(),
                pr.repo
            );
        }
        Ok(())
    }

    fn run_mutation(
        &mut self,
        pr_id: &str,
        cx: &mut Context<Self>,
        work: impl Future<Output = Result<()>> + Send + 'static,
    ) {
        if !self.pending.insert(pr_id.to_string()) {
            return; // läuft schon – keine Doppel-Submission
        }
        cx.notify();
        let id = pr_id.to_string();
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(work).await;
            this.update(cx, |this, cx| {
                this.pending.remove(&id);
                if let Err(e) = result {
                    cx.emit(RadarEvent::ActionFailed(format!("{e:#}")));
                }
                this.refresh(cx);
            })
            .ok();
        })
        .detach();
    }
}

/// IDs offener PRs, deren sichtbarer Zustand sich zwischen zwei Snapshots geändert hat.
fn changed_prs(prev: &Snapshot, next: &Snapshot) -> Vec<String> {
    next.open
        .iter()
        .filter(|pr| {
            prev.open.iter().find(|p| p.id == pr.id).is_some_and(|old| {
                old.pipeline.state != pr.pipeline.state
                    || old.auto_merge != pr.auto_merge
                    || old.review_decision != pr.review_decision
                    || old.has_conflict() != pr.has_conflict()
            })
        })
        .map(|pr| pr.id.clone())
        .collect()
}
