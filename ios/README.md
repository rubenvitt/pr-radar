# PR Radar für iOS

Die iPhone-/iPad-App zur macOS-App – gleiche Funktionen, in SwiftUI. Spricht GitHub direkt an, ohne Server.

- **Offene PRs** gebündelt (nach Repo gruppiert oder flach) mit Review-Status, Konflikten, Labels, Branch, Diff-Größe – als **Liste** oder **Flow** (PR → Checks je Workflow → Review → Merge → Ziel-Branch; scrollt auf dem iPhone horizontal)
- **Pipelines** pro PR: läuft · rot · grün, Detailansicht mit allen Checks und direkten Links
- **Auto-Merge** an/aus bzw. **direkt mergen** – nur mit Methoden, die Repo-Einstellungen **und** Rulesets des Ziel-Branches erlauben. Direktes Mergen braucht einen zweiten Tipp (Menü bzw. Bestätigung), weil es nicht umkehrbar ist
- **Alle mergen** für alle sichtbaren, sofort mergebaren PRs (je Repo und Ziel-Branch nacheinander, mit Bestätigung)
- Lehnt GitHub einen Merge vorübergehend ab, versucht die App es bis zu 3 min erneut bzw. aktiviert Auto-Merge – immer nur auf dem Commit, der beim Tippen zu sehen war
- **Zuletzt gemergt** als Timeline nach Tagen, **Releases** mit Release Notes (neuestes je Repo aufgeklappt)
- **Live**: pollt im Vordergrund alle 30 s, solange eine Pipeline läuft alle 10 s; Pull-to-Refresh; letzter Stand wird zwischengespeichert und beim Start sofort gezeigt
- Schnellfilter-Chips (läuft / rot / grün / Auto-Merge / Konflikt / „Dein Review“ / „Von dir“), Repo-Filter, Suche
- Wischgesten (links: Mergen/Auto-Merge, rechts: GitHub) und Kontextmenü (Merge-Methode, öffnen, Link kopieren, teilen)
- **Benachrichtigungen**, wenn eine Pipeline rot/grün wird oder ein PR gemergt wurde, rote PRs als Badge; im Hintergrund per App-Refresh, so oft iOS es zulässt
- iPad: Seitenleiste, Tastatur `⌘R` neu laden, `⌘1–4` Bereiche, `⌘L` Liste/Flow, `⌘G` gruppieren
- Hell/Dunkel, Dynamic Type, VoiceOver-Beschriftungen; der wandernde Punkt im Flow entfällt bei „Bewegung reduzieren“
- **Demo-Modus** mit Beispieldaten: ohne Token und Repos ausprobieren (Knopf „Mit Beispieldaten ausprobieren“ im leeren Zustand)

## Start

Xcode 16 oder neuer, iOS 18+.

```bash
open ios/PRRadar.xcodeproj
```

Unter *Signing & Capabilities* das eigene Team wählen, dann auf Gerät oder Simulator starten.
Den Token trägt man in der App unter **Einstellungen** ein – er liegt nur im Schlüsselbund des Geräts.
Für die Entwicklung kann man ihn alternativ im Scheme als Umgebungsvariable `GITHUB_TOKEN` setzen (ist vorbereitet, aber deaktiviert).

Token-Rechte wie bei der macOS-App: Fine-grained PAT mit Metadata, Contents, Commit statuses, Actions/Checks (read) und Pull requests (**write** für Merge und Auto-Merge).

## Demo-Modus

Ohne Token oder ohne Repos bietet die App „Mit Beispieldaten ausprobieren“ an. Dann zeigt sie drei erfundene
Repos (`acme/web`, `acme/api`, `acme/mobile`) mit allen wichtigen Zuständen: Pipeline läuft, rot, grün, Konflikt,
Draft, Auto-Merge, angefragtes Review, Releases und eine Gemergt-Timeline. Die Daten kommen aus
`PRRadarCore/Demo.swift` (`DemoData`, `DemoGitHub`), nicht aus dem Netz. Mergen und Auto-Merge wirken auf den
Demo-Stand, laufende Pipelines werden nach 20 bzw. 60 s grün. Konfiguration, Cache und Schlüsselbund bleiben unberührt.

Eine Leiste oben zeigt den Modus an und beendet ihn. Wer einen Token speichert, verlässt den Demo-Modus automatisch.
Für Screenshots und UI-Tests startet die App mit dem Startargument `--demo` direkt in der Demo
(Xcode: Scheme → Run → Arguments, Simulator: `xcrun simctl launch booted email.rubeen.prradar --demo`).

Hinweis für die App-Review-Notizen: „Die App braucht ein GitHub-Konto. Ohne Anmeldung lässt sie sich über
‚Mit Beispieldaten ausprobieren‘ auf dem Startbildschirm vollständig mit Beispieldaten testen.“

## Aufbau

| Pfad | Inhalt |
|---|---|
| `Packages/PRRadarCore/` | Plattformunabhängiger Kern: GraphQL-Queries, Client, Normalisierung, Rulesets, Poll-Zyklus, Merge/Auto-Merge inkl. Wiederholung, Filter und Flow-Stufen – portiert aus `native/src` |
| `PRRadar/Views/` | SwiftUI: Tabs, Liste, Flow, Detail, Gemergt, Releases, Einstellungen |
| `PRRadar/Support/` | Schlüsselbund, Benachrichtigungen, App-Zustand |

Der Kern hat keine UI-Abhängigkeit und läuft auch unter macOS und Linux:

```bash
cd ios/Packages/PRRadarCore
swift test
```

In Xcode laufen dieselben Tests mit `⌘U`. Die CI (`.github/workflows/ios.yml`) testet den Kern unter Linux und baut die App für den iOS-Simulator.

## Datenschutz

`PRRadar/PrivacyInfo.xcprivacy` ist das Privacy Manifest für den App Store: kein Tracking, keine erhobenen
Daten (die App spricht nur direkt mit GitHub, der Token bleibt im Schlüsselbund). Required-Reason-APIs:
Datei-Zeitstempel (`C617.1`, Alter des Snapshot-Caches) und UserDefaults (`CA92.1`, nur app-eigene Werte).
Wer neue Required-Reason-APIs oder SDKs einbaut, ergänzt das Manifest.
