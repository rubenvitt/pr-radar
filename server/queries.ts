const CHECKS = `
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
  }`;

const REPO_FIELDS = `
  nameWithOwner url
  autoMergeAllowed squashMergeAllowed mergeCommitAllowed rebaseMergeAllowed
  viewerPermission
  defaultBranchRef { name target { ... on Commit { statusCheckRollup { state } } } }
  open: pullRequests(states: OPEN, first: 40, orderBy: { field: UPDATED_AT, direction: DESC }) {
    nodes {
      id number title url isDraft createdAt updatedAt
      headRefName baseRefName additions deletions
      reviewDecision mergeable mergeStateStatus
      author { login avatarUrl }
      autoMergeRequest { enabledAt mergeMethod enabledBy { login } }
      labels(first: 6) { nodes { name color } }
      reviewRequests(first: 10) {
        nodes { requestedReviewer { __typename ... on User { login } ... on Team { slug } } }
      }
      commits(last: 1) { nodes { commit { ${CHECKS} } } }
    }
  }
  merged: pullRequests(states: MERGED, first: 20, orderBy: { field: UPDATED_AT, direction: DESC }) {
    nodes {
      id number title url mergedAt baseRefName
      author { login avatarUrl }
      mergedBy { login }
    }
  }
  releases(first: 6, orderBy: { field: CREATED_AT, direction: DESC }) {
    nodes {
      id name tagName url publishedAt isPrerelease isLatest isDraft descriptionHTML
      author { login }
    }
  }`;

export function buildDashboardQuery(repos: string[]): string {
  const parts = repos.map((full, i) => {
    const [owner, name] = full.split("/");
    return `r${i}: repository(owner: ${JSON.stringify(owner)}, name: ${JSON.stringify(name)}) { ${REPO_FIELDS} }`;
  });
  return `query Dashboard {
    viewer { login }
    rateLimit { remaining limit resetAt cost }
    ${parts.join("\n")}
  }`;
}

export const ENABLE_AUTO_MERGE = `
  mutation($id: ID!, $method: PullRequestMergeMethod!) {
    enablePullRequestAutoMerge(input: { pullRequestId: $id, mergeMethod: $method }) {
      pullRequest { id autoMergeRequest { enabledAt } }
    }
  }`;

export const DISABLE_AUTO_MERGE = `
  mutation($id: ID!) {
    disablePullRequestAutoMerge(input: { pullRequestId: $id }) {
      pullRequest { id }
    }
  }`;
