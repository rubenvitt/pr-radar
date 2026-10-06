//! `Radar` besitzt Konfiguration, letzten Snapshot und Poll-Status und pollt GitHub adaptiv.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Utc};
use gpui_kit::{AppContext as _, AsyncApp, Context, EventEmitter, Task, WeakEntity};

use crate::config::{Config, Prefs, parse_repo};
use crate::github::{GitHub, RateLimit, TokenSource};
use crate::model::{AutoMerge, MergeMethod, Snapshot, Transition, diff_snapshots};

/// So lange wird ein Merge bei vorübergehenden GitHub-Fehlern wiederholt.
const MERGE_DEADLINE: Duration = Duration::from_secs(180);
/// Wartezeit vor dem ersten erneuten Versuch; wächst bis `MERGE_RETRY_MAX`.
const MERGE_RETRY_START: Duration = Duration::from_secs(2);
const MERGE_RETRY_MAX: Duration = Duration::from_secs(10);

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

/// Wie ein Merge-Auftrag ausgegangen ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MergeOutcome {
    Merged,
    /// GitHub ließ den Merge noch nicht zu, Auto-Merge übernimmt.
    AutoMerge(MergeMethod),
}

impl MergeOutcome {
    fn settle(self) -> Settle {
        match self {
            Self::Merged => Settle::AwaitRefresh,
            Self::AutoMerge(m) => Settle::AutoMerge(Some(m)),
        }
    }
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
    /// „Alle mergen“ ist durch.
    MergeAllFinished {
        merged: usize,
        auto_merge: usize,
        failed: Vec<String>,
    },
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
    /// PRs, deren Merge GitHub gerade noch ablehnt und der gleich erneut versucht wird
    retrying: HashSet<String>,
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
            retrying: HashSet::new(),
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

    pub fn is_retrying(&self, pr_id: &str) -> bool {
        self.retrying.contains(pr_id)
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

    /// Jetzt mergen. Lehnt GitHub vorübergehend ab (z. B. weil gerade ein anderer PR in den
    /// Ziel-Branch gemergt wurde), wird erneut versucht bzw. Auto-Merge aktiviert.
    pub fn merge(&mut self, pr_id: &str, method: MergeMethod, cx: &mut Context<Self>) {
        if let Err(e) = self.check_method(pr_id, method) {
            cx.emit(RadarEvent::ActionFailed(e.to_string()));
            return;
        }
        if !self.pending.insert(pr_id.to_string()) {
            return; // läuft schon – keine Doppel-Submission
        }
        cx.notify();
        let id = pr_id.to_string();
        cx.spawn(async move |this, cx| {
            let result = merge_until_done(&this, &id, method, cx).await;
            this.update(cx, |this, cx| {
                this.settle(&id, result.map(MergeOutcome::settle).map_err(Some), cx)
            })
            .ok();
        })
        .detach();
    }

    /// Mergt mehrere PRs: je Repo und Ziel-Branch nacheinander (parallele Merges in denselben
    /// Branch lehnt GitHub ab), verschiedene Ziele parallel. Ein Fehlschlag stoppt die anderen nicht.
    pub fn merge_all(&mut self, items: Vec<(String, MergeMethod)>, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.clone() else {
            return;
        };
        // (Repo, Ziel-Branch) → [(PR-ID, „repo#nr“, Methode)]
        type Job = (String, String, MergeMethod);
        let mut groups: Vec<((String, String), Vec<Job>)> = Vec::new();
        for (id, method) in items {
            let Some(pr) = snapshot.open.iter().find(|p| p.id == id) else {
                continue;
            };
            if self.check_method(&id, method).is_err() || !self.pending.insert(id.clone()) {
                continue;
            }
            let key = (pr.repo.clone(), pr.base_ref.clone());
            let label = format!("{}#{}", pr.repo, pr.number);
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, list)) => list.push((id, label, method)),
                None => groups.push((key, vec![(id, label, method)])),
            }
        }
        if groups.is_empty() {
            return;
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let runs = groups.into_iter().map(|(_, list)| {
                let this = this.clone();
                let mut cx = cx.clone();
                async move {
                    let mut results = Vec::new();
                    for (id, label, method) in list {
                        let result = merge_until_done(&this, &id, method, &mut cx).await;
                        let summary = result
                            .as_ref()
                            .map(|o| *o)
                            .map_err(|e| format!("{label}: {e:#}"));
                        this.update(&mut cx, |this, cx| {
                            this.settle(&id, result.map(MergeOutcome::settle).map_err(|_| None), cx)
                        })
                        .ok();
                        results.push(summary);
                    }
                    results
                }
            });
            let results: Vec<_> = futures::future::join_all(runs)
                .await
                .into_iter()
                .flatten()
                .collect();
            let merged = results
                .iter()
                .filter(|r| matches!(r, Ok(MergeOutcome::Merged)))
                .count();
            let auto_merge = results
                .iter()
                .filter(|r| matches!(r, Ok(MergeOutcome::AutoMerge(_))))
                .count();
            let failed = results.into_iter().filter_map(Result::err).collect();
            this.update(cx, |_, cx| {
                cx.emit(RadarEvent::MergeAllFinished {
                    merged,
                    auto_merge,
                    failed,
                })
            })
            .ok();
        })
        .detach();
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
                this.settle(&id, result.map(|()| settle).map_err(Some), cx)
            })
            .ok();
        })
        .detach();
    }

    /// Schließt eine Mutation ab: Spinner lösen, Ergebnis anzeigen, neu laden.
    /// Ein `Err(None)` ist schon anderweitig gemeldet.
    fn settle(
        &mut self,
        pr_id: &str,
        result: Result<Settle, Option<anyhow::Error>>,
        cx: &mut Context<Self>,
    ) {
        self.pending.remove(pr_id);
        self.retrying.remove(pr_id);
        match result {
            Err(Some(e)) => cx.emit(RadarEvent::ActionFailed(format!("{e:#}"))),
            Err(None) => {}
            Ok(Settle::AutoMerge(method)) => self.apply_auto_merge(pr_id, method, cx),
            Ok(Settle::AwaitRefresh) => {
                self.awaiting_refresh.insert(pr_id.to_string());
            }
        }
        self.refresh(cx);
    }
}

