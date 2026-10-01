import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Bell, BellOff, GitMerge, GitPullRequest, Inbox, Layers, List, RefreshCw, Search, Settings2, Tag, TriangleAlert, Workflow, X, Zap } from "lucide-react";
import type { OpenPR, PipelineState, RepoInfo, Snapshot } from "../../shared/types";
import { api } from "./lib/api";
import { diffSnapshots, showNotification } from "./lib/notify";
import { usePref } from "./lib/prefs";
import { ago } from "./lib/time";
import { useLive, useTick } from "./lib/useLive";
import { FlowView } from "./components/FlowView";
import { MergedView } from "./components/MergedView";
import { hasConflict, PRRow } from "./components/PRRow";
import { ReleasesView } from "./components/ReleasesView";
import { RepoSettings } from "./components/RepoSettings";
import { cx, Empty, PipelineIcon, RepoTag } from "./components/ui";

type Tab = "open" | "merged" | "releases";
type View = "list" | "flow";
type Quick = "all" | "running" | "failed" | "passed" | "auto" | "conflict" | "review" | "mine";

function Stat({ label, value, active, onClick, icon, tone }: { label: string; value: number; active: boolean; onClick: () => void; icon: ReactNode; tone?: string }) {
  return (
    <button
      onClick={onClick}
      className={cx(
        "flex min-w-0 flex-1 items-center gap-3 rounded-xl border px-3.5 py-2.5 text-left transition outline-none focus-visible:ring-2 focus-visible:ring-accent/50",
        active ? "border-accent/60 bg-accent/8 ring-1 ring-accent/30" : "border-line bg-panel hover:border-muted/40",
      )}
    >
      <span className={cx("shrink-0", tone)}>{icon}</span>
      <span className="min-w-0">
        <span className="block text-xl font-semibold leading-none tabular-nums">{value}</span>
        <span className="mt-1 block truncate text-[11px] text-muted">{label}</span>
      </span>
    </button>
  );
}

