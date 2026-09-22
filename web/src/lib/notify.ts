import type { OpenPR, Snapshot } from "../../../shared/types";

export type Transition = { pr: OpenPR; kind: "failed" | "passed" | "merged" };

/** Ermittelt relevante Zustandswechsel zwischen zwei Snapshots. */
export function diffSnapshots(prev: Snapshot, next: Snapshot): Transition[] {
  const before = new Map(prev.open.map((p) => [p.id, p]));
  const out: Transition[] = [];
  for (const pr of next.open) {
    const old = before.get(pr.id);
    if (!old || old.pipeline.state === pr.pipeline.state) continue;
    if (pr.pipeline.state === "failed") out.push({ pr, kind: "failed" });
    else if (pr.pipeline.state === "passed" && old.pipeline.state === "running") out.push({ pr, kind: "passed" });
  }
  const mergedIds = new Set(next.merged.map((m) => m.id));
  for (const old of prev.open) {
    if (!next.open.some((p) => p.id === old.id) && mergedIds.has(old.id)) out.push({ pr: old, kind: "merged" });
  }
  return out;
}

const TEXT: Record<Transition["kind"], string> = {
  failed: "❌ Pipeline rot",
  passed: "✅ Pipeline grün",
  merged: "🔀 Gemergt",
};

export function showNotification(t: Transition) {
  if (typeof Notification === "undefined" || Notification.permission !== "granted") return;
  const n = new Notification(`${TEXT[t.kind]} · ${t.pr.repo}#${t.pr.number}`, {
    body: t.pr.title,
    icon: t.pr.author?.avatarUrl,
    tag: `${t.pr.id}-${t.kind}`,
  });
  n.onclick = () => window.open(t.pr.url, "_blank");
}
