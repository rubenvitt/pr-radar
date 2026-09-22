# PR Radar

Live-Dashboard für Pull Requests über mehrere GitHub-Repos – modern, übersichtlich, ohne Tab-Chaos.

- **Offene PRs** gebündelt (gruppiert nach Repo oder flach), mit Review-Status, Konflikten, Labels, Branch, Diff-Größe
- **Pipelines** pro PR: läuft (gelb, dreht) · rot · grün – aufklappbar mit allen Checks und direkten Links
- **Auto-Merge** mit einem Klick an/aus (Squash/Merge/Rebase, je nachdem was das Repo erlaubt)
- **Zuletzt gemergt** als Timeline nach Tagen
- **Releases** aller Repos inkl. gerenderter Beschreibung (neuestes je Repo aufgeklappt)
- **Live**: Server pollt GitHub (GraphQL, alle Repos in einem Request) und pusht Änderungen per Server-Sent Events.
  Läuft irgendwo eine Pipeline, wird schneller gepollt (10 s statt 30 s).
- Schnellfilter: läuft / rot / grün / Auto-Merge / „Dein Review“ / „Von dir“, Repo-Chips, Suche
- Optionale Desktop-Benachrichtigungen, wenn eine Pipeline rot/grün wird oder ein PR gemergt wurde
- Tab-Titel zeigt Anzahl roter PRs · Tastatur: `/` Suche, `r` neu laden, `1–3` Tabs
- Light & Dark Mode (folgt dem System)

## Start

```bash
mise install          # Node 22 + pnpm (oder selbst installieren)
pnpm install
cp .env.example .env  # optional – ohne Token wird `gh auth token` genutzt
pnpm dev              # http://localhost:5317
```

Repos im UI über ⚙︎ hinzufügen (`owner/repo`, GitHub-URL oder SSH-URL) – gespeichert in `data/config.json`.
Alternativ beim ersten Start per `REPOS=owner/a,owner/b` in `.env`.

Produktion lokal: `pnpm build && pnpm start` → http://127.0.0.1:4317

## Token

Reihenfolge: `GITHUB_TOKEN` / `GH_TOKEN` → GitHub CLI (`gh auth token`).

Für einen Fine-grained PAT auf die gewünschten Repos:
Metadata (read), Contents (read), Pull requests (**write** für Auto-Merge), Commit statuses (read), Actions/Checks (read).
Für Org-Repos (z. B. Kunden-Orgs) muss die Org fine-grained Tokens zulassen – sonst klassischen PAT mit `repo` oder die `gh`-CLI nutzen.

Auto-Merge muss im Repo erlaubt sein (Settings → General → *Allow auto-merge*) und greift nur bei Branch-Protection/Rulesets mit Required Checks oder Reviews.

## Architektur

```
GitHub GraphQL ──(Polling, adaptiv)──▶ Node/Hono-Server ──SSE──▶ React-UI
                                        │  Token bleibt serverseitig
                                        └─ POST /api/prs/:id/auto-merge → GraphQL-Mutation
```

| Pfad | Inhalt |
|---|---|
| `server/` | Hono-API, Poller, GraphQL-Queries, Normalisierung (getestet) |
| `shared/types.ts` | Gemeinsame Typen Server ↔ UI |
| `web/` | React 19 + Vite + Tailwind 4 |

API: `GET /api/snapshot`, `GET /api/events` (SSE), `POST /api/refresh`, `GET /api/config`,
`POST /api/repos`, `DELETE /api/repos/:owner/:name`, `POST /api/prs/:id/auto-merge`.

## Betrieb auf einem Server

`docker compose up -d --build`. Die App bindet standardmäßig nur an `127.0.0.1`, weil sie mit deinem Token
Auto-Merge schalten kann. Wenn sie öffentlich erreichbar wird (Traefik o. ä.), unbedingt
`BASIC_AUTH_USER`/`BASIC_AUTH_PASSWORD` setzen oder einen Auth-Proxy davorhängen.

## Ideen / nächste Schritte

- GitHub-Webhooks statt Polling (sofortige Updates, weniger API-Last)
- Ganze Orgs/Topics abonnieren (`owner/*`)
- „Merge now“-Button, Re-Run fehlgeschlagener Checks
- Menüleisten-App (Tauri) mit Badge für rote PRs
