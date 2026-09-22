import "./env.js";
import { serve } from "@hono/node-server";
import { serveStatic } from "@hono/node-server/serve-static";
import { Hono } from "hono";
import { basicAuth } from "hono/basic-auth";
import { streamSSE } from "hono/streaming";
import type { MergeMethod, PollStatus, Snapshot } from "../shared/types.js";
import { loadConfig, parseRepo, saveConfig } from "./config.js";
import { graphql } from "./github.js";
import { Poller } from "./poller.js";
import { DISABLE_AUTO_MERGE, ENABLE_AUTO_MERGE } from "./queries.js";

const poller = new Poller();
const app = new Hono();

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

app.get("/api/snapshot", (c) => c.json({ snapshot: poller.snapshot, status: poller.status }));

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

app.post("/api/refresh", async (c) => {
  await poller.refresh();
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
  const id = c.req.param("id");
  const { enable, method } = await c.req.json<{ enable: boolean; method?: MergeMethod }>();
  if (enable) {
    // Nur Methoden zulassen, die das Repo des PRs erlaubt.
    const repoName = poller.snapshot?.open.find((p) => p.id === id)?.repo;
    const allowed = poller.snapshot?.repos.find((r) => r.fullName === repoName)?.mergeMethods ?? [];
    if (!method || !allowed.includes(method)) {
      return c.json({ error: `Merge-Methode ${method ?? "–"} ist in diesem Repo nicht erlaubt` }, 422);
    }
  }
  const res = enable
    ? await graphql(ENABLE_AUTO_MERGE, { id, method })
    : await graphql(DISABLE_AUTO_MERGE, { id });
  if (res.errors.length) return c.json({ error: res.errors.map((e) => e.message).join("; ") }, 422);
  await poller.refresh();
  return c.json({ ok: true });
});

// Produktion: gebautes Frontend ausliefern
if (process.env.NODE_ENV === "production") {
  app.use("/*", serveStatic({ root: "./dist-web" }));
  app.get("*", serveStatic({ path: "./dist-web/index.html" }));
}

const port = Number(process.env.PORT ?? 4317);
const hostname = process.env.HOST ?? "127.0.0.1";
serve({ fetch: app.fetch, port, hostname }, () => {
  console.log(`PR Radar API läuft auf http://${hostname}:${port}`);
  poller.start();
});
