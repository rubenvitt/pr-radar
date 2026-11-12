//! GraphQL-Dokumente für Dashboard-Abfrage und Merge-Mutationen.

const CHECKS: &str = r#"
  statusCheckRollup {
    state
    contexts(first: 60) {
      nodes {
        __typename
        ... on CheckRun {
          name status conclusion detailsUrl
          checkSuite { workflowRun { workflow { name } } }
        }
        ... on StatusContext { context state targetUrl }
      }
    }
  }"#;

/// Höchstzahl offener PRs je Repo und Abruf (so viele wie vor der Anpassung an die Repo-Größe).
pub const MAX_OPEN_PAGE: usize = 40;
/// Kleinste Seite für offene PRs.
const MIN_OPEN_PAGE: usize = 5;
/// Puffer über der zuletzt gesehenen Zahl offener PRs, damit neue PRs ohne Nachladen reinpassen.
const OPEN_PAGE_HEADROOM: usize = 3;

/// Seitengröße für die offenen PRs eines Repos. GitHub berechnet die Kosten einer Abfrage nach
/// dem `first` jeder Verbindung, nicht nach den tatsächlich gelieferten Knoten: Jeder offene PR
/// bringt vier verschachtelte Verbindungen mit (Labels, Reviewer, Commit, Checks). Ein Repo mit
/// zwei offenen PRs kostet mit `first: 5` daher rund ein Siebtel von `first: 40`.
/// Unbekannte Repos (erster Abruf) bekommen die volle Seite.
pub fn open_page_size(known_open: Option<usize>) -> usize {
    match known_open {
        None => MAX_OPEN_PAGE,
        Some(n) => (n + OPEN_PAGE_HEADROOM)
            .div_ceil(MIN_OPEN_PAGE)
            .saturating_mul(MIN_OPEN_PAGE)
            .clamp(MIN_OPEN_PAGE, MAX_OPEN_PAGE),
    }
}

fn repo_fields(open_page: usize) -> String {
    format!(
        r#"
  nameWithOwner url
  autoMergeAllowed squashMergeAllowed mergeCommitAllowed rebaseMergeAllowed
  viewerPermission
  defaultBranchRef {{ name target {{ ... on Commit {{ statusCheckRollup {{ state }} }} }} }}
  open: pullRequests(states: OPEN, first: {open_page}, orderBy: {{ field: UPDATED_AT, direction: DESC }}) {{
    totalCount
    nodes {{
      id number title url isDraft createdAt updatedAt
      headRefName headRefOid baseRefName additions deletions
      reviewDecision mergeable mergeStateStatus
      author {{ login avatarUrl }}
      autoMergeRequest {{ enabledAt mergeMethod enabledBy {{ login }} }}
      labels(first: 6) {{ nodes {{ name color }} }}
      reviewRequests(first: 10) {{
        nodes {{ requestedReviewer {{ __typename ... on User {{ login }} ... on Team {{ slug }} }} }}
      }}
      commits(last: 1) {{ nodes {{ commit {{ {CHECKS} }} }} }}
    }}
  }}
  merged: pullRequests(states: MERGED, first: 20, orderBy: {{ field: UPDATED_AT, direction: DESC }}) {{
    nodes {{
      id number title url mergedAt baseRefName
      author {{ login avatarUrl }}
      mergedBy {{ login }}
    }}
  }}
  releases(first: 6, orderBy: {{ field: CREATED_AT, direction: DESC }}) {{
    nodes {{
      id name tagName url publishedAt isPrerelease isLatest isDraft descriptionHTML
      author {{ login }}
    }}
  }}"#
    )
}

/// Ein Request für mehrere Repos, aliased als `r0`, `r1`, …; je Repo mit eigener Seitengröße
/// für die offenen PRs.
pub fn dashboard_query(repos: &[(String, usize)]) -> String {
    let parts: Vec<String> = repos
        .iter()
        .enumerate()
        .map(|(i, (full, open_page))| {
            let (owner, name) = full.split_once('/').unwrap_or((full, ""));
            format!(
                "r{i}: repository(owner: {}, name: {}) {{ {} }}",
                serde_json::to_string(owner).unwrap_or_default(),
                serde_json::to_string(name).unwrap_or_default(),
                repo_fields(*open_page),
            )
        })
        .collect();
    format!(
        "query Dashboard {{\n  viewer {{ login }}\n  rateLimit {{ remaining limit resetAt cost }}\n  {}\n}}",
        parts.join("\n")
    )
}

pub const ENABLE_AUTO_MERGE: &str = r#"
  mutation($id: ID!, $method: PullRequestMergeMethod!, $head: GitObjectID) {
    enablePullRequestAutoMerge(input: { pullRequestId: $id, mergeMethod: $method, expectedHeadOid: $head }) {
      pullRequest { id autoMergeRequest { enabledAt } }
    }
  }"#;

pub const DISABLE_AUTO_MERGE: &str = r#"
  mutation($id: ID!) {
    disablePullRequestAutoMerge(input: { pullRequestId: $id }) {
      pullRequest { id }
    }
  }"#;

pub const MERGE_PR: &str = r#"
  mutation($id: ID!, $method: PullRequestMergeMethod!, $head: GitObjectID) {
    mergePullRequest(input: { pullRequestId: $id, mergeMethod: $method, expectedHeadOid: $head }) {
      pullRequest { id merged }
    }
  }"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliast_repos_und_escaped_namen() {
        let q = dashboard_query(&[("a/b".into(), 40), ("c/d".into(), 5)]);
        assert!(q.contains(r#"r0: repository(owner: "a", name: "b")"#));
        assert!(q.contains(r#"r1: repository(owner: "c", name: "d")"#));
    }

    #[test]
    fn seitengroesse_je_repo() {
        let q = dashboard_query(&[("a/b".into(), 40), ("c/d".into(), 5)]);
        assert!(q.contains("states: OPEN, first: 40,"));
        assert!(q.contains("states: OPEN, first: 5,"));
        assert!(q.contains("totalCount"));
    }

    #[test]
    fn seitengroesse_folgt_der_zahl_offener_prs() {
        assert_eq!(open_page_size(None), MAX_OPEN_PAGE);
        assert_eq!(open_page_size(Some(0)), 5);
        assert_eq!(open_page_size(Some(2)), 5);
        assert_eq!(open_page_size(Some(3)), 10);
        assert_eq!(open_page_size(Some(7)), 10);
        assert_eq!(open_page_size(Some(8)), 15);
        assert_eq!(open_page_size(Some(37)), 40);
        assert_eq!(open_page_size(Some(500)), 40);
    }
}
