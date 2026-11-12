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

    /// Höchstzahl offener PRs je Repo und Abruf (so viele wie vor der Anpassung an die Repo-Größe).
    static let maxOpenPage = 40
    /// Kleinste Seite für offene PRs.
    static let minOpenPage = 5
    /// Puffer über der zuletzt gesehenen Zahl offener PRs, damit neue PRs ohne Nachladen reinpassen.
    static let openPageHeadroom = 3

    /// Seitengröße für die offenen PRs eines Repos. GitHub berechnet die Kosten einer Abfrage nach
    /// dem `first` jeder Verbindung, nicht nach den tatsächlich gelieferten Knoten: Jeder offene PR
    /// bringt vier verschachtelte Verbindungen mit (Labels, Reviewer, Commit, Checks). Ein Repo mit
    /// zwei offenen PRs kostet mit `first: 5` daher rund ein Siebtel von `first: 40`.
    /// Unbekannte Repos (erster Abruf) bekommen die volle Seite.
    static func openPageSize(knownOpen: Int?) -> Int {
        guard let n = knownOpen else { return maxOpenPage }
        let rounded = (max(n, 0) + openPageHeadroom + minOpenPage - 1) / minOpenPage * minOpenPage
        return min(max(rounded, minOpenPage), maxOpenPage)
    }

    private static func repoFields(openPage: Int) -> String { """
          nameWithOwner url
          autoMergeAllowed squashMergeAllowed mergeCommitAllowed rebaseMergeAllowed
          viewerPermission
          defaultBranchRef { name target { ... on Commit { statusCheckRollup { state } } } }
          open: pullRequests(states: OPEN, first: \(openPage), orderBy: { field: UPDATED_AT, direction: DESC }) {
            totalCount
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
    }

    /// Ein Request für mehrere Repos, aliased als `r0`, `r1`, …; ohne Angabe mit voller Seite.
    static func dashboard(_ repos: [String]) -> String {
        dashboard(repos.map { ($0, maxOpenPage) })
    }

    /// Ein Request für mehrere Repos, aliased als `r0`, `r1`, …; je Repo mit eigener Seitengröße
    /// für die offenen PRs.
    static func dashboard(_ repos: [(String, Int)]) -> String {
        let parts = repos.enumerated().map { i, entry in
            let (full, openPage) = entry
            let split = full.split(separator: "/", maxSplits: 1).map(String.init)
            let owner = split.first ?? full
            let name = split.count > 1 ? split[1] : ""
            return "r\(i): repository(owner: \(jsonString(owner)), name: \(jsonString(name))) { \(repoFields(openPage: openPage)) }"
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

    /// Repos, die das Konto sieht: eigene, als Mitarbeiter und über Organisationen, zuletzt
    /// aktive zuerst. Kostet einen Punkt je Seite. Wie `VIEWER_REPOS` der macOS-App.
    static let viewerRepos = """
          query($after: String) {
            viewer {
              repositories(
                first: 100
                after: $after
                affiliations: [OWNER, COLLABORATOR, ORGANIZATION_MEMBER]
                ownerAffiliations: [OWNER, COLLABORATOR, ORGANIZATION_MEMBER]
                orderBy: { field: PUSHED_AT, direction: DESC }
              ) {
                pageInfo { hasNextPage endCursor }
                nodes { nameWithOwner isPrivate isArchived description }
              }
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
