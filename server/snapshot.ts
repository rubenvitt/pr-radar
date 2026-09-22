import type { PollStatus, Snapshot } from "../shared/types.js";
import { graphql } from "./github.js";
import { normalizeMerged, normalizeOpen, normalizeRelease, normalizeRepo, type RawRepo } from "./normalize.js";
import { buildDashboardQuery } from "./queries.js";

const CHUNK = 8; // Repos pro GraphQL-Request

export interface SnapshotResult {
  snapshot: Snapshot;
  rateLimit: PollStatus["rateLimit"];
}

/** Holt den kompletten Dashboard-Stand von GitHub. Ohne `token` wird der Token des Servers genutzt. */
export async function fetchSnapshot(repos: string[], token?: string): Promise<SnapshotResult> {
  const snapshot: Snapshot = {
    viewer: null,
    fetchedAt: new Date().toISOString(),
    repos: [],
    open: [],
    merged: [],
    releases: [],
  };
  let rateLimit: PollStatus["rateLimit"] = null;
  if (repos.length === 0) return { snapshot, rateLimit };

  const chunks: string[][] = [];
  for (let i = 0; i < repos.length; i += CHUNK) chunks.push(repos.slice(i, i + CHUNK));

  const results = await Promise.all(
    chunks.map(async (chunk) => ({
      chunk,
      res: await graphql<Record<string, any>>(buildDashboardQuery(chunk), {}, token),
    })),
  );

  for (const { chunk, res } of results) {
    if (!res.data) throw new Error(res.errors.map((e) => e.message).join("; ") || "Leere Antwort von GitHub");
    snapshot.viewer ??= res.data.viewer?.login ?? null;
    if (res.data.rateLimit) {
      const { remaining, limit, resetAt } = res.data.rateLimit;
      rateLimit = { remaining, limit, resetAt };
    }
    chunk.forEach((full, i) => {
      const raw = res.data![`r${i}`] as RawRepo | null;
      if (!raw) {
        const err = res.errors.find((e) => e.path?.[0] === `r${i}`);
        snapshot.repos.push({
          fullName: full,
          url: `https://github.com/${full}`,
          defaultBranch: null,
          defaultBranchPipeline: "none",
          autoMergeAllowed: false,
          mergeMethods: [],
          viewerCanMerge: false,
          error: err?.message ?? "Repository nicht gefunden oder kein Zugriff",
        });
        return;
      }
      const name = raw.nameWithOwner;
      snapshot.repos.push(normalizeRepo(raw));
      snapshot.open.push(...raw.open.nodes.filter(Boolean).map((n) => normalizeOpen(name, n)));
      snapshot.merged.push(...raw.merged.nodes.filter((n) => n?.mergedAt).map((n) => normalizeMerged(name, n)));
      snapshot.releases.push(...raw.releases.nodes.filter((n) => n && !n.isDraft).map((n) => normalizeRelease(name, n)));
    });
  }

  snapshot.open.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  snapshot.merged.sort((a, b) => b.mergedAt.localeCompare(a.mergedAt));
  snapshot.releases.sort((a, b) => (b.publishedAt ?? "").localeCompare(a.publishedAt ?? ""));
  return { snapshot, rateLimit };
}
