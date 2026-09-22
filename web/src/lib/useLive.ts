import { useEffect, useRef, useState } from "react";
import type { PollStatus, Snapshot } from "../../../shared/types";

export type Connection = "connecting" | "live" | "offline";

/** Hält per Server-Sent-Events Snapshot + Poll-Status aktuell. */
export function useLive() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [status, setStatus] = useState<PollStatus | null>(null);
  const [connection, setConnection] = useState<Connection>("connecting");
  const esRef = useRef<EventSource | null>(null);

  useEffect(() => {
    let retry: ReturnType<typeof setTimeout>;
    const connect = () => {
      const es = new EventSource("/api/events");
      esRef.current = es;
      es.onopen = () => setConnection("live");
      es.addEventListener("snapshot", (e) => setSnapshot(JSON.parse((e as MessageEvent).data)));
      es.addEventListener("status", (e) => setStatus(JSON.parse((e as MessageEvent).data)));
      es.onerror = () => {
        setConnection("offline");
        es.close();
        retry = setTimeout(connect, 3000);
      };
    };
    connect();
    return () => {
      clearTimeout(retry);
      esRef.current?.close();
    };
  }, []);

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
