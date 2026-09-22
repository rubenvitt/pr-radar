import { useEffect, useState } from "react";
import { Check, KeyRound, Plus, Trash2, X } from "lucide-react";
import type { PollStatus, RepoInfo } from "../../../shared/types";
import { api } from "../lib/api";
import { ago } from "../lib/time";
import { maskToken, useGithubToken } from "../lib/token";
import { PipelineIcon, RepoTag } from "./ui";

export function RepoSettings({ repos, status, onClose }: { repos: RepoInfo[]; status: PollStatus | null; onClose: () => void }) {
  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [configured, setConfigured] = useState<string[] | null>(null);

  // Konfigurierte Liste (enthält auch Repos, die noch nicht geladen sind)
  useEffect(() => {
    void api.config().then((c) => setConfigured(c.repos));
  }, []);
  const list = configured ?? repos.map((r) => r.fullName);
  const info = new Map(repos.map((r) => [r.fullName.toLowerCase(), r]));

  const add = async () => {
    setError(null);
    try {
      const cfg = await api.addRepo(input);
      setConfigured(cfg.repos);
      setInput("");
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <div className="fixed inset-0 z-40 flex justify-end bg-black/30 backdrop-blur-[2px]" onClick={onClose}>
      <aside className="flex h-full w-full max-w-md flex-col border-l border-line bg-panel shadow-2xl" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between border-b border-line px-5 py-4">
          <h2 className="font-semibold">Repositories</h2>
          <button onClick={onClose} className="rounded-md p-1.5 text-muted hover:bg-panel-2 hover:text-fg"><X size={18} /></button>
        </div>

        <form className="flex gap-2 px-5 pt-4" onSubmit={(e) => { e.preventDefault(); void add(); }}>
          <input
            autoFocus
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="owner/repo oder GitHub-URL"
            className="h-9 flex-1 rounded-lg border border-line bg-bg px-3 text-sm outline-none focus:border-accent"
          />
          <button disabled={!input.trim()} className="inline-flex h-9 items-center gap-1 rounded-lg bg-accent px-3 text-sm font-medium text-white disabled:opacity-40">
            <Plus size={15} />Hinzufügen
          </button>
        </form>
        {error && <p className="px-5 pt-2 text-sm text-fail">{error}</p>}

        <ul className="mt-4 flex-1 overflow-y-auto px-3">
          {list.map((full) => {
            const r = info.get(full.toLowerCase());
            return (
              <li key={full} className="group flex items-center gap-3 rounded-lg px-2 py-2 hover:bg-panel-2">
                <PipelineIcon state={r?.defaultBranchPipeline ?? "none"} size={16} />
                <div className="min-w-0 flex-1">
                  <RepoTag repo={full} short={false} />
                  <div className="text-[11px] text-muted">
                    {r?.error ? <span className="text-fail">{r.error}</span> : r ? `${r.defaultBranch ?? "–"} · Auto-Merge ${r.autoMergeAllowed ? "erlaubt" : "aus"}` : "wird geladen …"}
                  </div>
                </div>
                <button
                  title="Entfernen"
                  onClick={async () => setConfigured((await api.removeRepo(full)).repos)}
                  className="rounded-md p-1.5 text-muted opacity-0 hover:text-fail group-hover:opacity-100"
                >
                  <Trash2 size={15} />
                </button>
              </li>
            );
          })}
          {list.length === 0 && <li className="px-2 py-6 text-center text-sm text-muted">Noch keine Repos – füge oben eins hinzu.</li>}
        </ul>

        <TokenPanel status={status} />

        <div className="space-y-1 border-t border-line px-5 py-4 text-xs text-muted">
          <div>Aktualisierung alle {status?.intervalSeconds ?? "–"} s · zuletzt {ago(status?.lastSuccess)}</div>
          {status?.rateLimit && (
            <div>API-Kontingent: {status.rateLimit.remaining.toLocaleString("de-DE")} / {status.rateLimit.limit.toLocaleString("de-DE")}</div>
          )}
        </div>
      </aside>
    </div>
  );
}

/** GitHub-Token für diesen Browser – nützlich bei einer gehosteten Instanz ohne eigenen Token. */
function TokenPanel({ status }: { status: PollStatus | null }) {
  const [token, setToken] = useGithubToken();
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const [open, setOpen] = useState(false);

  const serverToken =
    status?.tokenSource === "env" ? "GITHUB_TOKEN (.env)" : status?.tokenSource === "gh-cli" ? "GitHub CLI (gh auth token)" : null;

  const save = async () => {
    const value = input.trim();
    if (!value) return;
    setBusy(true);
    setMessage(null);
    try {
      const { login } = await api.checkToken(value);
      setToken(value);
      setInput("");
      setOpen(false);
      setMessage({ ok: true, text: `Gespeichert – angemeldet als ${login}.` });
    } catch (e) {
      setMessage({ ok: false, text: (e as Error).message });
    } finally {
      setBusy(false);
    }
  };

  const remove = () => {
    setToken(null);
    setMessage(null);
    setOpen(false);
  };

  return (
    <div className="border-t border-line px-5 py-4 text-xs">
      <div className="flex items-center gap-2">
        <KeyRound size={14} className={token ? "text-accent" : "text-muted"} />
        <span className="font-medium text-fg">Token</span>
        <span className="min-w-0 flex-1 truncate text-muted">
          {token
            ? `im Browser (${maskToken(token)})`
            : serverToken
              ? `vom Server: ${serverToken}`
              : "keiner – der Server hat auch keinen"}
        </span>
        {token ? (
          <button onClick={remove} className="rounded-md px-2 py-1 text-muted hover:text-fail">Entfernen</button>
        ) : null}
        <button onClick={() => setOpen((o) => !o)} className="rounded-md px-2 py-1 text-muted hover:text-fg">
          {open ? "Abbrechen" : token ? "Ersetzen" : "Hinterlegen"}
        </button>
      </div>

      {open && (
        <form className="mt-3 space-y-2" onSubmit={(e) => { e.preventDefault(); void save(); }}>
          <input
            autoFocus
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="ghp_… oder github_pat_…"
            className="h-9 w-full rounded-lg border border-line bg-bg px-3 font-mono text-sm outline-none focus:border-accent"
          />
          <p className="text-muted">
            Bleibt in diesem Browser (localStorage) und wird pro Anfrage mitgeschickt – der Server speichert ihn nicht.
            Rechte: Pull requests (write für Auto-Merge), Contents, Commit statuses, Actions, Metadata (read).
          </p>
          <button
            disabled={!input.trim() || busy}
            className="inline-flex h-8 items-center gap-1 rounded-lg bg-accent px-3 text-sm font-medium text-white disabled:opacity-40"
          >
            <Check size={14} />{busy ? "Prüfe …" : "Prüfen & speichern"}
          </button>
        </form>
      )}

      {message && <p className={message.ok ? "mt-2 text-pass" : "mt-2 text-fail"}>{message.text}</p>}
    </div>
  );
}
