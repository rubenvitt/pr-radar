import { useEffect, useState } from "react";

const KEY = "pr-radar:github-token";
const CHANGED = "pr-radar:token-changed";

/**
 * Persönlicher GitHub-Token – bleibt im Browser (localStorage) und wird nur als Header
 * `X-GitHub-Token` mitgeschickt. Der Server speichert ihn nicht.
 */
export function readToken(): string | null {
  try {
    return localStorage.getItem(KEY) || null;
  } catch {
    return null;
  }
}

export function writeToken(token: string | null): void {
  try {
    if (token) localStorage.setItem(KEY, token);
    else localStorage.removeItem(KEY);
  } catch {
    /* z. B. privater Modus – dann gilt der Token nur für diese Sitzung */
  }
  window.dispatchEvent(new Event(CHANGED));
}

/** Nur die letzten Zeichen zeigen – genug zum Wiedererkennen, ohne den Token anzuzeigen. */
export function maskToken(token: string): string {
  return `…${token.slice(-4)}`;
}

export function useGithubToken() {
  const [token, setToken] = useState<string | null>(readToken);
  useEffect(() => {
    const sync = () => setToken(readToken());
    window.addEventListener(CHANGED, sync);
    window.addEventListener("storage", sync); // andere Tabs
    return () => {
      window.removeEventListener(CHANGED, sync);
      window.removeEventListener("storage", sync);
    };
  }, []);
  return [token, writeToken] as const;
}
