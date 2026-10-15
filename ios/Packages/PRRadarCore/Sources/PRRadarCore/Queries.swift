// GraphQL-Dokumente für Dashboard-Abfrage und Merge-Mutationen – identisch zur macOS-App.

import Foundation

enum Queries {
    private static let checks = """
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
          }
        """

    private static let repoFields = """
          nameWithOwner url
          autoMergeAllowed squashMergeAllowed mergeCommitAllowed rebaseMergeAllowed
          viewerPermission
          defaultBranchRef { name target { ... on Commit { statusCheckRollup { state } } } }
          open: pullRequests(states: OPEN, first: 40, orderBy: { field: UPDATED_AT, direction: DESC }) {
            nodes {
              id number title url isDraft createdAt updatedAt
              headRefName headRefOid baseRefName additions deletions
              reviewDecision mergeable mergeStateStatus
              author { login avatarUrl }
              autoMergeRequest { enabledAt mergeMethod enabledBy { login } }
              labels(first: 6) { nodes { name color } }
              reviewRequests(first: 10) {
                nodes { requestedReviewer { __typename ... on User { login } ... on Team { slug } } }
              }
              commits(last: 1) { nodes { commit { \(checks) } } }
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
          }
        """

    /// Ein Request für mehrere Repos, aliased als `r0`, `r1`, …
    static func dashboard(_ repos: [String]) -> String {
        let parts = repos.enumerated().map { i, full in
            let split = full.split(separator: "/", maxSplits: 1).map(String.init)
            let owner = split.first ?? full
            let name = split.count > 1 ? split[1] : ""
            return "r\(i): repository(owner: \(jsonString(owner)), name: \(jsonString(name))) { \(repoFields) }"
        }
        return """
            query Dashboard {
              viewer { login }
              rateLimit { remaining limit resetAt cost }
              \(parts.joined(separator: "\n"))
            }
            """
    }

    static let enableAutoMerge = """
          mutation($id: ID!, $method: PullRequestMergeMethod!, $head: GitObjectID) {
            enablePullRequestAutoMerge(input: { pullRequestId: $id, mergeMethod: $method, expectedHeadOid: $head }) {
              pullRequest { id autoMergeRequest { enabledAt } }
            }
          }
        """

    static let disableAutoMerge = """
          mutation($id: ID!) {
            disablePullRequestAutoMerge(input: { pullRequestId: $id }) {
              pullRequest { id }
            }
          }
        """

    static let mergePR = """
          mutation($id: ID!, $method: PullRequestMergeMethod!, $head: GitObjectID) {
            mergePullRequest(input: { pullRequestId: $id, mergeMethod: $method, expectedHeadOid: $head }) {
              pullRequest { id merged }
            }
          }
        """

    /// Als GraphQL-String-Literal escaped (JSON-Syntax ist kompatibel).
    private static func jsonString(_ s: String) -> String {
        guard let data = try? JSONEncoder().encode(s), let out = String(data: data, encoding: .utf8)
        else { return "\"\"" }
        return out
    }
}
