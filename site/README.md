# Produktseite

Statische Seite für PR Radar: Startseite, Support, Datenschutz, EULA und Impressum,
jeweils Englisch (Standard) und Deutsch. Kein Framework, kein JavaScript, keine fremden
Schriften oder Tracker – die Datenschutzerklärung verspricht das, `check.py` prüft es.

| Pfad | Seite |
|---|---|
| `/` · `/de/` | Startseite |
| `/support/` · `/de/support/` | Support und FAQ |
| `/privacy/` · `/de/datenschutz/` | Datenschutzerklärung |
| `/terms/` · `/de/eula/` | Endnutzer-Lizenzvereinbarung (die deutsche Fassung ist maßgeblich) |
| `/imprint/` | Impressum (zweisprachig) |

Kurzpfade wie `/impressum` oder `/datenschutz` leitet `public/_redirects` weiter.
Sicherheits-Header (u. a. eine CSP ohne Skripte) stehen in `public/_headers`.

## Lokal ansehen und prüfen

```bash
python3 -m http.server -d site/public 8000
sh site/build.sh            # Links, Anker, fremde Ressourcen, offene Platzhalter
sh site/build.sh --strict   # wie Produktion: Platzhalter sind ein Fehler
```

## Veröffentlichen (Cloudflare Pages)

Das Pages-Projekt `r-pr-radar` ist schon mit dem Repo verbunden (siehe PRR-42). Einstellungen:

- Build command: `sh site/build.sh`
- Build output directory: `site/public`
- Production branch: `main`
- Eigene Domain: `pr-radar.rubeen.dev` (die Apps verlinken diese Adresse, siehe unten)

Auf `main` bricht der Build ab, solange noch `TODO-RUBEN`-Platzhalter in den Seiten stehen.
Vorschau-Builds für PRs laufen trotzdem durch.

## Vor dem Livegang

1. **Anschrift** im Impressum und in beiden Datenschutzerklärungen eintragen
   (`grep -rn TODO-RUBEN site/public`). Ladungsfähige Anschrift, kein Postfach.
   Wer die Wohnanschrift nicht veröffentlichen will, nimmt eine Geschäfts- oder Impressumsadresse.
2. Optional einen zweiten Kontaktweg (Telefon) im Impressum ergänzen.
   Eine Wirtschafts-Identifikationsnummer nur eintragen, wenn sie zugeteilt ist.
3. Postfach bzw. Alias **pr-radar@rubeen.dev** anlegen.
4. Sobald der Merchant of Record feststeht (PRR-7): ihn in Datenschutz und EULA namentlich nennen
   und die Lizenzprüfung genau beschreiben (PRR-20). Die Stellen sind als HTML-Kommentar markiert.
5. Die Texte sind sorgfältig erstellt, aber keine Rechtsberatung. Vor dem ersten Verkauf
   einmal prüfen lassen (z. B. per Rechtstexte-Abo oder Anwalt).

## In den Apps

Die Adressen stehen an je einer Stelle und werden von Tests gegen `site/public` geprüft:

- macOS: `native/src/site.rs` (Hilfemenü: Website, Support, Datenschutz, EULA, Impressum)
- iOS/iPadOS: `ios/Packages/PRRadarCore/Sources/PRRadarCore/Site.swift` (Einstellungen → Info)

Ändert sich die Domain, beide Dateien anpassen.

## App Store Connect

| Feld | Wert |
|---|---|
| Privacy Policy URL | `https://pr-radar.rubeen.dev/privacy/` (Deutsch: `/de/datenschutz/`) |
| Support URL | `https://pr-radar.rubeen.dev/support/` (Deutsch: `/de/support/`) |
| Marketing URL | `https://pr-radar.rubeen.dev/` |
| Lizenzvereinbarung | Apples Standard-EULA belassen; die eigene EULA gilt nur für die direkt vertriebene Mac-App |
| App-Datenschutz („Nutritional Label“) | „Keine Daten erfasst“ – passt zu `ios/PRRadar/PrivacyInfo.xcprivacy` |
