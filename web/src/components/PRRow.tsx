import { useState } from "react";
import { ChevronDown, ExternalLink, GitBranch, GitMerge, MessageSquareWarning, ShieldCheck, TriangleAlert, Zap } from "lucide-react";
import type { MergeMethod, OpenPR, RepoInfo } from "../../../shared/types";
import { api } from "../lib/api";
import { ago } from "../lib/time";
import { Avatar, Badge, cx, PipelineIcon, RepoTag } from "./ui";

const METHOD_LABEL: Record<MergeMethod, string> = { SQUASH: "Squash", MERGE: "Merge", REBASE: "Rebase" };

function ReviewBadge({ pr }: { pr: OpenPR }) {
  if (pr.reviewDecision === "APPROVED") return <Badge tone="pass"><ShieldCheck size={12} />Approved</Badge>;
  if (pr.reviewDecision === "CHANGES_REQUESTED") return <Badge tone="fail"><MessageSquareWarning size={12} />Änderungen</Badge>;
  if (pr.reviewDecision === "REVIEW_REQUIRED") return <Badge>Review offen</Badge>;
  return null;
}

function AutoMergeControl({ pr, repo, onError }: { pr: OpenPR; repo?: RepoInfo; onError: (msg: string) => void }) {
  const [busy, setBusy] = useState(false);
  const [menu, setMenu] = useState(false);
  const methods = repo?.mergeMethods.length ? repo.mergeMethods : (["SQUASH"] as MergeMethod[]);
  const allowed = repo?.autoMergeAllowed ?? false;
  const canMerge = repo?.viewerCanMerge ?? false;

  const run = async (enable: boolean, method?: MergeMethod) => {
    setMenu(false);
    setBusy(true);
    try {
      await api.autoMerge(pr.id, enable, method);
    } catch (e) {
      onError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const on = !!pr.autoMerge;
  const disabled = busy || (!on && (!allowed || !canMerge || pr.isDraft));
  const title = on
    ? `Auto-Merge aktiv (${METHOD_LABEL[pr.autoMerge!.method]}${pr.autoMerge!.enabledBy ? `, von ${pr.autoMerge!.enabledBy}` : ""}) – klicken zum Deaktivieren`
    : !allowed
      ? "Auto-Merge ist in diesem Repo nicht erlaubt (Settings → Allow auto-merge)"
      : !canMerge
        ? "Keine Schreibrechte"
        : pr.isDraft
          ? "Draft-PRs können kein Auto-Merge"
          : "Auto-Merge aktivieren";

  return (
    <div className="relative">
      <button
        type="button"
        disabled={disabled}
        title={title}
        onClick={(e) => {
          e.stopPropagation();
          if (on) void run(false);
          else if (methods.length > 1) setMenu((m) => !m);
          else void run(true, methods[0]);
        }}
        className={cx(
          "inline-flex h-7 items-center gap-1.5 rounded-full border px-2.5 text-xs font-medium transition",
          on
            ? "border-accent/40 bg-accent/15 text-accent hover:bg-accent/25"
            : "border-line text-muted hover:border-accent/50 hover:text-fg",
          disabled && !on && "opacity-40 cursor-not-allowed hover:border-line hover:text-muted",
          busy && "opacity-60",
        )}
      >
        <Zap size={13} className={cx(on && "fill-current")} />
        {on ? `Auto · ${METHOD_LABEL[pr.autoMerge!.method]}` : "Auto-Merge"}
      </button>
      {menu && (
        <>
          <div className="fixed inset-0 z-10" onClick={(e) => { e.stopPropagation(); setMenu(false); }} />
          <div className="absolute right-0 top-8 z-20 min-w-36 overflow-hidden rounded-lg border border-line bg-panel shadow-xl">
            {methods.map((m) => (
              <button
                key={m}
                type="button"
                onClick={(e) => { e.stopPropagation(); void run(true, m); }}
                className="block w-full px-3 py-2 text-left text-sm hover:bg-panel-2"
              >
                {METHOD_LABEL[m]}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

export function PRRow({ pr, repo, viewer, showRepo, onError }: { pr: OpenPR; repo?: RepoInfo; viewer: string | null; showRepo: boolean; onError: (m: string) => void }) {
  const [open, setOpen] = useState(false);
  const p = pr.pipeline;
  const reviewForMe = !!viewer && pr.reviewRequests.includes(viewer);

  return (
    <div className={cx("group border-b border-line last:border-b-0 transition-colors", open ? "bg-panel-2/60" : "hover:bg-panel-2/40")}>
      <div className="flex cursor-pointer items-start gap-3 px-4 py-3" onClick={() => setOpen((o) => !o)}>
        <div className="pt-0.5"><PipelineIcon state={p.state} /></div>

        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <a
              href={pr.url}
              target="_blank"
              rel="noreferrer"
              onClick={(e) => e.stopPropagation()}
              className={cx("font-medium leading-snug hover:text-accent", pr.isDraft && "text-muted")}
            >
              {pr.title}
            </a>
            <span className="text-sm text-muted tabular-nums">#{pr.number}</span>
            {pr.isDraft && <Badge>Draft</Badge>}
            {pr.mergeable === "CONFLICTING" && <Badge tone="fail"><TriangleAlert size={12} />Konflikt</Badge>}
            {reviewForMe && <Badge tone="accent">Dein Review</Badge>}
            {pr.labels.map((l) => (
              <span
                key={l.name}
                className="rounded-full px-1.5 py-px text-[11px] font-medium"
                style={{ background: `#${l.color}26`, color: `color-mix(in oklab, #${l.color} 70%, var(--fg))` }}
              >
                {l.name}
              </span>
            ))}
          </div>

          <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted">
            {showRepo && <RepoTag repo={pr.repo} />}
            <span className="inline-flex items-center gap-1.5">
              <Avatar person={pr.author} size={16} />
              {pr.author?.login ?? "ghost"}
            </span>
            <span className="inline-flex min-w-0 items-center gap-1 font-mono text-[11px]">
              <GitBranch size={12} className="shrink-0" />
              <span className="max-w-56 truncate">{pr.headRef}</span>
              <span className="opacity-60">→ {pr.baseRef}</span>
            </span>
            <span className="tabular-nums"><span className="text-pass">+{pr.additions}</span> <span className="text-fail">−{pr.deletions}</span></span>
            <span title={new Date(pr.updatedAt).toLocaleString("de-DE")}>aktualisiert {ago(pr.updatedAt)}</span>
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          <ReviewBadge pr={pr} />
          {p.total > 0 && (
            <span className="hidden text-xs tabular-nums text-muted sm:inline" title={`${p.passed} grün · ${p.running} laufend · ${p.failed} rot`}>
              {p.passed}/{p.total}
            </span>
          )}
          <AutoMergeControl pr={pr} repo={repo} onError={onError} />
          <ChevronDown size={16} className={cx("text-muted transition-transform", open && "rotate-180")} />
        </div>
      </div>

      {open && (
        <div className="px-4 pb-4 pl-11">
          {p.checks.length === 0 ? (
            <div className="text-sm text-muted">Keine Checks für den letzten Commit.</div>
          ) : (
            <div className="grid gap-1 sm:grid-cols-2">
              {p.checks.map((c, i) => (
                <a
                  key={`${c.name}-${i}`}
                  href={c.url ?? pr.url}
                  target="_blank"
                  rel="noreferrer"
                  className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-panel"
                >
                  <PipelineIcon state={c.state} size={15} />
                  <span className="min-w-0 flex-1 truncate">
                    {c.group && <span className="text-muted">{c.group} / </span>}
                    {c.name}
                  </span>
                  <ExternalLink size={12} className="text-muted opacity-0 group-hover:opacity-100" />
                </a>
              ))}
            </div>
          )}
          <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted">
            <span className="inline-flex items-center gap-1"><GitMerge size={12} />Merge-Status: {pr.mergeStateStatus.toLowerCase()}</span>
            {pr.reviewRequests.length > 0 && <span>Review angefragt: {pr.reviewRequests.join(", ")}</span>}
            <span>erstellt {ago(pr.createdAt)}</span>
          </div>
        </div>
      )}
    </div>
  );
}
