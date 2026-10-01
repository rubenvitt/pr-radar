//! `Radar` besitzt Konfiguration, letzten Snapshot und Poll-Status und pollt GitHub adaptiv.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Utc};
use gpui_kit::{AppContext as _, Context, EventEmitter, Task};

use crate::config::{Config, Prefs, parse_repo};
use crate::github::{GitHub, RateLimit, TokenSource};
use crate::model::{AutoMerge, MergeMethod, Snapshot, Transition, diff_snapshots};

/// Wie lange ein lokal angenommener Auto-Merge-Zustand gegen ältere GitHub-Antworten gewinnt
const OPTIMISTIC_TTL: Duration = Duration::from_secs(30);

/// Was nach einer erfolgreichen Mutation passiert, bis GitHub den neuen Stand liefert.
#[derive(Clone, Copy)]
enum Settle {
    /// Auto-Merge an/aus: sofort lokal übernehmen.
    AutoMerge(Option<MergeMethod>),
    /// Direkt gemergt: Spinner bis zum nächsten Poll (dann verschwindet der PR).
    AwaitRefresh,
}

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
    /// Gemergte PRs, die bis zum nächsten Poll noch als „läuft“ gelten
    awaiting_refresh: HashSet<String>,
    /// Lokal vorweggenommener Auto-Merge-Zustand je PR, bis GitHub ihn bestätigt
    optimistic: HashMap<String, (Instant, Option<AutoMerge>)>,
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
            awaiting_refresh: HashSet::new(),
            optimistic: HashMap::new(),
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
        self.pending.contains(pr_id) || self.awaiting_refresh.contains(pr_id)
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
                let mut snapshot = f.snapshot;
                reconcile_optimistic(&mut self.optimistic, &mut snapshot, Instant::now());
                self.snapshot = Some(Arc::new(snapshot));
                self.awaiting_refresh.clear();
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
        self.run_mutation(pr_id, Settle::AutoMerge(method), cx, async move {
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
        self.run_mutation(pr_id, Settle::AwaitRefresh, cx, async move {
            github.merge(&id, method).await
        });
    }

    /// Zeigt einen erfolgreich geschalteten Auto-Merge sofort an, statt auf den nächsten Poll zu warten.
    fn apply_auto_merge(
        &mut self,
        pr_id: &str,
        method: Option<MergeMethod>,
        cx: &mut Context<Self>,
    ) {
        let Some(snapshot) = self.snapshot.as_mut() else {
            return;
        };
        let viewer = snapshot.viewer.clone();
        let state = method.map(|method| AutoMerge {
            method,
            enabled_by: viewer,
        });
        if let Some(pr) = Arc::make_mut(snapshot)
            .open
            .iter_mut()
            .find(|p| p.id == pr_id)
        {
            pr.auto_merge = state.clone();
        }
        self.optimistic
            .insert(pr_id.to_string(), (Instant::now(), state));
        cx.emit(RadarEvent::Changed(vec![pr_id.to_string()]));
        cx.notify();
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
        settle: Settle,
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
                match (result, settle) {
                    (Err(e), _) => cx.emit(RadarEvent::ActionFailed(format!("{e:#}"))),
                    (Ok(()), Settle::AutoMerge(method)) => this.apply_auto_merge(&id, method, cx),
                    (Ok(()), Settle::AwaitRefresh) => {
                        this.awaiting_refresh.insert(id.clone());
                    }
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

/// GitHub liefert einen neuen Auto-Merge-Zustand manchmal erst verzögert. Bis er ankommt
/// (max. `OPTIMISTIC_TTL`), gilt der lokal angenommene, damit der Button nicht zurückspringt.
fn reconcile_optimistic(
    optimistic: &mut HashMap<String, (Instant, Option<AutoMerge>)>,
    snapshot: &mut Snapshot,
    now: Instant,
) {
    optimistic.retain(|id, (at, expected)| {
        let Some(pr) = snapshot.open.iter_mut().find(|p| p.id == *id) else {
            return false;
        };
        if pr.auto_merge.is_some() == expected.is_some() || now.duration_since(*at) > OPTIMISTIC_TTL
        {
            return false;
        }
        pr.auto_merge = expected.clone();
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Mergeable, OpenPr, Pipeline, PipelineState};

    fn snapshot(auto: Option<AutoMerge>) -> Snapshot {
        Snapshot {
            open: vec![OpenPr {
                id: "1".into(),
                repo: "a/b".into(),
                number: 1,
                title: "t".into(),
                url: "u".into(),
                author: None,
                is_draft: false,
                updated_at: Utc::now(),
                head_ref: "h".into(),
                base_ref: "main".into(),
                additions: 0,
                deletions: 0,
                review_decision: None,
                mergeable: Mergeable::Mergeable,
                merge_state_status: "BLOCKED".into(),
                auto_merge: auto,
                labels: vec![],
                review_requests: vec![],
                pipeline: Pipeline {
                    state: PipelineState::Running,
                    total: 0,
                    passed: 0,
                    failed: 0,
                    running: 0,
                    checks: vec![],
                },
                merge_methods: vec![MergeMethod::Merge],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn optimistischer_auto_merge_ueberlebt_veraltete_antwort() {
        let t0 = Instant::now();
        let on = Some(AutoMerge {
            method: MergeMethod::Merge,
            enabled_by: None,
        });
        let mut optimistic = HashMap::from([("1".to_string(), (t0, on.clone()))]);

        // GitHub hängt hinterher → lokaler Zustand bleibt sichtbar
        let mut stale = snapshot(None);
        reconcile_optimistic(&mut optimistic, &mut stale, t0);
        assert_eq!(stale.open[0].auto_merge, on);
        assert!(optimistic.contains_key("1"));

        // GitHub bestätigt → Eintrag erledigt
        let mut fresh = snapshot(on.clone());
        reconcile_optimistic(&mut optimistic, &mut fresh, t0);
        assert!(optimistic.is_empty());

        // Nach Ablauf gewinnt GitHub
        let mut optimistic = HashMap::from([("1".to_string(), (t0, on))]);
        let mut stale = snapshot(None);
        reconcile_optimistic(
            &mut optimistic,
            &mut stale,
            t0 + OPTIMISTIC_TTL + Duration::from_secs(1),
        );
        assert_eq!(stale.open[0].auto_merge, None);
    }
}
