import type { MergeMethod } from "../../../shared/types";

async function call<T>(method: string, url: string, body?: unknown): Promise<T> {
  const res = await fetch(url, {
    method,
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const json = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error((json as { error?: string }).error ?? `HTTP ${res.status}`);
  return json as T;
}

export const api = {
  refresh: () => call("POST", "/api/refresh", {}),
  config: () => call<{ repos: string[] }>("GET", "/api/config"),
  addRepo: (repo: string) => call<{ repos: string[] }>("POST", "/api/repos", { repo }),
  removeRepo: (full: string) => call<{ repos: string[] }>("DELETE", `/api/repos/${full}`, {}),
  autoMerge: (id: string, enable: boolean, method?: MergeMethod) =>
    call("POST", `/api/prs/${encodeURIComponent(id)}/auto-merge`, { enable, method }),
};
