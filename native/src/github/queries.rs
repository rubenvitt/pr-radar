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

fn repo_fields() -> String {
    format!(
        r#"
  nameWithOwner url
  autoMergeAllowed squashMergeAllowed mergeCommitAllowed rebaseMergeAllowed
  viewerPermission
  defaultBranchRef {{ name target {{ ... on Commit {{ statusCheckRollup {{ state }} }} }} }}
  open: pullRequests(states: OPEN, first: 40, orderBy: {{ field: UPDATED_AT, direction: DESC }}) {{
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

/// Ein Request für mehrere Repos, aliased als `r0`, `r1`, …
pub fn dashboard_query(repos: &[String]) -> String {
    let fields = repo_fields();
    let parts: Vec<String> = repos
        .iter()
        .enumerate()
        .map(|(i, full)| {
            let (owner, name) = full.split_once('/').unwrap_or((full, ""));
            format!(
                "r{i}: repository(owner: {}, name: {}) {{ {fields} }}",
                serde_json::to_string(owner).unwrap_or_default(),
                serde_json::to_string(name).unwrap_or_default(),
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
        let q = dashboard_query(&["a/b".into(), "c/d".into()]);
        assert!(q.contains(r#"r0: repository(owner: "a", name: "b")"#));
        assert!(q.contains(r#"r1: repository(owner: "c", name: "d")"#));
    }
}
