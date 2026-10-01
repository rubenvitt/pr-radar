import type { ReactNode } from "react";
import { CircleCheck, Eye, GitBranch, GitMerge, GitPullRequestDraft, MessageSquareWarning, ShieldCheck, TriangleAlert, UserCheck, Zap } from "lucide-react";
import type { Check, OpenPR, PipelineState, RepoInfo } from "../../../shared/types";
import { ago } from "../lib/time";
import { DIRECT_MERGE_STATES, hasConflict, MergeControl, METHOD_LABEL } from "./PRRow";
import { Avatar, cx, PipelineIcon, RepoTag } from "./ui";

type Tone = "pass" | "fail" | "run" | "accent" | "idle";

const TONE: Record<Tone, { border: string; tile: string; stroke: string }> = {
  pass: { border: "border-pass/45", tile: "bg-pass/12 text-pass", stroke: "var(--pass)" },
  fail: { border: "border-fail/55", tile: "bg-fail/12 text-fail", stroke: "var(--fail)" },
  run: { border: "border-run/55", tile: "bg-run/14 text-run", stroke: "var(--run)" },
  accent: { border: "border-accent/55", tile: "bg-accent/12 text-accent", stroke: "var(--accent)" },
  idle: { border: "border-line", tile: "bg-panel-2 text-muted", stroke: "var(--muted)" },
};

const STATE_TONE: Record<PipelineState, Tone> = { passed: "pass", failed: "fail", running: "run", none: "idle" };
const STATE_LABEL: Record<PipelineState, string> = { passed: "grün", failed: "rot", running: "läuft", none: "ohne Ergebnis" };
// Reihenfolge beim Sortieren: Rotes und Laufendes zuerst, damit es beim Kürzen sichtbar bleibt.
const STATE_RANK: Record<PipelineState, number> = { failed: 0, running: 1, passed: 2, none: 3 };

// Feste Maße, damit sich die Kanten ohne DOM-Messung berechnen lassen.
const EDGE_W = 40;
const CHECK_H = 44;
const CHECK_GAP = 8;
const MIN_H = 108;
const MAX_CHECK_NODES = 4;

type CheckNode = { key: string; label: string; sub: string; state: PipelineState; url: string | null };

function worst(states: (Check["state"] | PipelineState)[]): PipelineState {
  if (states.includes("failed")) return "failed";
  if (states.includes("running")) return "running";
  if (states.includes("passed")) return "passed";
  return "none";
}

/** Checks nach Workflow bündeln – ein Knoten pro Workflow. */
function checkNodes(pr: OpenPR): CheckNode[] {
  const groups = new Map<string, Check[]>();
  for (const c of pr.pipeline.checks) {
    const key = c.group ?? c.name;
    groups.set(key, [...(groups.get(key) ?? []), c]);
  }
  if (!groups.size) return [{ key: "none", label: "Keine Checks", sub: "letzter Commit", state: "none", url: null }];

  const nodes = [...groups].map(([label, checks]): CheckNode => {
    const state = worst(checks.map((c) => c.state));
    const passed = checks.filter((c) => c.state === "passed").length;
    return {
      key: label,
      label,
      sub: checks.length > 1 ? `${passed}/${checks.length} grün` : checks[0].group ? checks[0].name : STATE_LABEL[state],
      state,
      url: (checks.find((c) => c.state === state) ?? checks[0]).url,
    };
  });
  nodes.sort((a, b) => STATE_RANK[a.state] - STATE_RANK[b.state]);
  if (nodes.length <= MAX_CHECK_NODES) return nodes;

  const rest = nodes.slice(MAX_CHECK_NODES - 1);
  const state = worst(rest.map((n) => n.state));
  return [
    ...nodes.slice(0, MAX_CHECK_NODES - 1),
    { key: "more", label: `+${rest.length} weitere`, sub: STATE_LABEL[state], state, url: pr.url },
  ];
}

