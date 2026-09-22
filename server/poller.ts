import { EventEmitter } from "node:events";
import type { PollStatus, Snapshot } from "../shared/types.js";
import { loadConfig } from "./config.js";
import { findServerToken, NO_SERVER_TOKEN, tokenSource } from "./github.js";
import { fetchSnapshot } from "./snapshot.js";

export const POLL_NORMAL = Number(process.env.POLL_SECONDS ?? 30);
export const POLL_ACTIVE = Number(process.env.POLL_SECONDS_ACTIVE ?? 10);

export class Poller extends EventEmitter {
  snapshot: Snapshot | null = null;
  status: PollStatus = {
    state: "idle",
    lastSuccess: null,
    nextPollAt: null,
    intervalSeconds: POLL_NORMAL,
    error: null,
    rateLimit: null,
    tokenSource: null,
    serverToken: true,
  };
  private timer: NodeJS.Timeout | null = null;
  private running: Promise<void> | null = null;
  private lastHash = "";

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
      const interval = anyRunning ? POLL_ACTIVE : POLL_NORMAL;
      // Ohne eigenen Token bringt häufiges Pollen nichts – nur gelegentlich prüfen, ob doch einer auftaucht.
      const wait = !this.status.serverToken
        ? Math.max(interval, 60)
        : this.status.state === "error"
          ? Math.max(interval, 30)
          : interval;
      this.timer = setTimeout(() => void this.poll(), wait * 1000);
      this.setStatus({ intervalSeconds: interval, nextPollAt: new Date(Date.now() + wait * 1000).toISOString() });
    });
    return this.running;
  }

  private async doPoll() {
    this.setStatus({ state: "polling" });
    try {
      // Kein serverseitiger Token (typisch für die gehostete Variante): Der Browser muss einen mitbringen.
      if (!(await findServerToken())) {
        this.setStatus({ state: "error", error: NO_SERVER_TOKEN, tokenSource: null, serverToken: false });
        return;
      }
      const { repos } = await loadConfig();
      const { snapshot, rateLimit } = await fetchSnapshot(repos);
      const hash = JSON.stringify({ ...snapshot, fetchedAt: "" });
      this.snapshot = snapshot;
      if (hash !== this.lastHash) {
        this.lastHash = hash;
        this.emit("snapshot", snapshot);
      }
      this.setStatus({
        state: "idle",
        error: null,
        lastSuccess: snapshot.fetchedAt,
        rateLimit: rateLimit ?? this.status.rateLimit,
        tokenSource: tokenSource(),
        serverToken: true,
      });
    } catch (e) {
      this.setStatus({ state: "error", error: (e as Error).message, tokenSource: tokenSource() });
    }
  }
}
