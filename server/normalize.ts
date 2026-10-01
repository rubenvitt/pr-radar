import type {
  Check,
  MergeMethod,
  MergedPR,
  OpenPR,
  Pipeline,
  PipelineState,
  Release,
  RepoInfo,
} from "../shared/types.js";

/* ---------- Rohdaten (Ausschnitt der GraphQL-Antwort) ---------- */

type RawContext =
  | {
      __typename: "CheckRun";
      name: string;
      status: string;
      conclusion: string | null;
      detailsUrl: string | null;
      checkSuite?: { workflowRun?: { workflow?: { name?: string } | null } | null } | null;
    }
  | { __typename: "StatusContext"; context: string; state: string; targetUrl: string | null };

interface RawRollup {
  state: string;
  contexts?: { nodes: (RawContext | null)[] } | null;
}

export interface RawRepo {
  nameWithOwner: string;
  url: string;
  autoMergeAllowed: boolean;
  squashMergeAllowed: boolean;
  mergeCommitAllowed: boolean;
  rebaseMergeAllowed: boolean;
  viewerPermission: string | null;
  defaultBranchRef: { name: string; target: { statusCheckRollup?: RawRollup | null } | null } | null;
  open: { nodes: any[] };
  merged: { nodes: any[] };
  releases: { nodes: any[] };
}

/* ---------- Checks & Pipeline ---------- */

const FAILED_CONCLUSIONS = new Set(["FAILURE", "TIMED_OUT", "CANCELLED", "STARTUP_FAILURE", "ACTION_REQUIRED", "STALE"]);

export function normalizeCheck(ctx: RawContext): Check {
  if (ctx.__typename === "StatusContext") {
    const s = ctx.state;
    return {
      name: ctx.context,
      url: ctx.targetUrl,
      state: s === "SUCCESS" ? "passed" : s === "FAILURE" || s === "ERROR" ? "failed" : "running",
    };
  }
  const group = ctx.checkSuite?.workflowRun?.workflow?.name ?? null;
  let state: Check["state"];
  if (ctx.status !== "COMPLETED") state = "running";
  else if (ctx.conclusion === "SUCCESS") state = "passed";
  else if (ctx.conclusion === "SKIPPED") state = "skipped";
  else if (ctx.conclusion === "NEUTRAL") state = "neutral";
  else if (ctx.conclusion && FAILED_CONCLUSIONS.has(ctx.conclusion)) state = "failed";
  else state = "neutral";
  return { name: ctx.name, url: ctx.detailsUrl, state, group };
}

export function rollupState(state: string | null | undefined): PipelineState {
  switch (state) {
    case "SUCCESS":
      return "passed";
    case "FAILURE":
    case "ERROR":
      return "failed";
    case "PENDING":
    case "EXPECTED":
      return "running";
    default:
      return "none";
  }
}

const ORDER: Record<Check["state"], number> = { failed: 0, running: 1, passed: 2, neutral: 3, skipped: 4 };

export function buildPipeline(rollup: RawRollup | null | undefined): Pipeline {
  const checks = (rollup?.contexts?.nodes ?? [])
    .filter((n): n is RawContext => !!n && (n.__typename === "CheckRun" || n.__typename === "StatusContext"))
    .map(normalizeCheck)
    .sort((a, b) => ORDER[a.state] - ORDER[b.state] || a.name.localeCompare(b.name));

  const failed = checks.filter((c) => c.state === "failed").length;
  const running = checks.filter((c) => c.state === "running").length;
  const passed = checks.filter((c) => c.state === "passed").length;

  let state: PipelineState;
  if (failed > 0) state = "failed";
  else if (running > 0) state = "running";
  else if (checks.length > 0) state = "passed";
  else state = rollupState(rollup?.state);

  return { state, total: checks.length, passed, failed, running, checks };
}

/* ---------- PRs, Releases, Repo ---------- */

const person = (a: any) => (a ? { login: a.login, avatarUrl: a.avatarUrl } : null);

