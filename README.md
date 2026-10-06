# PR Radar

Native macOS-App für Pull Requests über mehrere GitHub-Repos – modern, übersichtlich, ohne Tab-Chaos.
Geschrieben in Rust mit [GPUI Kit](https://gpui-kit.com); spricht GitHub direkt an, ohne Server.

- **Offene PRs** gebündelt (gruppiert nach Repo oder flach), mit Review-Status, Konflikten, Labels, Branch, Diff-Größe – als Liste oder Flow
- **Pipelines** pro PR: läuft (gelb, dreht) · rot · grün – aufklappbar mit allen Checks und direkten Links
- **Auto-Merge** mit einem Klick an/aus bzw. direkt mergen – angeboten werden nur Methoden, die Repo-Einstellungen **und** Rulesets des Ziel-Branches erlauben
- **Alle mergen** für alle sichtbaren, sofort mergebaren PRs (je Repo und Ziel-Branch nacheinander)
- Lehnt GitHub einen Merge vorübergehend ab (z. B. „Base branch was modified“ direkt nach einem anderen Merge), versucht die App es bis zu 3 min erneut bzw. aktiviert Auto-Merge – immer nur auf dem Commit, der beim Klick zu sehen war
- **Zuletzt gemergt** als Timeline nach Tagen
- **Releases** aller Repos inkl. Beschreibung
- **Live**: pollt GitHub per GraphQL (alle Repos in wenigen Requests); läuft irgendwo eine Pipeline, alle 10 s statt 30 s
- Seitenleiste mit Ansichten, Schnellfiltern (läuft / rot / grün / Auto-Merge / Konflikt / „Dein Review“ / „Von dir“) und Repos, Repositories-Sheet (`⌘,`)
- Systembenachrichtigungen, wenn eine Pipeline rot/grün wird oder ein PR gemergt wurde (nur aus dem App-Bundle, nicht unter `cargo run`)
- Tastatur: `/` bzw. `⌘F` Suche, `r` neu laden, `v` Liste/Flow, `g` gruppieren, `1–3` bzw. `⌘1–3` Ansichten, `⌘B` Seitenleiste
- Folgt Hell/Dunkel des Systems; Animationen entfallen bei „Bewegung reduzieren“

## Start

```bash
cd native
cargo run                       # Entwicklung
scripts/bundle.sh --install     # „PR Radar.app“ bauen und nach /Applications kopieren
```

Konfiguration: `~/Library/Application Support/pr-radar/config.json`. Repos im Repositories-Sheet hinzufügen
(`owner/repo`, GitHub-URL oder SSH-URL) oder beim ersten Start per `REPOS=owner/a,owner/b`.

## Token

Reihenfolge: `GITHUB_TOKEN` / `GH_TOKEN` → GitHub CLI (`gh auth token`, funktioniert auch beim Start aus dem Finder).

Für einen Fine-grained PAT auf die gewünschten Repos:
Metadata (read), Contents (read), Pull requests (**write** für Merge und Auto-Merge), Commit statuses (read), Actions/Checks (read).
Für Org-Repos muss die Org fine-grained Tokens zulassen – sonst klassischen PAT mit `repo` oder die `gh`-CLI nutzen.

Auto-Merge muss im Repo erlaubt sein (Settings → General → *Allow auto-merge*) und greift nur bei Branch-Protection/Rulesets mit Required Checks oder Reviews.

## Aufbau

| Pfad | Inhalt |
|---|---|
| `native/src/github/` | GraphQL-Queries, HTTP-Client, Normalisierung (getestet) |
| `native/src/radar.rs` | Poll-Zyklus, Snapshot, Merge/Auto-Merge inkl. Wiederholung |
| `native/src/ui/` | Fenster, Seitenleiste, Liste, Flow, Einstellungen |

## Ideen / nächste Schritte

- GitHub-Webhooks statt Polling (sofortige Updates, weniger API-Last)
- Ganze Orgs/Topics abonnieren (`owner/*`)
- Re-Run fehlgeschlagener Checks
- Menüleisten-Symbol mit Badge für rote PRs

## Lizenz

[Apache-2.0](LICENSE)