export function App() {
  const { snapshot, status, connection } = useLive();
  useTick(20_000);

  const [tab, setTab] = usePref<Tab>("tab", "open");
  const [quick, setQuick] = usePref<Quick>("quick", "all");
  const [grouped, setGrouped] = usePref("grouped", true);
  const [view, setView] = usePref<View>("view", "list");
  const [repoFilter, setRepoFilter] = usePref<string[]>("repos", []);
  const [notify, setNotify] = usePref("notify", false);
  const [query, setQuery] = useState("");
  const [settings, setSettings] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  // Desktop-Benachrichtigungen bei Zustandswechseln
  const prev = useRef<Snapshot | null>(null);
  useEffect(() => {
    if (!snapshot) return;
    if (prev.current && notify) diffSnapshots(prev.current, snapshot).forEach(showNotification);
    prev.current = snapshot;
  }, [snapshot, notify]);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 6000);
    return () => clearTimeout(t);
  }, [toast]);

  // Tastatur: "/" Suche, "r" neu laden, "v" Ansicht, 1-3 Tabs
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.metaKey || e.ctrlKey) return;
      if (e.key === "/") { e.preventDefault(); searchRef.current?.focus(); }
      if (e.key === "r") void api.refresh();
      if (e.key === "v") setView((v) => (v === "flow" ? "list" : "flow"));
      if (e.key === "1") setTab("open");
      if (e.key === "2") setTab("merged");
      if (e.key === "3") setTab("releases");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setTab, setView]);

  const viewer = snapshot?.viewer ?? null;
  const repos = snapshot?.repos ?? [];
  const repoMap = useMemo(() => new Map(repos.map((r) => [r.fullName, r])), [repos]);
  const inRepo = (repo: string) => repoFilter.length === 0 || repoFilter.includes(repo);
  const matches = (text: string) => !query || text.toLowerCase().includes(query.toLowerCase());

  const openBase = (snapshot?.open ?? []).filter((p) => inRepo(p.repo));
  const count = (s: PipelineState) => openBase.filter((p) => p.pipeline.state === s).length;
  const quickFilter: Record<Quick, (p: OpenPR) => boolean> = {
    all: () => true,
    running: (p) => p.pipeline.state === "running",
    failed: (p) => p.pipeline.state === "failed",
    passed: (p) => p.pipeline.state === "passed",
    auto: (p) => !!p.autoMerge,
    conflict: hasConflict,
    review: (p) => !!viewer && p.reviewRequests.includes(viewer),
    mine: (p) => !!viewer && p.author?.login === viewer,
  };
  const openList = openBase.filter(quickFilter[quick]).filter((p) => matches(`${p.title} ${p.repo} #${p.number} ${p.author?.login} ${p.headRef}`));
  const merged = (snapshot?.merged ?? []).filter((m) => inRepo(m.repo) && matches(`${m.title} ${m.repo} #${m.number} ${m.author?.login}`));
  const releases = (snapshot?.releases ?? []).filter((r) => inRepo(r.repo) && matches(`${r.name} ${r.tagName} ${r.repo}`));

  const failing = count("failed");
  useEffect(() => {
    document.title = failing ? `(${failing}) PR Radar` : "PR Radar";
  }, [failing]);

  const groups = useMemo(() => {
    const m = new Map<string, OpenPR[]>();
    for (const p of openList) m.set(p.repo, [...(m.get(p.repo) ?? []), p]);
    return [...m.entries()];
  }, [openList]);

  const toggleRepo = (r: string) => setRepoFilter((f) => (f.includes(r) ? f.filter((x) => x !== r) : [...f, r]));
  const polling = status?.state === "polling";

  const prList = (list: OpenPR[], showRepo: boolean) =>
    view === "flow" ? (
      <FlowView prs={list} repoMap={repoMap} viewer={viewer} showRepo={showRepo} onError={setToast} />
    ) : (
      <div className="overflow-hidden rounded-xl border border-line bg-panel">
        {list.map((pr) => (
          <PRRow key={pr.id} pr={pr} repo={repoMap.get(pr.repo)} viewer={viewer} showRepo={showRepo} onError={setToast} />
        ))}
      </div>
    );

  return (
    <div className="mx-auto max-w-6xl px-4 pb-16 sm:px-6">
      {/* Header */}
      <header className="sticky top-0 z-30 -mx-4 mb-5 flex items-center gap-3 border-b border-line bg-bg/85 px-4 py-3 backdrop-blur sm:-mx-6 sm:px-6">
        <img src="/favicon.svg" alt="" className="size-7" />
        <h1 className="text-lg font-semibold tracking-tight">PR Radar</h1>
        <span
          className="ml-1 inline-flex items-center gap-1.5 rounded-full border border-line px-2 py-0.5 text-[11px] text-muted"
          title={status?.lastSuccess ? `Letzte Aktualisierung: ${new Date(status.lastSuccess).toLocaleTimeString("de-DE")}` : ""}
        >
          <span className={cx("size-1.5 rounded-full", connection === "live" && status?.state !== "error" ? "bg-pass pulse-dot" : connection === "connecting" ? "bg-run" : "bg-fail")} />
          {connection === "offline" ? "offline" : status?.state === "error" ? "Fehler" : `live · ${ago(status?.lastSuccess)}`}
        </span>
        <div className="ml-auto flex items-center gap-1">
          <IconButton title="Jetzt aktualisieren (r)" onClick={() => void api.refresh()}>
            <RefreshCw size={17} className={cx(polling && "spin-slow")} />
          </IconButton>
          <IconButton
            title={notify ? "Benachrichtigungen aus" : "Benachrichtigen bei roter/grüner Pipeline & Merge"}
            onClick={async () => {
              if (!notify && typeof Notification !== "undefined" && Notification.permission !== "granted") {
                if ((await Notification.requestPermission()) !== "granted") return;
              }
              setNotify(!notify);
            }}
          >
            {notify ? <Bell size={17} className="text-accent" /> : <BellOff size={17} />}
          </IconButton>
          <IconButton title="Repositories verwalten" onClick={() => setSettings(true)}>
            <Settings2 size={17} />
          </IconButton>
        </div>
      </header>

      {status?.error && (
        <div className="mb-4 rounded-xl border border-fail/30 bg-fail/8 px-4 py-3 text-sm text-fail">{status.error}</div>
      )}

      {/* Kennzahlen */}
      <div className="mb-5 grid grid-cols-2 gap-2 sm:grid-cols-4 lg:grid-cols-8">
        <Stat label="Offen" value={openBase.length} icon={<GitPullRequest size={18} />} active={tab === "open" && quick === "all"} onClick={() => { setTab("open"); setQuick("all"); }} />
        <Stat label="Pipeline läuft" value={count("running")} icon={<PipelineIcon state="running" />} active={tab === "open" && quick === "running"} onClick={() => { setTab("open"); setQuick("running"); }} />
        <Stat label="Rot" value={count("failed")} icon={<PipelineIcon state="failed" />} active={tab === "open" && quick === "failed"} onClick={() => { setTab("open"); setQuick("failed"); }} />
        <Stat label="Grün" value={count("passed")} icon={<PipelineIcon state="passed" />} active={tab === "open" && quick === "passed"} onClick={() => { setTab("open"); setQuick("passed"); }} />
        <Stat label="Auto-Merge" value={openBase.filter(quickFilter.auto).length} icon={<Zap size={18} />} tone="text-accent" active={tab === "open" && quick === "auto"} onClick={() => { setTab("open"); setQuick("auto"); }} />
        <Stat label="Konflikt" value={openBase.filter(quickFilter.conflict).length} icon={<TriangleAlert size={18} />} tone="text-fail" active={tab === "open" && quick === "conflict"} onClick={() => { setTab("open"); setQuick("conflict"); }} />
        <Stat label="Dein Review" value={openBase.filter(quickFilter.review).length} icon={<Inbox size={18} />} tone="text-accent" active={tab === "open" && quick === "review"} onClick={() => { setTab("open"); setQuick("review"); }} />
        <Stat label="Von dir" value={openBase.filter(quickFilter.mine).length} icon={<GitPullRequest size={18} />} tone="text-muted" active={tab === "open" && quick === "mine"} onClick={() => { setTab("open"); setQuick("mine"); }} />
      </div>

      {/* Tabs + Suche */}
      <div className="mb-3 flex flex-wrap items-center gap-3">
        <nav className="flex rounded-lg border border-line bg-panel p-0.5">
          <TabButton active={tab === "open"} onClick={() => setTab("open")} icon={<GitPullRequest size={15} />} label="Offen" count={openList.length} />
          <TabButton active={tab === "merged"} onClick={() => setTab("merged")} icon={<GitMerge size={15} />} label="Gemergt" count={merged.length} />
          <TabButton active={tab === "releases"} onClick={() => setTab("releases")} icon={<Tag size={15} />} label="Releases" count={releases.length} />
        </nav>
        <label className="relative ml-auto min-w-48 flex-1 sm:max-w-72">
          <Search size={15} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted" />
          <input
            ref={searchRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Escape" && (setQuery(""), e.currentTarget.blur())}
            placeholder="Suchen …  /"
            className="h-9 w-full rounded-lg border border-line bg-panel pl-8 pr-3 text-sm outline-none focus:border-accent"
          />
        </label>
        {tab === "open" && (
          <>
            <IconButton title={view === "flow" ? "Listenansicht (v)" : "Flow-Ansicht (v)"} onClick={() => setView(view === "flow" ? "list" : "flow")} active={view === "flow"}>
              {view === "flow" ? <List size={17} /> : <Workflow size={17} />}
            </IconButton>
            <IconButton title={grouped ? "Flache Liste" : "Nach Repo gruppieren"} onClick={() => setGrouped(!grouped)} active={grouped}>
              <Layers size={17} />
            </IconButton>
          </>
        )}
      </div>

      {/* Repo-Filter */}
      {repos.length > 1 && (
        <div className="mb-4 flex flex-wrap gap-1.5">
          {repos.map((r) => (
            <button
              key={r.fullName}
              onClick={() => toggleRepo(r.fullName)}
              className={cx(
                "rounded-full border px-2.5 py-1 transition",
                repoFilter.includes(r.fullName) ? "border-accent/60 bg-accent/10" : "border-line bg-panel hover:border-muted/40",
                r.error && "opacity-60",
              )}
              title={r.error ?? r.fullName}
            >
              <RepoTag repo={r.fullName} />
            </button>
          ))}
          {repoFilter.length > 0 && (
            <button onClick={() => setRepoFilter([])} className="inline-flex items-center gap-1 rounded-full px-2 py-1 text-xs text-muted hover:text-fg">
              <X size={13} />zurücksetzen
            </button>
          )}
        </div>
      )}

      {/* Inhalt */}
      {!snapshot ? (
        <Empty icon={<RefreshCw size={26} className="spin-slow" />} title="Lade Daten von GitHub …" />
      ) : repos.length === 0 ? (
        <Empty icon={<GitPullRequest size={28} />} title="Noch keine Repositories">
          <button onClick={() => setSettings(true)} className="mt-3 rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-white">Repos hinzufügen</button>
        </Empty>
      ) : tab === "open" ? (
        openList.length === 0 ? (
          <Empty icon={<Inbox size={28} />} title="Nichts offen">Keine Pull Requests für diesen Filter.</Empty>
        ) : grouped ? (
          <div className="space-y-5">
            {groups.map(([repo, list]) => (
              <section key={repo}>
                <RepoHeader repo={repoMap.get(repo)} name={repo} count={list.length} />
                {prList(list, false)}
              </section>
            ))}
          </div>
        ) : (
          prList(openList, true)
        )
      ) : tab === "merged" ? (
        <MergedView items={merged} />
      ) : (
        <ReleasesView items={releases} />
      )}

      {settings && <RepoSettings repos={repos} status={status} onClose={() => setSettings(false)} />}

      {toast && (
        <div className="fixed bottom-5 left-1/2 z-50 flex max-w-lg -translate-x-1/2 items-start gap-3 rounded-xl border border-fail/30 bg-panel px-4 py-3 text-sm shadow-2xl">
          <span className="text-fail">{toast}</span>
          <button onClick={() => setToast(null)} className="text-muted hover:text-fg"><X size={15} /></button>
        </div>
      )}
    </div>
  );
}

