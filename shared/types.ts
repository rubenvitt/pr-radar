export type PipelineState = "running" | "failed" | "passed" | "none";
export type MergeMethod = "SQUASH" | "MERGE" | "REBASE";

export interface Check {
  name: string;
  state: "running" | "failed" | "passed" | "skipped" | "neutral";
  url: string | null;
  /** z. B. Workflow-Name bei GitHub Actions */
  group?: string | null;
}

export interface Pipeline {
  state: PipelineState;
  total: number;
  passed: number;
  failed: number;
  running: number;
  checks: Check[];
}

export interface Person {
  login: string;
  avatarUrl: string;
}

export interface Label {
  name: string;
  color: string;
}

export interface OpenPR {
  id: string;
  repo: string;
  number: number;
  title: string;
  url: string;
  author: Person | null;
  isDraft: boolean;
  createdAt: string;
  updatedAt: string;
  headRef: string;
  baseRef: string;
  additions: number;
  deletions: number;
  reviewDecision: "APPROVED" | "CHANGES_REQUESTED" | "REVIEW_REQUIRED" | null;
  mergeable: "MERGEABLE" | "CONFLICTING" | "UNKNOWN";
  mergeStateStatus: string;
  autoMerge: { method: MergeMethod; enabledBy: string | null; enabledAt: string } | null;
  labels: Label[];
  reviewRequests: string[];
  pipeline: Pipeline;
}

export interface MergedPR {
  id: string;
  repo: string;
  number: number;
  title: string;
  url: string;
  author: Person | null;
  mergedAt: string;
  mergedBy: string | null;
  baseRef: string;
}

export interface Release {
  id: string;
  repo: string;
  name: string;
  tagName: string;
  url: string;
  publishedAt: string | null;
  isPrerelease: boolean;
  isLatest: boolean;
  author: string | null;
  descriptionHTML: string;
}

export interface RepoInfo {
  fullName: string;
  url: string;
  defaultBranch: string | null;
  defaultBranchPipeline: PipelineState;
  autoMergeAllowed: boolean;
  mergeMethods: MergeMethod[];
  viewerCanMerge: boolean;
  error: string | null;
}

export interface Snapshot {
  viewer: string | null;
  fetchedAt: string;
  repos: RepoInfo[];
  open: OpenPR[];
  merged: MergedPR[];
  releases: Release[];
}

export interface PollStatus {
  state: "idle" | "polling" | "error";
  lastSuccess: string | null;
  nextPollAt: string | null;
  intervalSeconds: number;
  error: string | null;
  rateLimit: { remaining: number; limit: number; resetAt: string } | null;
  tokenSource: "env" | "gh-cli" | null;
}

export type ServerEvent =
  | { type: "snapshot"; data: Snapshot }
  | { type: "status"; data: PollStatus };
