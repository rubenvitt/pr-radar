# PR Radar

Native macOS-App für Pull Requests über mehrere GitHub-Repos – modern, übersichtlich, ohne Tab-Chaos.
Geschrieben in Rust mit [GPUI Kit](https://gpui-kit.com); spricht GitHub direkt an, ohne Server.

Dazu gibt es eine **iOS-/iPadOS-App** mit denselben Funktionen in SwiftUI – siehe [`ios/`](ios/README.md).

- **Offene PRs** gebündelt (gruppiert nach Repo oder flach), mit Review-Status, Konflikten, Labels, Branch, Diff-Größe – als Liste oder Flow
- **Pipelines** pro PR: läuft (amber, dreht) · rot · grün – aufklappbar mit allen Checks und direkten Links
- **Auto-Merge** mit einem Klick an/aus bzw. direkt mergen – angeboten werden nur Methoden, die Repo-Einstellungen **und** Rulesets des Ziel-Branches erlauben
- **Alle mergen** für alle sichtbaren, sofort mergebaren PRs (je Repo und Ziel-Branch nacheinander)
- Lehnt GitHub einen Merge vorübergehend ab (z. B. „Base branch was modified“ direkt nach einem anderen Merge), versucht die App es bis zu 3 min erneut bzw. aktiviert Auto-Merge – immer nur auf dem Commit, der beim Klick zu sehen war
- **Zuletzt gemergt** als Timeline nach Tagen
- **Releases** aller Repos inkl. Beschreibung
- **Live**: pollt GitHub per GraphQL (alle Repos in wenigen Requests); läuft irgendwo eine Pipeline, alle 10 s statt 30 s
- Seitenleiste mit Ansichten, Schnellfiltern (läuft / rot / grün / Auto-Merge / Konflikt / „Dein Review“ / „Von dir“) und Repos, Repositories-Sheet (`⌘,`)
- **Menüleisten-Symbol** mit der Zahl roter Pipelines (dazu Badge am Dock-Symbol); das Menü listet die neuesten roten PRs (Klick öffnet sie auf GitHub), zeigt alle roten PRs im Fenster und öffnet PR Radar auch nach `⌘W` wieder
- **Beim Anmelden starten** im Menü „PR Radar“ bzw. im Menüleisten-Menü (nur aus dem App-Bundle; verlangt macOS eine Freigabe, öffnen sich die Systemeinstellungen → Anmeldeobjekte). Nach der Anmeldung startet PR Radar ohne Fenster, nur mit dem Menüleisten-Symbol; Menüleiste oder Dock-Symbol öffnen das Fenster
- Systembenachrichtigungen, wenn eine Pipeline rot/grün wird oder ein PR gemergt wurde, auch bei geschlossenem Fenster; Klick öffnet den PR (nur aus dem App-Bundle, nicht unter `cargo run`)
- Tastatur: `/` bzw. `⌘F` Suche, `r` neu laden, `v` Liste/Flow, `g` gruppieren, `1–3` bzw. `⌘1–3` Ansichten, `⌘B` Seitenleiste
- macOS-Standards: `⌘W` schließt Sheet bzw. Fenster (die App läuft weiter, Dock-Klick öffnet es wieder), `⌘M` Dock, `⌘H`/`⌥⌘H` ausblenden, `⌃⌘F` Vollbild; Menüs Ablage, Bearbeiten (Widerrufen, Ausschneiden, Kopieren, Einsetzen, Alles auswählen), Fenster und Hilfe
- Eigenes Farbschema (Akzent Blau, „gemergt“ Lila, Statusfarben mit mind. 4,5:1, gestaffelte Flächen im Dunkelmodus), folgt Hell/Dunkel des Systems; Animationen entfallen bei „Bewegung reduzieren“
- Englisch (Standard) und Deutsch, nach der Systemsprache bzw. „Sprache pro App“; `PR_RADAR_LANG=de|en` erzwingt eine Sprache (z. B. bei `cargo run`)

## Start

```bash
cd native
cargo run                       # Entwicklung
scripts/bundle.sh --install     # „PR Radar.app“ bauen und nach /Applications kopieren
```

Releases (signiert und beglaubigt als GitHub-Release, iOS über TestFlight) entstehen per Tag, siehe [docs/RELEASING.md](docs/RELEASING.md).

### Demo-Modus

```bash
cargo run -- --demo                 # Beispieldaten statt GitHub, ohne Token
cargo run -- --demo --dark --tab releases
scripts/screenshots.sh              # alle Ansichten hell/dunkel nach native/screenshots/
```

Auch per `PR_RADAR_DEMO=1`. Die App zeigt dann einen festen Stand (7 offene PRs in 4 Repos mit Konflikt, Auto-Merge,
Draft, Review-Anfrage und roter Pipeline, ein nicht erreichbares Repo, 7 gemergte PRs, 2 Releases), fragt GitHub
nicht ab, liest und schreibt die `config.json` nicht und zeigt bei Merge und Auto-Merge nur einen Hinweis.
Weitere Schalter nur im Demo-Modus: `--dark`/`--light`, `--tab open|merged|releases`, `--view list|flow`,
`--quick failed|running|…` (oder `PR_RADAR_APPEARANCE`, `PR_RADAR_TAB`, `PR_RADAR_VIEW`, `PR_RADAR_QUICK`).
`scripts/screenshots.sh` läuft auf macOS (`screencapture`) und unter Linux mit Xvfb, `xdotool`, ImageMagick und
Software-Vulkan (`libxkbcommon-dev`, `libxkbcommon-x11-dev`, `mesa-vulkan-drivers`).

Konfiguration: `~/Library/Application Support/pr-radar/config.json`. Repos im Repositories-Sheet hinzufügen
(`owner/repo`, GitHub-URL oder SSH-URL) oder beim ersten Start per `REPOS=owner/a,owner/b`.

Protokoll: `~/Library/Logs/PR Radar/pr-radar.log` (Fehler, Warnungen und Abstürze, keine Tokens; Menü Hilfe → „Protokoll im Finder zeigen“).

Website, Support, Datenschutz, EULA und Impressum: <https://pr-radar.rubeen.dev> (auch im Menü Hilfe).

## Anmeldung und Token

**Mit GitHub anmelden** (Repositories-Sheet, `⌘,`): Die App zeigt einen Code und öffnet github.com/login/device,
nach der Bestätigung gilt der Token sofort (OAuth Device Flow, Scope `repo`). Org-Repos erscheinen erst, wenn die
Organisation PR Radar freigibt (Settings → Third-party access). Die Anmeldung bleibt im Schlüsselbund, bis man
sich im Repositories-Sheet abmeldet oder GitHub den Token widerruft (dann fragt die App erneut).

Die Client-ID der OAuth App steht in `native/src/github/oauth.rs` (`CLIENT_ID`; in der OAuth App „Enable Device Flow“
anschalten). Zum Ausprobieren ohne neuen Build: `PR_RADAR_GITHUB_CLIENT_ID=… cargo run`.

Erweitert, ohne Anmeldung: `GITHUB_TOKEN` / `GH_TOKEN` → GitHub CLI (`gh auth token`, funktioniert auch beim Start aus dem Finder).

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
| `ios/` | iOS-/iPadOS-App (SwiftUI) mit plattformunabhängigem Kern `PRRadarCore` |
| `site/` | Produktseite mit Support, Datenschutz, EULA und Impressum (Cloudflare Pages, siehe [`site/README.md`](site/README.md)) |

## Ideen / nächste Schritte

- GitHub-Webhooks statt Polling (sofortige Updates, weniger API-Last)
- Ganze Orgs/Topics abonnieren (`owner/*`)
- Re-Run fehlgeschlagener Checks

## Lizenz

[Functional Source License 1.1, ALv2 Future License](LICENSE.md) (FSL-1.1-ALv2).

Der Quellcode ist offen einsehbar. Nutzen, ändern und weitergeben ist erlaubt, nur kein konkurrierendes kommerzielles Produkt daraus machen. Zwei Jahre nach ihrer Veröffentlichung steht jede Version zusätzlich unter Apache-2.0.

Stände bis einschließlich Commit `469e369` (2. November 2026) wurden unter Apache-2.0 veröffentlicht und bleiben das.

### Drittanbieter

- **Rust-Abhängigkeiten:** nur freizügige Lizenzen (MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0 u. a.). `cargo deny check` in `native/` prüft Lizenzen, Sicherheitsmeldungen und Quellen (`deny.toml`, läuft in der CI). `scripts/bundle.sh` erzeugt mit [`cargo about`](https://github.com/EmbarkStudios/cargo-about) die Datei `ThirdPartyNotices.html` im App-Bundle (Menü Hilfe → „Drittanbieter-Lizenzen“).
- **Lucide-Icons** (über GPUI Kit, eingebettet als Rückfall für Komponenten-Icons ohne eigene Datei): ISC, teils MIT (Feather). Die Hinweise stehen in `ThirdPartyNotices.html`.
- **App-Icons** in `native/assets/icons/`: [Icons8](https://icons8.com) „Material Rounded“, kommerziell lizenziert über ein Icons8-Abo, daher ohne Namensnennung. Sie fallen **nicht** unter die Lizenz dieses Repos und dürfen nicht losgelöst von PR Radar weiterverwendet werden.
- **iOS-App:** keine Fremdbibliotheken, Symbole aus Apples SF Symbols.
