# Produktseite

Statische Seite für PR Radar: Startseite, Support und Datenschutz, jeweils Englisch
(Standard) und Deutsch. Kein Framework, kein JavaScript, keine fremden Schriften oder
Tracker – die Datenschutzerklärung verspricht das, `check.py` prüft es.

PR Radar wird nicht verkauft (Entscheidung vom 12.11.2026, PRR-70): Support läuft über
GitHub Issues, es gibt kein Impressum mit Anschrift und keine EULA.

| Pfad | Seite |
|---|---|
| `/` · `/de/` | Startseite |
| `/support/` · `/de/support/` | Support und FAQ |
| `/privacy/` · `/de/datenschutz/` | Datenschutzerklärung |

Kurzpfade wie `/datenschutz` leitet `public/_redirects` weiter, ebenso die früheren
Pfade von Impressum und EULA (auf die Startseite, weil ältere App-Versionen sie verlinken).
Sicherheits-Header (u. a. eine CSP ohne Skripte) stehen in `public/_headers`.

## Build

`build.sh` prüft `public/` mit `check.py` (Links, Anker, keine fremden Ressourcen, keine
Platzhalter und keine `mailto:`-Links) und kopiert es nach `dist/` (nicht versioniert).
Veröffentlicht wird `dist/`. Build-Variablen braucht es keine.

## Lokal ansehen und prüfen

```bash
sh site/build.sh                     # prüft public/, erzeugt dist/
python3 -m http.server -d site/dist 8000
```

## Veröffentlichen (Cloudflare)

Das Cloudflare-Projekt `pr-radar` ist ein **Worker mit statischen Assets** (Workers Builds),
kein Pages-Projekt. Was ausgeliefert wird, steht in `wrangler.jsonc` im Repo-Wurzelverzeichnis
(`assets.directory = ./site/dist`, unbekannte Pfade bekommen `404.html`). `_headers` und
`_redirects` wendet Cloudflare als Regeln an.

Einstellungen unter Workers & Pages → `pr-radar` → Settings → Build:

- Build command: `sh site/build.sh`
- Deploy command: `npx wrangler deploy` (Standard)
- Non-production branch deploy command: `npx wrangler preview` (braucht den `previews`-Block in `wrangler.jsonc`)
- Root directory: leer (Repo-Wurzel)
- Production branch: `main`
- Eigene Domain unter Settings → Domains & Routes: `pr-radar.rubeen.dev`
  (die Apps verlinken diese Adresse, siehe unten)

Die früheren Build-Variablen mit der Anschrift fürs Impressum werden nicht mehr gebraucht
und können unter Settings → Build → Variables and secrets gelöscht werden.

Probelauf lokal, ohne etwas zu veröffentlichen:

```bash
npx wrangler deploy --dry-run
```

## Vor dem Livegang

1. Im Repo unter Settings → General → Features **Issues** eingeschaltet lassen; die Vorlagen
   liegen in `.github/ISSUE_TEMPLATE/`.
2. Die Texte sind sorgfältig erstellt, aber keine Rechtsberatung.

## In den Apps

Die Adressen stehen an je einer Stelle und werden von Tests gegen `site/public` geprüft:

- macOS: `native/src/site.rs` (Hilfemenü: Website, Support, Datenschutz; „Problem melden …“
  öffnet die Issue-Vorlagen auf GitHub)
- iOS/iPadOS: `ios/Packages/PRRadarCore/Sources/PRRadarCore/Site.swift` (Einstellungen → Info,
  dort auch „Problem melden“)

Ändert sich die Domain, beide Dateien anpassen.

## App Store Connect

| Feld | Wert |
|---|---|
| Privacy Policy URL | `https://pr-radar.rubeen.dev/privacy/` (Deutsch: `/de/datenschutz/`) |
| Support URL | `https://pr-radar.rubeen.dev/support/` (Deutsch: `/de/support/`) |
| Marketing URL | `https://pr-radar.rubeen.dev/` |
| Lizenzvereinbarung | Apples Standard-EULA belassen |
| Preis | kostenlos |
| App-Datenschutz („Nutritional Label“) | „Keine Daten erfasst“ – passt zu `ios/PRRadar/PrivacyInfo.xcprivacy` |