function RepoHeader({ repo, name, count }: { repo?: RepoInfo; name: string; count: number }) {
  return (
    <div className="mb-2 flex items-center gap-2 px-1">
      <a href={repo?.url ?? `https://github.com/${name}`} target="_blank" rel="noreferrer" className="text-sm font-semibold hover:text-accent">
        <RepoTag repo={name} short={false} />
      </a>
      <span className="text-xs text-muted tabular-nums">{count}</span>
      {repo?.defaultBranch && (
        <span className="ml-auto inline-flex items-center gap-1.5 text-xs text-muted" title={`Status von ${repo.defaultBranch}`}>
          <PipelineIcon state={repo.defaultBranchPipeline} size={14} />
          <span className="font-mono">{repo.defaultBranch}</span>
        </span>
      )}
    </div>
  );
}

function TabButton({ active, onClick, icon, label, count }: { active: boolean; onClick: () => void; icon: ReactNode; label: string; count: number }) {
  return (
    <button
      onClick={onClick}
      className={cx("inline-flex h-8 items-center gap-1.5 rounded-md px-3 text-sm font-medium transition", active ? "bg-panel-2 text-fg shadow-sm" : "text-muted hover:text-fg")}
    >
      {icon}
      {label}
      <span className="text-xs tabular-nums text-muted">{count}</span>
    </button>
  );
}

function IconButton({ children, title, onClick, active }: { children: ReactNode; title: string; onClick: () => void; active?: boolean }) {
  return (
    <button
      title={title}
      onClick={onClick}
      className={cx("inline-flex size-9 items-center justify-center rounded-lg text-muted transition hover:bg-panel-2 hover:text-fg", active && "bg-panel-2 text-fg")}
    >
      {children}
    </button>
  );
}