function reviewNode(pr: OpenPR, viewer: string | null) {
  if (viewer && pr.reviewRequests.includes(viewer) && pr.reviewDecision !== "APPROVED")
    return { tone: "accent" as Tone, icon: <UserCheck size={17} />, title: "Dein Review", sub: "angefragt" };
  switch (pr.reviewDecision) {
    case "APPROVED":
      return { tone: "pass" as Tone, icon: <ShieldCheck size={17} />, title: "Approved", sub: "Review ok" };
    case "CHANGES_REQUESTED":
      return { tone: "fail" as Tone, icon: <MessageSquareWarning size={17} />, title: "Änderungen", sub: "angefordert" };
    case "REVIEW_REQUIRED":
      return { tone: "idle" as Tone, icon: <Eye size={17} />, title: "Review offen", sub: pr.reviewRequests.join(", ") || "ausstehend" };
    default:
      return { tone: "idle" as Tone, icon: <Eye size={17} />, title: "Review", sub: "nicht erforderlich" };
  }
}

function mergeNode(pr: OpenPR) {
  if (hasConflict(pr)) return { tone: "fail" as Tone, icon: <TriangleAlert size={17} />, title: "Konflikt", sub: "nicht mergebar" };
  if (pr.isDraft) return { tone: "idle" as Tone, icon: <GitPullRequestDraft size={17} />, title: "Draft", sub: "noch nicht bereit" };
  if (pr.autoMerge) return { tone: "accent" as Tone, icon: <Zap size={17} className="fill-current" />, title: "Auto-Merge", sub: `${METHOD_LABEL[pr.autoMerge.method]} · wenn grün` };
  if (pr.mergeable === "MERGEABLE" && DIRECT_MERGE_STATES.includes(pr.mergeStateStatus))
    return { tone: "pass" as Tone, icon: <GitMerge size={17} />, title: "Bereit", sub: "kann gemergt werden" };
  if (pr.mergeStateStatus === "BEHIND") return { tone: "run" as Tone, icon: <GitMerge size={17} />, title: "Veraltet", sub: "Base ist weiter" };
  if (pr.mergeStateStatus === "BLOCKED") return { tone: "idle" as Tone, icon: <GitMerge size={17} />, title: "Blockiert", sub: "wartet auf Regeln" };
  return { tone: "idle" as Tone, icon: <GitMerge size={17} />, title: "Merge", sub: pr.mergeStateStatus.toLowerCase() };
}

function Node({ tone, icon, title, sub, href, compact, className, children, ports = "both" }: {
  tone: Tone;
  icon: ReactNode;
  title: ReactNode;
  sub?: ReactNode;
  href?: string | null;
  compact?: boolean;
  className?: string;
  children?: ReactNode;
  ports?: "in" | "out" | "both";
}) {
  const t = TONE[tone];
  const handle = "absolute top-1/2 size-2.5 -translate-y-1/2 rounded-full border-2 bg-panel";
  const content = (
    <>
      {ports !== "out" && <span className={cx(handle, "-left-[6px]")} style={{ borderColor: t.stroke }} />}
      {ports !== "in" && <span className={cx(handle, "-right-[6px]")} style={{ borderColor: t.stroke }} />}
      {tone === "pass" && <CircleCheck size={15} className="absolute -right-2 -top-2 rounded-full bg-panel text-pass" />}
      <span className={cx("flex shrink-0 items-center justify-center rounded-lg", compact ? "size-7" : "size-9", t.tile)}>{icon}</span>
      <span className="min-w-0 flex-1">
        <span className={cx("block text-sm font-medium leading-snug", compact ? "truncate" : "line-clamp-2")}>{title}</span>
        {sub && <span className="block truncate text-[11px] text-muted">{sub}</span>}
        {children}
      </span>
    </>
  );
  const cls = cx(
    "relative flex items-center gap-2.5 rounded-xl border-[1.5px] bg-panel px-2.5 shadow-sm transition",
    compact ? "h-11" : "py-2.5",
    t.border,
    className,
  );
  return href ? (
    <a href={href} target="_blank" rel="noreferrer" className={cx(cls, "hover:-translate-y-px hover:shadow-md")}>{content}</a>
  ) : (
    <div className={cls}>{content}</div>
  );
}

function Edges({ height, links }: { height: number; links: { y1: number; y2: number; tone: Tone }[] }) {
  const w = EDGE_W;
  return (
    <svg width={w} height={height} className="shrink-0 overflow-visible" aria-hidden>
      {links.map((l, i) => (
        <path
          key={i}
          d={`M0 ${l.y1} C ${w / 2} ${l.y1}, ${w / 2} ${l.y2}, ${w} ${l.y2}`}
          fill="none"
          stroke={TONE[l.tone].stroke}
          strokeWidth={2}
          strokeOpacity={l.tone === "idle" ? 0.45 : 0.85}
          className={cx(l.tone === "run" && "edge-run", l.tone === "idle" && "edge-idle")}
        />
      ))}
    </svg>
  );
}

