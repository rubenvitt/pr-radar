import type { MergeMethod, PollStatus, Snapshot } from "../../../shared/types";
import { readToken } from "./token";

/** Signalisiert der Live-Schleife, dass sich etwas geändert hat (nur im Browser-Token-Modus relevant). */
export const REFRESH_EVENT = "pr-radar:refresh";

function signalRefresh() {
  window.dispatchEvent(new Event(REFRESH_EVENT));
}

async function call<T>(method: string, url: string, body?: unknown, tokenOverride?: string): Promise<T> {
  const token = tokenOverride ?? readToken();
  const res = await fetch(url, {
    method,
    headers: {
      "Content-Type": "application/json",
      ...(token ? { "X-GitHub-Token": token } : {}),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const json = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error((json as { error?: string }).error ?? `HTTP ${res.status}`);
  return json as T;
}

export const api = {
  snapshot: () => call<{ snapshot: Snapshot | null; status: PollStatus }>("GET", "/api/snapshot"),
  checkToken: (token: string) => call<{ login: string }>("POST", "/api/token/check", {}, token),
  refresh: async () => {
    await call("POST", "/api/refresh", {});
    signalRefresh();
  },
  config: () => call<{ repos: string[] }>("GET", "/api/config"),
  addRepo: async (repo: string) => {
    const cfg = await call<{ repos: string[] }>("POST", "/api/repos", { repo });
    signalRefresh();
    return cfg;
  },
  removeRepo: async (full: string) => {
    const cfg = await call<{ repos: string[] }>("DELETE", `/api/repos/${full}`, {});
    signalRefresh();
    return cfg;
  },
  autoMerge: async (id: string, enable: boolean, method?: MergeMethod) => {
    const res = await call("POST", `/api/prs/${encodeURIComponent(id)}/auto-merge`, { enable, method });
    signalRefresh();
    return res;
  },
};
