//! GraphQL-Rohdaten → Domänenmodell. Port von `server/normalize.ts`.

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::model::*;

/* ---------- Rohdaten (Ausschnitt der GraphQL-Antwort) ---------- */

#[derive(Deserialize, Debug, Clone)]
pub struct Nodes<T> {
    #[serde(default = "Vec::new")]
    pub nodes: Vec<Option<T>>,
}

impl<T> Default for Nodes<T> {
    fn default() -> Self {
        Self { nodes: Vec::new() }
    }
}

impl<T> Nodes<T> {
    fn iter(&self) -> impl Iterator<Item = &T> {
        self.nodes.iter().flatten()
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "__typename")]
pub enum RawContext {
    CheckRun {
        name: String,
        status: String,
        conclusion: Option<String>,
        #[serde(rename = "detailsUrl")]
        details_url: Option<String>,
        #[serde(rename = "checkSuite")]
        check_suite: Option<RawCheckSuite>,
    },
    StatusContext {
        context: String,
        state: String,
        #[serde(rename = "targetUrl")]
        target_url: Option<String>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RawCheckSuite {
    workflow_run: Option<RawWorkflowRun>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RawWorkflowRun {
    workflow: Option<RawNamed>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RawNamed {
    name: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct RawRollup {
    pub state: Option<String>,
    pub contexts: Option<Nodes<RawContext>>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RawPerson {
    login: String,
    #[serde(default)]
    avatar_url: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RawRepo {
    pub name_with_owner: String,
    pub url: String,
    #[serde(default)]
    pub auto_merge_allowed: bool,
    #[serde(default)]
    pub squash_merge_allowed: bool,
    #[serde(default)]
    pub merge_commit_allowed: bool,
    #[serde(default)]
    pub rebase_merge_allowed: bool,
    pub viewer_permission: Option<String>,
    pub default_branch_ref: Option<RawBranchRef>,
    #[serde(default)]
    pub open: Nodes<RawOpen>,
    #[serde(default)]
    pub merged: Nodes<RawMerged>,
    #[serde(default)]
    pub releases: Nodes<RawRelease>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RawBranchRef {
    name: String,
    target: Option<RawTarget>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RawTarget {
    status_check_rollup: Option<RawRollup>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RawOpen {
    id: String,
    number: u64,
    title: String,
    url: String,
    #[serde(default)]
    is_draft: bool,
    updated_at: DateTime<Utc>,
    head_ref_name: String,
    #[serde(default)]
    head_ref_oid: String,
    base_ref_name: String,
    #[serde(default)]
    additions: u64,
    #[serde(default)]
    deletions: u64,
    review_decision: Option<String>,
    mergeable: Option<String>,
    merge_state_status: Option<String>,
    author: Option<RawPerson>,
    auto_merge_request: Option<RawAutoMerge>,
    #[serde(default)]
    labels: Nodes<RawLabel>,
    #[serde(default)]
    review_requests: Nodes<RawReviewRequest>,
    #[serde(default)]
    commits: Nodes<RawCommitNode>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RawAutoMerge {
    merge_method: String,
    enabled_by: Option<RawPerson>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RawLabel {
    name: String,
    color: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RawReviewRequest {
    requested_reviewer: Option<RawReviewer>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RawReviewer {
    login: Option<String>,
    slug: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RawCommitNode {
    commit: Option<RawCommit>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RawCommit {
    status_check_rollup: Option<RawRollup>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RawMerged {
    id: String,
    number: u64,
    title: String,
    url: String,
    merged_at: Option<DateTime<Utc>>,
    base_ref_name: String,
    author: Option<RawPerson>,
    merged_by: Option<RawPerson>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RawRelease {
    id: String,
    name: Option<String>,
    tag_name: String,
    url: String,
    published_at: Option<DateTime<Utc>>,
    #[serde(default)]
    is_prerelease: bool,
    #[serde(default)]
    is_latest: bool,
    #[serde(default)]
    is_draft: bool,
    #[serde(rename = "descriptionHTML")]
    description_html: Option<String>,
    author: Option<RawPerson>,
}

/* ---------- Checks & Pipeline ---------- */

const FAILED_CONCLUSIONS: [&str; 6] = [
    "FAILURE",
    "TIMED_OUT",
    "CANCELLED",
    "STARTUP_FAILURE",
    "ACTION_REQUIRED",
    "STALE",
];

pub fn normalize_check(ctx: &RawContext) -> Option<Check> {
    match ctx {
        RawContext::StatusContext {
            context,
            state,
            target_url,
        } => Some(Check {
            name: context.clone(),
            url: target_url.clone(),
            group: None,
            state: match state.as_str() {
                "SUCCESS" => CheckState::Passed,
                "FAILURE" | "ERROR" => CheckState::Failed,
                _ => CheckState::Running,
            },
        }),
        RawContext::CheckRun {
            name,
            status,
            conclusion,
            details_url,
            check_suite,
        } => {
            let group = check_suite
                .as_ref()
                .and_then(|s| s.workflow_run.as_ref())
                .and_then(|r| r.workflow.as_ref())
                .and_then(|w| w.name.clone());
            let state = match (status.as_str(), conclusion.as_deref()) {
                (s, _) if s != "COMPLETED" => CheckState::Running,
                (_, Some("SUCCESS")) => CheckState::Passed,
                (_, Some("SKIPPED")) => CheckState::Skipped,
                (_, Some(c)) if FAILED_CONCLUSIONS.contains(&c) => CheckState::Failed,
                _ => CheckState::Neutral,
            };
            Some(Check {
                name: name.clone(),
                url: details_url.clone(),
                state,
                group,
            })
        }
        RawContext::Other => None,
    }
}

pub fn rollup_state(state: Option<&str>) -> PipelineState {
    match state {
        Some("SUCCESS") => PipelineState::Passed,
        Some("FAILURE" | "ERROR") => PipelineState::Failed,
        Some("PENDING" | "EXPECTED") => PipelineState::Running,
        _ => PipelineState::None,
    }
}

pub fn build_pipeline(rollup: Option<&RawRollup>) -> Pipeline {
    let mut checks: Vec<Check> = rollup
        .and_then(|r| r.contexts.as_ref())
        .map(|c| c.iter().filter_map(normalize_check).collect())
        .unwrap_or_default();
    checks.sort_by(|a, b| {
        a.state
            .rank()
            .cmp(&b.state.rank())
            .then_with(|| a.name.cmp(&b.name))
    });

    let count = |s: CheckState| checks.iter().filter(|c| c.state == s).count();
    let (failed, running, passed) = (
        count(CheckState::Failed),
        count(CheckState::Running),
        count(CheckState::Passed),
    );

    let state = if failed > 0 {
        PipelineState::Failed
    } else if running > 0 {
        PipelineState::Running
    } else if !checks.is_empty() {
        PipelineState::Passed
    } else {
        rollup_state(rollup.and_then(|r| r.state.as_deref()))
    };

    Pipeline {
        state,
        total: checks.len(),
        passed,
        failed,
        running,
        checks,
    }
}

/* ---------- PRs, Releases, Repo ---------- */

fn person(p: &Option<RawPerson>) -> Option<Person> {
    p.as_ref().map(|p| Person {
        login: p.login.clone(),
        avatar_url: p.avatar_url.clone().unwrap_or_default(),
    })
}

pub fn normalize_open(repo: &str, n: &RawOpen) -> OpenPr {
    let rollup = n
        .commits
        .iter()
        .next()
        .and_then(|c| c.commit.as_ref())
        .and_then(|c| c.status_check_rollup.as_ref());
    OpenPr {
        id: n.id.clone(),
        repo: repo.to_string(),
        number: n.number,
        title: n.title.clone(),
        url: n.url.clone(),
        author: person(&n.author),
        is_draft: n.is_draft,
        updated_at: n.updated_at,
        head_ref: n.head_ref_name.clone(),
        head_oid: n.head_ref_oid.clone(),
        base_ref: n.base_ref_name.clone(),
        additions: n.additions,
        deletions: n.deletions,
        review_decision: match n.review_decision.as_deref() {
            Some("APPROVED") => Some(ReviewDecision::Approved),
            Some("CHANGES_REQUESTED") => Some(ReviewDecision::ChangesRequested),
            Some("REVIEW_REQUIRED") => Some(ReviewDecision::ReviewRequired),
            _ => None,
        },
        mergeable: match n.mergeable.as_deref() {
            Some("MERGEABLE") => Mergeable::Mergeable,
            Some("CONFLICTING") => Mergeable::Conflicting,
            _ => Mergeable::Unknown,
        },
        merge_state_status: n
            .merge_state_status
            .clone()
            .unwrap_or_else(|| "UNKNOWN".into()),
        auto_merge: n.auto_merge_request.as_ref().and_then(|a| {
            Some(AutoMerge {
                method: MergeMethod::parse(&a.merge_method)?,
                enabled_by: a.enabled_by.as_ref().map(|p| p.login.clone()),
            })
        }),
        labels: n
            .labels
            .iter()
            .map(|l| Label {
                name: l.name.clone(),
                color: l.color.clone(),
            })
            .collect(),
        review_requests: n
            .review_requests
            .iter()
            .filter_map(|r| {
                let rv = r.requested_reviewer.as_ref()?;
                rv.login
                    .clone()
                    .or_else(|| rv.slug.as_ref().map(|s| format!("@{s}")))
            })
            .collect(),
        pipeline: build_pipeline(rollup),
        // wird nach dem Laden der Rulesets gesetzt
        merge_methods: Vec::new(),
    }
}

/// Schränkt die Repo-Methoden mit den effektiven Branch-Regeln ein
/// (`GET /repos/{repo}/rules/branches/{branch}`): `pull_request.allowed_merge_methods`
/// und `required_linear_history` (verbietet Merge-Commits).
pub fn apply_branch_rules(
    repo_methods: &[MergeMethod],
    rules: &serde_json::Value,
) -> Vec<MergeMethod> {
    let mut methods = repo_methods.to_vec();
    for rule in rules.as_array().into_iter().flatten() {
        match rule["type"].as_str() {
            Some("pull_request") => {
                if let Some(allowed) = rule["parameters"]["allowed_merge_methods"].as_array() {
                    let allowed: Vec<MergeMethod> = allowed
                        .iter()
                        .filter_map(|m| m.as_str())
                        .filter_map(|m| MergeMethod::parse(&m.to_uppercase()))
                        .collect();
                    methods.retain(|m| allowed.contains(m));
                }
            }
            Some("required_linear_history") => methods.retain(|m| *m != MergeMethod::Merge),
            _ => {}
        }
    }
    methods
}

pub fn normalize_merged(repo: &str, n: &RawMerged) -> Option<MergedPr> {
    Some(MergedPr {
        id: n.id.clone(),
        repo: repo.to_string(),
        number: n.number,
        title: n.title.clone(),
        url: n.url.clone(),
        author: person(&n.author),
        merged_at: n.merged_at?,
        merged_by: n.merged_by.as_ref().map(|p| p.login.clone()),
        base_ref: n.base_ref_name.clone(),
    })
}

pub fn normalize_release(repo: &str, n: &RawRelease) -> Option<Release> {
    if n.is_draft {
        return None;
    }
    Some(Release {
        id: n.id.clone(),
        repo: repo.to_string(),
        name: n
            .name
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| n.tag_name.clone()),
        tag_name: n.tag_name.clone(),
        url: n.url.clone(),
        published_at: n.published_at,
        is_prerelease: n.is_prerelease,
        is_latest: n.is_latest,
        author: n.author.as_ref().map(|p| p.login.clone()),
        description_html: n.description_html.clone().unwrap_or_default(),
    })
}

pub fn normalize_repo(raw: &RawRepo) -> RepoInfo {
    let mut methods = Vec::new();
    if raw.squash_merge_allowed {
        methods.push(MergeMethod::Squash);
    }
    if raw.merge_commit_allowed {
        methods.push(MergeMethod::Merge);
    }
    if raw.rebase_merge_allowed {
        methods.push(MergeMethod::Rebase);
    }
    RepoInfo {
        full_name: raw.name_with_owner.clone(),
        url: raw.url.clone(),
        default_branch: raw.default_branch_ref.as_ref().map(|b| b.name.clone()),
        default_branch_pipeline: rollup_state(
            raw.default_branch_ref
                .as_ref()
                .and_then(|b| b.target.as_ref())
                .and_then(|t| t.status_check_rollup.as_ref())
                .and_then(|r| r.state.as_deref()),
        ),
        auto_merge_allowed: raw.auto_merge_allowed,
        merge_methods: methods,
        viewer_can_merge: matches!(
            raw.viewer_permission.as_deref(),
            Some("ADMIN" | "MAINTAIN" | "WRITE")
        ),
        error: None,
    }
}

/// Fügt ein Repo mit allen PRs, Merges und Releases in den Snapshot ein.
pub fn push_repo(snapshot: &mut Snapshot, raw: &RawRepo) {
    let name = raw.name_with_owner.as_str();
    snapshot.repos.push(normalize_repo(raw));
    snapshot
        .open
        .extend(raw.open.iter().map(|n| normalize_open(name, n)));
    snapshot
        .merged
        .extend(raw.merged.iter().filter_map(|n| normalize_merged(name, n)));
    snapshot.releases.extend(
        raw.releases
            .iter()
            .filter_map(|n| normalize_release(name, n)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rollup(v: serde_json::Value) -> RawRollup {
        serde_json::from_value(v).unwrap()
    }

    fn run(status: &str, conclusion: Option<&str>, name: &str) -> serde_json::Value {
        json!({ "__typename": "CheckRun", "name": name, "status": status, "conclusion": conclusion, "detailsUrl": null })
    }

    #[test]
    fn none_ohne_checks() {
        assert_eq!(build_pipeline(None).state, PipelineState::None);
    }

    #[test]
    fn rot_schlaegt_laufend() {
        let r = rollup(
            json!({ "state": "PENDING", "contexts": { "nodes": [run("IN_PROGRESS", None, "a"), run("COMPLETED", Some("FAILURE"), "b")] } }),
        );
        let p = build_pipeline(Some(&r));
        assert_eq!(
            (p.state, p.failed, p.running, p.total),
            (PipelineState::Failed, 1, 1, 2)
        );
        assert_eq!(p.checks[0].name, "b", "Fehler zuerst");
    }

    #[test]
    fn laufend_solange_etwas_nicht_fertig_ist() {
        let r = rollup(
            json!({ "state": "PENDING", "contexts": { "nodes": [run("QUEUED", None, "ci"), run("COMPLETED", Some("SUCCESS"), "ci")] } }),
        );
        assert_eq!(build_pipeline(Some(&r)).state, PipelineState::Running);
    }

    #[test]
    fn gruen_mit_uebersprungenen_checks() {
        let r = rollup(
            json!({ "state": "SUCCESS", "contexts": { "nodes": [run("COMPLETED", Some("SUCCESS"), "ci"), run("COMPLETED", Some("SKIPPED"), "ci")] } }),
        );
        assert_eq!(build_pipeline(Some(&r)).state, PipelineState::Passed);
    }

    #[test]
    fn versteht_klassische_commit_statuses() {
        let r = rollup(
            json!({ "state": "FAILURE", "contexts": { "nodes": [{ "__typename": "StatusContext", "context": "ci/jenkins", "state": "ERROR", "targetUrl": "x" }] } }),
        );
        assert_eq!(build_pipeline(Some(&r)).state, PipelineState::Failed);
    }

    #[test]
    fn ignoriert_unbekannte_kontexte_und_null() {
        let r = rollup(
            json!({ "state": "SUCCESS", "contexts": { "nodes": [null, { "__typename": "Something" }] } }),
        );
        let p = build_pipeline(Some(&r));
        assert_eq!((p.state, p.total), (PipelineState::Passed, 0));
    }

    #[test]
    fn mappt_rollup_states() {
        assert_eq!(rollup_state(Some("EXPECTED")), PipelineState::Running);
        assert_eq!(rollup_state(None), PipelineState::None);
    }

    fn raw_repo(flags: serde_json::Value) -> RawRepo {
        let mut base = json!({ "nameWithOwner": "a/b", "url": "u", "autoMergeAllowed": true, "viewerPermission": null, "defaultBranchRef": null });
        base.as_object_mut()
            .unwrap()
            .extend(flags.as_object().unwrap().clone());
        serde_json::from_value(base).unwrap()
    }

    #[test]
    fn nur_erlaubte_merge_methoden() {
        assert_eq!(
            normalize_repo(&raw_repo(json!({ "squashMergeAllowed": true }))).merge_methods,
            vec![MergeMethod::Squash]
        );
        assert_eq!(
            normalize_repo(&raw_repo(
                json!({ "mergeCommitAllowed": true, "rebaseMergeAllowed": true })
            ))
            .merge_methods,
            vec![MergeMethod::Merge, MergeMethod::Rebase]
        );
        assert!(
            normalize_repo(&raw_repo(json!({})))
                .merge_methods
                .is_empty()
        );
    }

    #[test]
    fn rulesets_schraenken_merge_methoden_ein() {
        let all = [MergeMethod::Squash, MergeMethod::Merge, MergeMethod::Rebase];
        let only_merge = json!([{ "type": "pull_request", "parameters": { "allowed_merge_methods": ["merge"] } }]);
        assert_eq!(
            apply_branch_rules(&all, &only_merge),
            vec![MergeMethod::Merge]
        );
        let linear = json!([{ "type": "required_linear_history" }, { "type": "deletion" }]);
        assert_eq!(
            apply_branch_rules(&all, &linear),
            vec![MergeMethod::Squash, MergeMethod::Rebase]
        );
        assert_eq!(
            apply_branch_rules(&[MergeMethod::Squash], &only_merge),
            vec![]
        );
        assert_eq!(apply_branch_rules(&all, &json!([])), all.to_vec());
    }

    #[test]
    fn normalisiert_offenen_pr() {
        let n: RawOpen = serde_json::from_value(json!({
            "id": "PR_1", "number": 7, "title": "Feat", "url": "u", "isDraft": false,
            "createdAt": "2026-09-30T10:00:00Z", "updatedAt": "2026-09-30T11:00:00Z",
            "headRefName": "feat", "baseRefName": "main", "additions": 3, "deletions": 1,
            "reviewDecision": "APPROVED", "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN",
            "author": { "login": "rubeen", "avatarUrl": "a" },
            "autoMergeRequest": { "enabledAt": "2026-09-30T11:00:00Z", "mergeMethod": "SQUASH", "enabledBy": { "login": "rubeen" } },
            "labels": { "nodes": [{ "name": "bug", "color": "d73a4a" }] },
            "reviewRequests": { "nodes": [
                { "requestedReviewer": { "__typename": "User", "login": "x" } },
                { "requestedReviewer": { "__typename": "Team", "slug": "core" } }
            ] },
            "commits": { "nodes": [{ "commit": { "statusCheckRollup": null } }] }
        }))
        .unwrap();
        let pr = normalize_open("a/b", &n);
        assert_eq!(pr.review_requests, vec!["x", "@core"]);
        assert_eq!(pr.auto_merge.as_ref().unwrap().method, MergeMethod::Squash);
        assert!(pr.ready_to_merge());
        assert_eq!(pr.pipeline.state, PipelineState::None);
    }
}