export function normalizeOpen(repo: string, n: any): OpenPR {
  const rollup = n.commits?.nodes?.[0]?.commit?.statusCheckRollup;
  return {
    id: n.id,
    repo,
    number: n.number,
    title: n.title,
    url: n.url,
    author: person(n.author),
    isDraft: n.isDraft,
    createdAt: n.createdAt,
    updatedAt: n.updatedAt,
    headRef: n.headRefName,
    baseRef: n.baseRefName,
    additions: n.additions,
    deletions: n.deletions,
    reviewDecision: n.reviewDecision ?? null,
    mergeable: n.mergeable,
    mergeStateStatus: n.mergeStateStatus,
    autoMerge: n.autoMergeRequest
      ? {
          method: n.autoMergeRequest.mergeMethod,
          enabledAt: n.autoMergeRequest.enabledAt,
          enabledBy: n.autoMergeRequest.enabledBy?.login ?? null,
        }
      : null,
    labels: (n.labels?.nodes ?? []).map((l: any) => ({ name: l.name, color: l.color })),
    reviewRequests: (n.reviewRequests?.nodes ?? [])
      .map((r: any) => r.requestedReviewer?.login ?? (r.requestedReviewer?.slug ? `@${r.requestedReviewer.slug}` : null))
      .filter(Boolean),
    pipeline: buildPipeline(rollup),
    mergeMethods: [], // wird nach dem Laden der Rulesets gesetzt
  };
}

/**
 * Schränkt die Repo-Methoden mit den effektiven Branch-Regeln ein (`GET /repos/{repo}/rules/branches/{branch}`):
 * `pull_request.allowed_merge_methods` und `required_linear_history` (verbietet Merge-Commits).
 */
export function applyBranchRules(repoMethods: MergeMethod[], rules: { type: string; parameters?: { allowed_merge_methods?: string[] } }[]): MergeMethod[] {
  let methods = [...repoMethods];
  for (const rule of rules) {
    if (rule.type === "pull_request" && rule.parameters?.allowed_merge_methods) {
      const allowed = rule.parameters.allowed_merge_methods.map((m) => m.toUpperCase());
      methods = methods.filter((m) => allowed.includes(m));
    }
    if (rule.type === "required_linear_history") methods = methods.filter((m) => m !== "MERGE");
  }
  return methods;
}

export function normalizeMerged(repo: string, n: any): MergedPR {
  return {
    id: n.id,
    repo,
    number: n.number,
    title: n.title,
    url: n.url,
    author: person(n.author),
    mergedAt: n.mergedAt,
    mergedBy: n.mergedBy?.login ?? null,
    baseRef: n.baseRefName,
  };
}

export function normalizeRelease(repo: string, n: any): Release {
  return {
    id: n.id,
    repo,
    name: n.name || n.tagName,
    tagName: n.tagName,
    url: n.url,
    publishedAt: n.publishedAt,
    isPrerelease: n.isPrerelease,
    isLatest: n.isLatest,
    author: n.author?.login ?? null,
    descriptionHTML: n.descriptionHTML ?? "",
  };
}

export function normalizeRepo(raw: RawRepo): RepoInfo {
  const methods: MergeMethod[] = [];
  if (raw.squashMergeAllowed) methods.push("SQUASH");
  if (raw.mergeCommitAllowed) methods.push("MERGE");
  if (raw.rebaseMergeAllowed) methods.push("REBASE");
  return {
    fullName: raw.nameWithOwner,
    url: raw.url,
    defaultBranch: raw.defaultBranchRef?.name ?? null,
    defaultBranchPipeline: rollupState(raw.defaultBranchRef?.target?.statusCheckRollup?.state),
    autoMergeAllowed: raw.autoMergeAllowed,
    mergeMethods: methods,
    viewerCanMerge: ["ADMIN", "MAINTAIN", "WRITE"].includes(raw.viewerPermission ?? ""),
    error: null,
  };
}
