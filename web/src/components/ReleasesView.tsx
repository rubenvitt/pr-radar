import { useState } from "react";
import { ChevronDown, ExternalLink, Tag } from "lucide-react";
import type { Release } from "../../../shared/types";
import { ago } from "../lib/time";
import { Badge, cx, Empty, RepoTag } from "./ui";

function ReleaseCard({ r, defaultOpen }: { r: Release; defaultOpen: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  const hasBody = r.descriptionHTML.trim().length > 0;
  return (
    <article className="overflow-hidden rounded-xl border border-line bg-panel">
      <header className="flex cursor-pointer items-center gap-3 px-4 py-3 hover:bg-panel-2/40" onClick={() => setOpen((o) => !o)}>
        <Tag size={17} className="shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-semibold">{r.name}</span>
            {r.name !== r.tagName && <span className="font-mono text-xs text-muted">{r.tagName}</span>}
            {r.isLatest && <Badge tone="pass">Latest</Badge>}
            {r.isPrerelease && <Badge tone="run">Pre-Release</Badge>}
          </div>
          <div className="mt-0.5 flex flex-wrap items-center gap-x-3 text-xs text-muted">
            <RepoTag repo={r.repo} short={false} />
            {r.author && <span>von {r.author}</span>}
            <span title={r.publishedAt ? new Date(r.publishedAt).toLocaleString("de-DE") : ""}>{ago(r.publishedAt)}</span>
          </div>
        </div>
        <a href={r.url} target="_blank" rel="noreferrer" onClick={(e) => e.stopPropagation()} className="rounded-md p-1.5 text-muted hover:bg-panel-2 hover:text-fg" title="Auf GitHub öffnen">
          <ExternalLink size={15} />
        </a>
        {hasBody && <ChevronDown size={16} className={cx("text-muted transition-transform", open && "rotate-180")} />}
      </header>
      {open && hasBody && (
        // descriptionHTML ist von GitHub gerendertes und bereinigtes HTML
        <div className="release-body border-t border-line px-5 py-4" dangerouslySetInnerHTML={{ __html: r.descriptionHTML }} />
      )}
    </article>
  );
}

export function ReleasesView({ items }: { items: Release[] }) {
  if (!items.length) return <Empty icon={<Tag size={28} />} title="Keine Releases" />;
  const seen = new Set<string>();
  return (
    <div className="space-y-3">
      {items.map((r) => {
        // Neuestes Release je Repo aufgeklappt
        const first = !seen.has(r.repo);
        seen.add(r.repo);
        return <ReleaseCard key={r.id} r={r} defaultOpen={first} />;
      })}
    </div>
  );
}
