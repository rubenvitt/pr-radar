//! Domänenmodell: der normalisierte Stand aller beobachteten Repos.

use chrono::{DateTime, Utc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PipelineState {
    Running,
    Failed,
    Passed,
    None,
}

impl PipelineState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "läuft",
            Self::Failed => "rot",
            Self::Passed => "grün",
            Self::None => "ohne Ergebnis",
        }
    }

    /// Sortierreihenfolge: Rotes und Laufendes zuerst.
    pub fn rank(self) -> u8 {
        match self {
            Self::Failed => 0,
            Self::Running => 1,
            Self::Passed => 2,
            Self::None => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState {
    Running,
    Failed,
    Passed,
    Skipped,
    Neutral,
}

impl CheckState {
    pub fn rank(self) -> u8 {
        match self {
            Self::Failed => 0,
            Self::Running => 1,
            Self::Passed => 2,
            Self::Neutral => 3,
            Self::Skipped => 4,
        }
    }

    pub fn pipeline(self) -> PipelineState {
        match self {
            Self::Failed => PipelineState::Failed,
            Self::Running => PipelineState::Running,
            Self::Passed => PipelineState::Passed,
            Self::Skipped | Self::Neutral => PipelineState::None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MergeMethod {
    Squash,
    Merge,
    Rebase,
}

impl MergeMethod {
    pub fn label(self) -> &'static str {
        match self {
            Self::Squash => "Squash",
            Self::Merge => "Merge",
            Self::Rebase => "Rebase",
        }
    }

    pub fn graphql(self) -> &'static str {
        match self {
            Self::Squash => "SQUASH",
            Self::Merge => "MERGE",
            Self::Rebase => "REBASE",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "SQUASH" => Some(Self::Squash),
            "MERGE" => Some(Self::Merge),
            "REBASE" => Some(Self::Rebase),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
    ReviewRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)] // spiegelt GitHubs MergeableState
pub enum Mergeable {
    Mergeable,
    Conflicting,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub name: String,
    pub state: CheckState,
    pub url: Option<String>,
    /// Workflow-Name bei GitHub Actions
    pub group: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pipeline {
    pub state: PipelineState,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub running: usize,
    pub checks: Vec<Check>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    pub login: String,
    pub avatar_url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub name: String,
    /// Hex ohne `#`, so wie GitHub ihn liefert
    pub color: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutoMerge {
    pub method: MergeMethod,
    pub enabled_by: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OpenPr {
    pub id: String,
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: Option<Person>,
    pub is_draft: bool,
    pub updated_at: DateTime<Utc>,
    pub head_ref: String,
    /// Commit, auf dem der Branch gerade steht – Merges werden daran gebunden
    pub head_oid: String,
    pub base_ref: String,
    pub additions: u64,
    pub deletions: u64,
    pub review_decision: Option<ReviewDecision>,
    pub mergeable: Mergeable,
    pub merge_state_status: String,
    pub auto_merge: Option<AutoMerge>,
    pub labels: Vec<Label>,
    pub review_requests: Vec<String>,
    pub pipeline: Pipeline,
    /// Effektiv erlaubte Merge-Methoden: Repo-Einstellungen ∩ Rulesets des Ziel-Branches
    pub merge_methods: Vec<MergeMethod>,
}

/// GitHub-Zustände, in denen ein PR sofort gemergt werden kann (Auto-Merge lehnt GitHub dann ab).
const DIRECT_MERGE_STATES: [&str; 3] = ["CLEAN", "HAS_HOOKS", "UNSTABLE"];

impl OpenPr {
    /// Merge-Konflikt – `DIRTY` greift auch, solange GitHub `mergeable` noch berechnet.
    pub fn has_conflict(&self) -> bool {
        self.mergeable == Mergeable::Conflicting || self.merge_state_status == "DIRTY"
    }

    pub fn ready_to_merge(&self) -> bool {
        self.mergeable == Mergeable::Mergeable
            && DIRECT_MERGE_STATES.contains(&self.merge_state_status.as_str())
    }

    pub fn is_by(&self, login: Option<&str>) -> bool {
        matches!((login, &self.author), (Some(l), Some(a)) if a.login == l)
    }

    pub fn requests_review_from(&self, login: Option<&str>) -> bool {
        login.is_some_and(|l| self.review_requests.iter().any(|r| r == l))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MergedPr {
    pub id: String,
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: Option<Person>,
    pub merged_at: DateTime<Utc>,
    pub merged_by: Option<String>,
    pub base_ref: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub id: String,
    pub repo: String,
    pub name: String,
    pub tag_name: String,
    pub url: String,
    pub published_at: Option<DateTime<Utc>>,
    pub is_prerelease: bool,
    pub is_latest: bool,
    pub author: Option<String>,
    pub description_html: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RepoInfo {
    pub full_name: String,
    pub url: String,
    pub default_branch: Option<String>,
    pub default_branch_pipeline: PipelineState,
    pub auto_merge_allowed: bool,
    pub merge_methods: Vec<MergeMethod>,
    pub viewer_can_merge: bool,
    pub error: Option<String>,
}

impl RepoInfo {
    pub fn unavailable(full_name: &str, error: String) -> Self {
        Self {
            full_name: full_name.to_string(),
            url: format!("https://github.com/{full_name}"),
            default_branch: None,
            default_branch_pipeline: PipelineState::None,
            auto_merge_allowed: false,
            merge_methods: Vec::new(),
            viewer_can_merge: false,
            error: Some(error),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub viewer: Option<String>,
    pub repos: Vec<RepoInfo>,
    pub open: Vec<OpenPr>,
    pub merged: Vec<MergedPr>,
    pub releases: Vec<Release>,
}

impl Snapshot {
    pub fn repo(&self, full_name: &str) -> Option<&RepoInfo> {
        self.repos.iter().find(|r| r.full_name == full_name)
    }

    pub fn any_running(&self) -> bool {
        self.open
            .iter()
            .any(|p| p.pipeline.state == PipelineState::Running)
    }

    /// Merge-Methoden, die Repo und Rulesets für diesen PR erlauben.
    pub fn allowed_methods<'a>(&self, pr: &'a OpenPr) -> &'a [MergeMethod] {
        &pr.merge_methods
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionKind {
    Failed,
    Passed,
    Merged,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub kind: TransitionKind,
    pub pr: OpenPr,
}

/// Relevante Zustandswechsel zwischen zwei Snapshots (für Benachrichtigungen).
pub fn diff_snapshots(prev: &Snapshot, next: &Snapshot) -> Vec<Transition> {
    let mut out = Vec::new();
    for pr in &next.open {
        let Some(old) = prev.open.iter().find(|p| p.id == pr.id) else {
            continue;
        };
        if old.pipeline.state == pr.pipeline.state {
            continue;
        }
        match pr.pipeline.state {
            PipelineState::Failed => out.push(Transition {
                kind: TransitionKind::Failed,
                pr: pr.clone(),
            }),
            PipelineState::Passed if old.pipeline.state == PipelineState::Running => {
                out.push(Transition {
                    kind: TransitionKind::Passed,
                    pr: pr.clone(),
                })
            }
            _ => {}
        }
    }
    for old in &prev.open {
        let gone = !next.open.iter().any(|p| p.id == old.id);
        if gone && next.merged.iter().any(|m| m.id == old.id) {
            out.push(Transition {
                kind: TransitionKind::Merged,
                pr: old.clone(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(id: &str, state: PipelineState) -> OpenPr {
        OpenPr {
            id: id.into(),
            repo: "a/b".into(),
            number: 1,
            title: "t".into(),
            url: "u".into(),
            author: None,
            is_draft: false,
            updated_at: Utc::now(),
            head_ref: "h".into(),
            head_oid: String::new(),
            base_ref: "main".into(),
            additions: 0,
            deletions: 0,
            review_decision: None,
            mergeable: Mergeable::Mergeable,
            merge_state_status: "CLEAN".into(),
            auto_merge: None,
            labels: vec![],
            review_requests: vec![],
            pipeline: Pipeline {
                state,
                total: 0,
                passed: 0,
                failed: 0,
                running: 0,
                checks: vec![],
            },
            merge_methods: vec![MergeMethod::Merge],
        }
    }

    #[test]
    fn diff_meldet_rot_gruen_und_merge() {
        let prev = Snapshot {
            open: vec![
                pr("1", PipelineState::Running),
                pr("2", PipelineState::Running),
                pr("3", PipelineState::Passed),
            ],
            ..Default::default()
        };
        let next = Snapshot {
            open: vec![
                pr("1", PipelineState::Failed),
                pr("2", PipelineState::Passed),
            ],
            merged: vec![MergedPr {
                id: "3".into(),
                repo: "a/b".into(),
                number: 3,
                title: "t".into(),
                url: "u".into(),
                author: None,
                merged_at: Utc::now(),
                merged_by: None,
                base_ref: "main".into(),
            }],
            ..Default::default()
        };
        let kinds: Vec<_> = diff_snapshots(&prev, &next)
            .iter()
            .map(|t| (t.pr.id.clone(), t.kind))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("1".into(), TransitionKind::Failed),
                ("2".into(), TransitionKind::Passed),
                ("3".into(), TransitionKind::Merged)
            ]
        );
    }

    #[test]
    fn konflikt_auch_bei_dirty() {
        let mut p = pr("1", PipelineState::Passed);
        p.mergeable = Mergeable::Unknown;
        p.merge_state_status = "DIRTY".into();
        assert!(p.has_conflict());
        assert!(!p.ready_to_merge());
    }
}
