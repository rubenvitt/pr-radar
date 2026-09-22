import { GitMerge } from "lucide-react";
import type { MergedPR } from "../../../shared/types";
import { ago, clock, dayLabel } from "../lib/time";
import { Avatar, Empty, RepoTag } from "./ui";

export function MergedView({ items }: { items: MergedPR[] }) {
  if (!items.length) return <Empty icon={<GitMerge size={28} />} title="Noch nichts gemergt" />;

  const groups: [string, MergedPR[]][] = [];
  for (const m of items) {
    const label = dayLabel(m.mergedAt);
    const last = groups.at(-1);
    if (last && last[0] === label) last[1].push(m);
    else groups.push([label, [m]]);
  }

  return (
    <div className="space-y-6">
      {groups.map(([label, list]) => (
        <section key={label}>
          <h3 className="mb-2 px-1 text-xs font-semibold uppercase tracking-wider text-muted">{label}</h3>
          <div className="overflow-hidden rounded-xl border border-line bg-panel">
            {list.map((m) => (
              <a
                key={m.id}
                href={m.url}
                target="_blank"
                rel="noreferrer"
                className="flex items-center gap-3 border-b border-line px-4 py-3 last:border-b-0 hover:bg-panel-2/50"
              >
                <GitMerge size={17} className="shrink-0 text-accent" />
                <div className="min-w-0 flex-1">
                  <div className="truncate font-medium">
                    {m.title} <span className="font-normal text-muted">#{m.number}</span>
                  </div>
                  <div className="mt-0.5 flex flex-wrap items-center gap-x-3 text-xs text-muted">
                    <RepoTag repo={m.repo} />
                    <span className="inline-flex items-center gap-1.5">
                      <Avatar person={m.author} size={15} />
                      {m.author?.login ?? "ghost"}
                    </span>
                    {m.mergedBy && m.mergedBy !== m.author?.login && <span>gemergt von {m.mergedBy}</span>}
                    <span className="font-mono text-[11px]">→ {m.baseRef}</span>
                  </div>
                </div>
                <div className="shrink-0 text-right text-xs text-muted tabular-nums" title={new Date(m.mergedAt).toLocaleString("de-DE")}>
                  <div>{clock(m.mergedAt)}</div>
                  <div className="opacity-70">{ago(m.mergedAt)}</div>
                </div>
              </a>
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