/// Mergt einen PR und wiederholt bei vorübergehenden Ablehnungen bis `MERGE_DEADLINE`.
/// Zwischendurch wird Auto-Merge versucht – GitHub nimmt ihn nur an, solange der PR nicht
/// direkt mergebar ist, und mergt dann selbst, sobald es geht.
async fn merge_until_done(
    this: &WeakEntity<Radar>,
    pr_id: &str,
    method: MergeMethod,
    cx: &mut AsyncApp,
) -> Result<MergeOutcome> {
    let started = Instant::now();
    let mut delay = MERGE_RETRY_START;
    loop {
        // PR inzwischen weg (gemergt/geschlossen)? Dann ist nichts mehr zu tun.
        let Some((github, auto_merge_allowed)) = this.update(cx, |this, _| {
            let snapshot = this.snapshot.as_ref()?;
            let pr = snapshot.open.iter().find(|p| p.id == pr_id)?;
            let allowed = snapshot
                .repo(&pr.repo)
                .is_some_and(|r| r.auto_merge_allowed);
            Some((this.github.clone(), allowed))
        })?
        else {
            return Ok(MergeOutcome::Merged);
        };

        let id = pr_id.to_string();
        let gh = github.clone();
        let Err(error) = cx
            .background_spawn(async move { gh.merge(&id, method).await })
            .await
        else {
            return Ok(MergeOutcome::Merged);
        };
        if !is_transient_merge_error(&format!("{error:#}")) {
            return Err(error);
        }

        if auto_merge_allowed {
            let id = pr_id.to_string();
            let enabled = cx
                .background_spawn(async move { github.set_auto_merge(&id, Some(method)).await })
                .await;
            if enabled.is_ok() {
                return Ok(MergeOutcome::AutoMerge(method));
            }
        }

        if started.elapsed() + delay > MERGE_DEADLINE {
            return Err(error.context(format!(
                "GitHub lässt den Merge seit {} min nicht zu",
                MERGE_DEADLINE.as_secs() / 60
            )));
        }
        this.update(cx, |this, cx| {
            this.retrying.insert(pr_id.to_string());
            cx.notify();
        })?;
        cx.background_executor().timer(delay).await;
        delay = (delay * 3 / 2).min(MERGE_RETRY_MAX);
    }
}

/// Ablehnungen, die sich von selbst erledigen – etwa direkt nachdem ein anderer PR in denselben
/// Branch gemergt wurde und GitHub die Mergebarkeit neu berechnet.
fn is_transient_merge_error(message: &str) -> bool {
    let message = message.to_lowercase();
    [
        "base branch was modified",
        "try the merge again",
        "not mergeable",
        "merge already in progress",
    ]
    .iter()
    .any(|pattern| message.contains(pattern))
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
    fn erkennt_voruebergehende_merge_fehler() {
        assert!(is_transient_merge_error(
            "Base branch was modified. Review and try the merge again."
        ));
        assert!(is_transient_merge_error("Pull Request is not mergeable"));
        assert!(!is_transient_merge_error(
            "Repository rule violations found: Changes must be made through a pull request."
        ));
        assert!(!is_transient_merge_error(
            "viewer does not have permission to merge"
        ));
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