function FlowRow({ pr, repo, viewer, showRepo, onError }: { pr: OpenPR; repo?: RepoInfo; viewer: string | null; showRepo: boolean; onError: (m: string) => void }) {
  const checks = checkNodes(pr);
  const checksH = checks.length * CHECK_H + (checks.length - 1) * CHECK_GAP;
  const h = Math.max(MIN_H, checksH + 16);
  const mid = h / 2;
  const top = (h - checksH) / 2;
  const ys = checks.map((_, i) => top + i * (CHECK_H + CHECK_GAP) + CHECK_H / 2);

  const review = reviewNode(pr, viewer);
  const merge = mergeNode(pr);
  const onDefault = !!repo?.defaultBranch && pr.baseRef === repo.defaultBranch;
  const baseState = onDefault ? repo!.defaultBranchPipeline : "none";

  return (
    <div className={cx("border-b border-dashed border-line px-4 py-2 last:border-b-0", hasConflict(pr) && "bg-fail/4")}>
      <div className="flex items-center" style={{ height: h }}>
        <Node
          tone={viewer && pr.author?.login === viewer ? "accent" : "idle"}
          icon={<Avatar person={pr.author} size={24} />}
          ports="out"
          className="min-w-56 flex-1"
          title={
            <a href={pr.url} target="_blank" rel="noreferrer" className={cx("hover:text-accent", pr.isDraft && "text-muted")} title={pr.title}>
              {pr.title}
            </a>
          }
          sub={
            <span className="inline-flex max-w-full items-center gap-1.5">
              {showRepo && <RepoTag repo={pr.repo} />}
              <span className="tabular-nums">#{pr.number}</span>
              <span className="truncate font-mono">{pr.headRef}</span>
              <span className="shrink-0">· {ago(pr.updatedAt)}</span>
            </span>
          }
        />

        <Edges height={h} links={checks.map((c, i) => ({ y1: mid, y2: ys[i], tone: STATE_TONE[c.state] }))} />
        <div className="flex w-48 shrink-0 flex-col justify-center gap-2">
          {checks.map((c) => (
            <Node key={c.key} compact tone={STATE_TONE[c.state]} icon={<PipelineIcon state={c.state} size={15} />} title={c.label} sub={c.sub} href={c.url} />
          ))}
        </div>
        <Edges height={h} links={checks.map((c, i) => ({ y1: ys[i], y2: mid, tone: STATE_TONE[c.state] }))} />

        <Node tone={review.tone} icon={review.icon} title={review.title} sub={review.sub} className="w-40 shrink-0" />
        <Edges height={h} links={[{ y1: mid, y2: mid, tone: review.tone }]} />

        <Node tone={merge.tone} icon={merge.icon} title={merge.title} sub={merge.sub} className="w-44 shrink-0">
          <div className="mt-1.5">
            <MergeControl pr={pr} repo={repo} onError={onError} />
          </div>
        </Node>
        <Edges height={h} links={[{ y1: mid, y2: mid, tone: merge.tone }]} />

        <Node
          tone={STATE_TONE[baseState]}
          icon={<GitBranch size={17} />}
          title={<span className="font-mono text-[13px]">{pr.baseRef}</span>}
          sub={onDefault ? `Pipeline ${STATE_LABEL[baseState]}` : "Ziel-Branch"}
          className="w-36 shrink-0"
          ports="in"
        />
      </div>
    </div>
  );
}

/** Node-Canvas im n8n-Stil: PR → Workflows → Review → Merge → Ziel-Branch. */
export function FlowView({ prs, repoMap, viewer, showRepo, onError }: {
  prs: OpenPR[];
  repoMap: Map<string, RepoInfo>;
  viewer: string | null;
  showRepo: boolean;
  onError: (m: string) => void;
}) {
  return (
    <div className="flow-canvas overflow-x-auto rounded-xl border border-line xl:overflow-visible">
      <div className="min-w-[68rem]">
        {prs.map((pr) => (
          <FlowRow key={pr.id} pr={pr} repo={repoMap.get(pr.repo)} viewer={viewer} showRepo={showRepo} onError={onError} />
        ))}
      </div>
    </div>
  );
}
