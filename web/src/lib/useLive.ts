import { useEffect, useState } from "react";
import type { PollStatus, Snapshot } from "../../../shared/types";
import { api, REFRESH_EVENT } from "./api";
import { useGithubToken } from "./token";

export type Connection = "connecting" | "live" | "offline";

const FALLBACK: PollStatus = {
  state: "idle",
  lastSuccess: null,
  nextPollAt: null,
  intervalSeconds: 30,
  error: null,
  rateLimit: null,
  tokenSource: "client",
  serverToken: true,
};

/**
 * Hält Snapshot + Poll-Status aktuell:
 * ohne Browser-Token per Server-Sent-Events, mit Token per eigenem Polling
 * (EventSource kann keine Header senden, der Token gehört nicht in die URL).
 */
export function useLive() {
  const [token] = useGithubToken();
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [status, setStatus] = useState<PollStatus | null>(null);
  const [connection, setConnection] = useState<Connection>("connecting");

  useEffect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    setConnection("connecting");

    if (!token) {
      let es: EventSource | null = null;
      const connect = () => {
        es = new EventSource("/api/events");
        es.onopen = () => setConnection("live");
        es.addEventListener("snapshot", (e) => setSnapshot(JSON.parse((e as MessageEvent).data)));
        es.addEventListener("status", (e) => setStatus(JSON.parse((e as MessageEvent).data)));
        es.onerror = () => {
          if (stopped) return;
          setConnection("offline");
          es?.close();
          timer = setTimeout(connect, 3000);
        };
      };
      connect();
      return () => {
        stopped = true;
        clearTimeout(timer);
        es?.close();
      };
    }

    let busy = false;
    const schedule = (seconds: number) => {
      clearTimeout(timer);
      timer = setTimeout(() => void tick(), Math.max(5, seconds) * 1000);
    };
    const tick = async () => {
      if (stopped || busy) return;
      busy = true;
      try {
        const res = await api.snapshot();
        if (stopped) return;
        if (res.snapshot) setSnapshot(res.snapshot);
        setStatus(res.status);
        setConnection("live");
        schedule(res.status.intervalSeconds);
      } catch (e) {
        if (stopped) return;
        setConnection("offline");
        setStatus((s) => ({ ...(s ?? FALLBACK), state: "error", error: (e as Error).message, tokenSource: "client" }));
        schedule(30);
      } finally {
        busy = false;
      }
    };
    const onRefresh = () => void tick();
    window.addEventListener(REFRESH_EVENT, onRefresh);
    void tick();
    return () => {
      stopped = true;
      clearTimeout(timer);
      window.removeEventListener(REFRESH_EVENT, onRefresh);
    };
  }, [token]);

  return { snapshot, status, connection };
}

/** Re-rendert periodisch, damit relative Zeiten frisch bleiben. */
export function useTick(ms = 30_000) {
  const [, set] = useState(0);
  useEffect(() => {
    const t = setInterval(() => set((n) => n + 1), ms);
    return () => clearInterval(t);
  }, [ms]);
}
