import { EventEmitter } from "node:events";
import type { PollStatus, Snapshot } from "../shared/types.js";
import { graphql, rest, tokenSource } from "./github.js";
import { buildDashboardQuery } from "./queries.js";
import { applyBranchRules, normalizeMerged, normalizeOpen, normalizeRelease, normalizeRepo, type RawRepo } from "./normalize.js";
import { loadConfig } from "./config.js";

const CHUNK = 8; // Repos pro GraphQL-Request
/** Branch-Regeln ändern sich selten, kosten aber REST-Kontingent. */
const RULES_TTL_MS = 10 * 60_000;

type BranchRules = Parameters<typeof applyBranchRules>[1];

const NORMAL = Number(process.env.POLL_SECONDS ?? 30);
const ACTIVE = Number(process.env.POLL_SECONDS_ACTIVE ?? 10);

export class Poller extends EventEmitter {
  snapshot: Snapshot | null = null;
  status: PollStatus = {
    state: "idle",
    lastSuccess: null,
    nextPollAt: null,
    intervalSeconds: NORMAL,
    error: null,
    rateLimit: null,
    tokenSource: null,
  };
  private timer: NodeJS.Timeout | null = null;
  private running: Promise<void> | null = null;
  private lastHash = "";
  private rulesCache = new Map<string, { at: number; rules: BranchRules }>();

  start() {
    void this.poll();
  }

  /** Sofort neu laden (z. B. nach Auto-Merge-Toggle oder Repo-Änderung). */
  refresh(): Promise<void> {
    return this.poll();
  }

  private setStatus(patch: Partial<PollStatus>) {
    this.status = { ...this.status, ...patch };
    this.emit("status", this.status);
  }

  private poll(): Promise<void> {
    if (this.running) return this.running;
    if (this.timer) clearTimeout(this.timer);
    this.running = this.doPoll().finally(() => {
      this.running = null;
      const anyRunning = this.snapshot?.open.some((p) => p.pipeline.state === "running") ?? false;
      const interval = anyRunning ? ACTIVE : NORMAL;
      const wait = this.status.state === "error" ? Math.max(interval, 30) : interval;
      this.timer = setTimeout(() => void this.poll(), wait * 1000);
      this.setStatus({ intervalSeconds: interval, nextPollAt: new Date(Date.now() + wait * 1000).toISOString() });
    });
    return this.running;
  }

  private async doPoll() {
    this.setStatus({ state: "polling" });
    try {
      const snapshot = await this.fetchSnapshot();
      const hash = JSON.stringify({ ...snapshot, fetchedAt: "" });
      this.snapshot = snapshot;
      if (hash !== this.lastHash) {
        this.lastHash = hash;
        this.emit("snapshot", snapshot);
      }
      this.setStatus({ state: "idle", error: null, lastSuccess: snapshot.fetchedAt, tokenSource: tokenSource() });
    } catch (e) {
      this.setStatus({ state: "error", error: (e as Error).message, tokenSource: tokenSource() });
    }
  }

  /** Je PR die effektiv erlaubten Merge-Methoden setzen (Repo-Einstellungen ∩ Rulesets des Ziel-Branches). */
  private async applyRulesets(snapshot: Snapshot) {
    const keys = [...new Set(snapshot.open.map((p) => `${p.repo}\u0000${p.baseRef}`))];
    const rules = new Map(
      await Promise.all(
        keys.map(async (key) => {
          const cached = this.rulesCache.get(key);
          if (cached && Date.now() - cached.at < RULES_TTL_MS) return [key, cached.rules] as const;
          const [repo, branch] = key.split("\u0000");
          const fetched = await rest<BranchRules>(`/repos/${repo}/rules/branches/${encodeURIComponent(branch)}`);
          if (fetched) this.rulesCache.set(key, { at: Date.now(), rules: fetched });
          // Abruf gescheitert: letzter bekannter Stand gilt weiter.
          return [key, fetched ?? cached?.rules ?? null] as const;
        }),
      ),
    );
    for (const pr of snapshot.open) {
      const repoMethods = snapshot.repos.find((r) => r.fullName === pr.repo)?.mergeMethods ?? [];
      const branchRules = rules.get(`${pr.repo}\u0000${pr.baseRef}`);
      // Regeln nicht lesbar: GitHub lehnt Unerlaubtes beim Mergen ohnehin ab.
      pr.mergeMethods = branchRules ? applyBranchRules(repoMethods, branchRules) : repoMethods;
    }
  }

  private async fetchSnapshot(): Promise<Snapshot> {
    const { repos } = await loadConfig();
    const snapshot: Snapshot = {
      viewer: null,
      fetchedAt: new Date().toISOString(),
      repos: [],
      open: [],
      merged: [],
      releases: [],
    };
    if (repos.length === 0) return snapshot;

    const chunks: string[][] = [];
    for (let i = 0; i < repos.length; i += CHUNK) chunks.push(repos.slice(i, i + CHUNK));

    const results = await Promise.all(
      chunks.map(async (chunk) => ({ chunk, res: await graphql<Record<string, any>>(buildDashboardQuery(chunk)) })),
    );

    for (const { chunk, res } of results) {
      if (!res.data) throw new Error(res.errors.map((e) => e.message).join("; ") || "Leere Antwort von GitHub");
      snapshot.viewer ??= res.data.viewer?.login ?? null;
      if (res.data.rateLimit) {
        const { remaining, limit, resetAt } = res.data.rateLimit;
        this.status.rateLimit = { remaining, limit, resetAt };
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
        snapshot.releases.push(
          ...raw.releases.nodes.filter((n) => n && !n.isDraft).map((n) => normalizeRelease(name, n)),
        );
      });
    }

    await this.applyRulesets(snapshot);
    snapshot.open.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
    snapshot.merged.sort((a, b) => b.mergedAt.localeCompare(a.mergedAt));
    snapshot.releases.sort((a, b) => (b.publishedAt ?? "").localeCompare(a.publishedAt ?? ""));
    return snapshot;
  }
}
