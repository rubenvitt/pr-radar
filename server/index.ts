import "./env.js";
import { serve } from "@hono/node-server";
import { serveStatic } from "@hono/node-server/serve-static";
import { Hono, type Context } from "hono";
import { basicAuth } from "hono/basic-auth";
import { streamSSE } from "hono/streaming";
import type { MergeMethod, PollStatus, Snapshot } from "../shared/types.js";
import { loadConfig, parseRepo, saveConfig } from "./config.js";
import { findServerToken, graphql, GraphQLError, parseClientToken } from "./github.js";
import { Poller, POLL_ACTIVE, POLL_NORMAL } from "./poller.js";
import { fetchSnapshot } from "./snapshot.js";
import { DISABLE_AUTO_MERGE, ENABLE_AUTO_MERGE, VIEWER } from "./queries.js";

const poller = new Poller();
const app = new Hono();

const BAD_TOKEN = "Der mitgeschickte Token hat ein ungültiges Format.";

/**
 * Token aus dem Browser (Header `X-GitHub-Token`). Er gilt nur für diesen Request,
 * wird nirgends gespeichert und nie auf die Platte geschrieben.
 */
function clientToken(c: Context): string | null | false {
  const raw = c.req.header("x-github-token");
  if (!raw?.trim()) return null;
  return parseClientToken(raw) ?? false;
}

function errorResponse(c: Context, e: unknown) {
  const status = e instanceof GraphQLError ? e.status : 502;
  return c.json({ error: (e as Error).message }, status as 401 | 502);
}

// Optionaler Schutz, sobald die App nicht nur lokal läuft.
if (process.env.BASIC_AUTH_USER && process.env.BASIC_AUTH_PASSWORD) {
  app.use("*", basicAuth({ username: process.env.BASIC_AUTH_USER, password: process.env.BASIC_AUTH_PASSWORD }));
}

// Schreibende Requests nur als JSON → Browser erzwingt Preflight, fremde Seiten können nichts auslösen.
app.use("/api/*", async (c, next) => {
  if (c.req.method !== "GET" && !c.req.header("content-type")?.includes("application/json")) {
    return c.json({ error: "Content-Type application/json erforderlich" }, 415);
  }
  await next();
});

app.get("/api/snapshot", async (c) => {
  const token = clientToken(c);
  if (token === false) return c.json({ error: BAD_TOKEN }, 400);
  if (!token) return c.json({ snapshot: poller.snapshot, status: poller.status });

  // Browser-Token: eigener Abruf, unabhängig vom gemeinsamen Poller.
  try {
    const { repos } = await loadConfig();
    const { snapshot, rateLimit } = await fetchSnapshot(repos, token);
    const interval = snapshot.open.some((p) => p.pipeline.state === "running") ? POLL_ACTIVE : POLL_NORMAL;
    const status: PollStatus = {
      state: "idle",
      lastSuccess: snapshot.fetchedAt,
      nextPollAt: new Date(Date.now() + interval * 1000).toISOString(),
      intervalSeconds: interval,
      error: null,
      rateLimit,
      tokenSource: "client",
      serverToken: poller.status.serverToken,
    };
    return c.json({ snapshot, status });
  } catch (e) {
    return errorResponse(c, e);
  }
});

/** Prüft einen Token, bevor der Browser ihn dauerhaft speichert. */
app.post("/api/token/check", async (c) => {
  const token = clientToken(c);
  if (token === false) return c.json({ error: BAD_TOKEN }, 400);
  if (!token) return c.json({ error: "Kein Token mitgeschickt." }, 400);
  try {
    const res = await graphql<{ viewer: { login: string } }>(VIEWER, {}, token);
    const login = res.data?.viewer?.login;
    if (!login) return c.json({ error: res.errors.map((e) => e.message).join("; ") || "Token abgelehnt" }, 401);
    return c.json({ login });
  } catch (e) {
    return errorResponse(c, e);
  }
});

app.get("/api/events", (c) =>
  streamSSE(c, async (stream) => {
    const send = (type: string, data: unknown) => stream.writeSSE({ event: type, data: JSON.stringify(data) });
    const onSnapshot = (s: Snapshot) => void send("snapshot", s);
    const onStatus = (s: PollStatus) => void send("status", s);
    poller.on("snapshot", onSnapshot);
    poller.on("status", onStatus);
    if (poller.snapshot) await send("snapshot", poller.snapshot);
    await send("status", poller.status);

    let open = true;
    stream.onAbort(() => {
      open = false;
    });
    while (open) {
      await stream.sleep(25_000);
      if (open) await stream.writeSSE({ event: "ping", data: "" });
    }
    poller.off("snapshot", onSnapshot);
    poller.off("status", onStatus);
  }),
);

// Nicht auf den Poll warten – sonst läuft der Request bei vielen Repos in den Timeout eines Proxys (504).
app.post("/api/refresh", (c) => {
  if (!clientToken(c)) void poller.refresh();
  return c.json({ ok: true });
});

app.get("/api/config", async (c) => c.json(await loadConfig()));

app.post("/api/repos", async (c) => {
  const body = await c.req.json<{ repo?: string }>();
  const repo = parseRepo(body.repo ?? "");
  if (!repo) return c.json({ error: "Format: owner/name oder GitHub-URL" }, 400);
  const cfg = await loadConfig();
  if (!cfg.repos.some((r) => r.toLowerCase() === repo.toLowerCase())) {
    await saveConfig({ ...cfg, repos: [...cfg.repos, repo] });
  }
  void poller.refresh();
  return c.json(await loadConfig());
});

app.delete("/api/repos/:owner/:name", async (c) => {
  const full = `${c.req.param("owner")}/${c.req.param("name")}`.toLowerCase();
  const cfg = await loadConfig();
  await saveConfig({ ...cfg, repos: cfg.repos.filter((r) => r.toLowerCase() !== full) });
  void poller.refresh();
  return c.json(await loadConfig());
});

app.post("/api/prs/:id/auto-merge", async (c) => {
  const token = clientToken(c);
  if (token === false) return c.json({ error: BAD_TOKEN }, 400);
  const id = c.req.param("id");
  const { enable, method } = await c.req.json<{ enable: boolean; method?: MergeMethod }>();
  try {
    const res = enable
      ? await graphql(ENABLE_AUTO_MERGE, { id, method: method ?? "SQUASH" }, token ?? undefined)
      : await graphql(DISABLE_AUTO_MERGE, { id }, token ?? undefined);
    if (res.errors.length) return c.json({ error: res.errors.map((e) => e.message).join("; ") }, 422);
  } catch (e) {
    return errorResponse(c, e);
  }
  // Mit Browser-Token holt sich der Client seinen Stand selbst.
  if (!token) void poller.refresh();
  return c.json({ ok: true });
});

// Produktion: gebautes Frontend ausliefern
if (process.env.NODE_ENV === "production") {
  app.use("/*", serveStatic({ root: "./dist-web" }));
  app.get("*", serveStatic({ path: "./dist-web/index.html" }));
}

const port = Number(process.env.PORT ?? 4317);
const hostname = process.env.HOST ?? "127.0.0.1";
serve({ fetch: app.fetch, port, hostname }, async () => {
  console.log(`PR Radar API läuft auf http://${hostname}:${port}`);
  if (!(await findServerToken())) {
    console.log("Kein Server-Token – Clients können unter ⚙︎ einen eigenen Token im Browser hinterlegen.");
  }
  poller.start();
});
