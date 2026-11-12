# Releases

Releases laufen automatisch. Jeder Push auf `main` startet `.github/workflows/release.yml`. Der Workflow
schaut pro Plattform auf die Commits seit deren letztem Tag, die ihren Ordner berühren, und leitet daraus die
nächste Version ab (Conventional Commits, `.github/scripts/next-version.sh`):

| Commit | Sprung |
|---|---|
| `feat: …` | Minor (0.1.0 → 0.2.0) |
| `fix: …`, `perf: …` | Patch (0.1.0 → 0.1.1) |
| `feat!: …` oder `BREAKING CHANGE:` im Text | Major, vor 1.0 nur Minor |
| alles andere (`docs`, `ci`, `chore`, `refactor`, `test`, …) | kein Release |

| Plattform | Ordner | Tag | Workflow | Ergebnis |
|---|---|---|---|---|
| macOS | `native/` | `macos-vX.Y.Z` | `.github/workflows/release-macos.yml` | GitHub-Release mit signiertem, beglaubigtem `PR-Radar-X.Y.Z-macOS.dmg` (Universal: Apple Silicon und Intel) und SHA-256 |
| iOS | `ios/` | `ios-vX.Y.Z` | `.github/workflows/release-ios.yml` | Build in App Store Connect bzw. TestFlight |

Maßgeblich ist der Tag. Die Versionen in `native/Cargo.toml` und `MARKETING_VERSION` bleiben im Repo stehen;
der Workflow setzt sie nur für den Build. Der Tag entsteht erst, wenn Build und Upload geklappt haben.

macOS: Der Workflow baut ein Universal-Binary (arm64 + x86_64, `lipo`), signiert mit der Developer ID und
gehärteter Laufzeit (`native/PRRadar.entitlements`), lässt erst die App, dann das Disk-Image von Apple
beglaubigen, heftet jeweils das Ticket an und legt Tag und GitHub-Release mit automatischen Release Notes an
(ab dem vorigen `macos-v*`-Tag). Dazu kommt `appcast.xml` für die Auto-Updates (siehe *Sparkle* unten). Das
Release wird als *Latest* markiert, damit `https://github.com/rubenvitt/pr-radar/releases/latest` und der
Update-Feed immer auf die aktuelle Mac-App zeigen (iOS legt keine GitHub-Releases an).

iOS: Der Workflow archiviert, signiert für den App Store, lädt hoch und setzt danach den Tag. Die Build-Nummer
ist `Laufnummer.Versuch` von `release.yml` und steigt dadurch von selbst. Nach der Verarbeitung durch Apple
(meist 10 bis 30 min) erscheint der Build in TestFlight. Für den App Store reicht man ihn dann in App Store
Connect zur Prüfung ein.

Schlägt ein Lauf fehl, gibt es keinen Tag. Entweder den Lauf in *Actions* neu starten (*Re-run failed jobs*)
oder die Ursache beheben und mergen: Der nächste Lauf nimmt die Commits des gescheiterten mit. Bei macOS vorher
unter *Releases* nachsehen, ob ein Entwurf `macos-vX.Y.Z` übrig ist (Upload abgebrochen), und ihn löschen, sonst
scheitert `gh release create`.

## Secrets

Unter *Settings → Secrets and variables → Actions → Repository secrets* anlegen.

### Für beide: App-Store-Connect-API-Key

In [App Store Connect](https://appstoreconnect.apple.com/access/integrations/api) unter *Benutzer und Zugriff →
Integrationen → App Store Connect API* einen Team-Key anlegen, Rolle **App Manager** (reicht für Upload und
Beglaubigung). Die `.p8`-Datei lässt sich nur einmal herunterladen.

| Secret | Inhalt |
|---|---|
| `ASC_KEY_ID` | Schlüssel-ID (10 Zeichen) |
| `ASC_ISSUER_ID` | Issuer-ID (UUID oben auf der Seite) |
| `ASC_KEY_P8` | Inhalt der Datei `AuthKey_XXXXXXXXXX.p8`, so wie er ist (mit `-----BEGIN PRIVATE KEY-----`) |

### macOS: Developer-ID-Zertifikat

Im [Developer-Portal](https://developer.apple.com/account/resources/certificates/list) ein Zertifikat vom Typ
**Developer ID Application** erstellen (nur der Account Holder darf das), in den Schlüsselbund laden und dort
*Zertifikat samt privatem Schlüssel* als `.p12` mit Passwort exportieren.

| Secret | Inhalt |
|---|---|
| `MACOS_CERTIFICATE_P12` | `base64 -i DeveloperID.p12 \| pbcopy` |
| `MACOS_CERTIFICATE_PASSWORD` | Passwort des `.p12` |

### macOS: Sparkle (Auto-Updates)

Die Mac-App sucht per [Sparkle](https://sparkle-project.org) nach Updates. Der Feed ist
`https://github.com/rubenvitt/pr-radar/releases/latest/download/appcast.xml`; der Workflow legt ihn als Asset
an jedes Release. Updates sind mit EdDSA signiert, Sparkle installiert nur, was zum öffentlichen Schlüssel in der
App passt. **Den privaten Schlüssel nie verlieren:** Ohne ihn bekommen bestehende Installationen keine Updates
mehr und müssen von Hand neu laden.

Einmalig auf dem Mac:

```sh
curl -LO https://github.com/sparkle-project/Sparkle/releases/download/2.9.0/Sparkle-2.9.0.tar.xz
mkdir sparkle && tar -xf Sparkle-2.9.0.tar.xz -C sparkle
sparkle/bin/generate_keys                     # legt das Schlüsselpaar im Schlüsselbund an, zeigt den öffentlichen
sparkle/bin/generate_keys -x sparkle-private.key   # privaten Schlüssel exportieren (danach sicher löschen)
```

| Wo | Name | Inhalt |
|---|---|---|
| *Variables* (nicht Secrets) | `SPARKLE_PUBLIC_KEY` | öffentlicher Schlüssel aus `generate_keys` (Base64) |
| *Secrets* | `SPARKLE_ED_PRIVATE_KEY` | Inhalt von `sparkle-private.key` |

Fehlen beide, baut der Workflow wie bisher ohne Sparkle (Warnung im Lauf). Fehlt nur eins, bricht er ab.
`CFBundleVersion` ist die Zahl der Commits auf `main`; Sparkle vergleicht diese Nummer, nicht `X.Y.Z`.
Lokal bettet `SPARKLE_PUBLIC_KEY=… scripts/bundle.sh` Sparkle ebenfalls ein. Das lädt nur mit Developer-ID-
Signatur (`CODESIGN_IDENTITY`); ad hoc signiert verweigert die gehärtete Laufzeit das Framework, die App läuft
dann ohne Updates weiter.

### iOS: Verteilungszertifikat und Profil

1. Zertifikat vom Typ **Apple Distribution** erstellen und wie oben als `.p12` exportieren.
2. Falls noch nicht geschehen: App-ID `dev.rubeen.prradar` registrieren und in App Store Connect eine App
   mit dieser Bundle-ID anlegen.
3. Provisioning-Profil vom Typ **App Store Connect** für `dev.rubeen.prradar` mit diesem Zertifikat erstellen
   und herunterladen.

| Secret | Inhalt |
|---|---|
| `IOS_DISTRIBUTION_P12` | `base64 -i AppleDistribution.p12 \| pbcopy` |
| `IOS_DISTRIBUTION_PASSWORD` | Passwort des `.p12` |
| `IOS_PROVISIONING_PROFILE` | `base64 -i PR_Radar_App_Store.mobileprovision \| pbcopy` |

Team-ID und Profilname liest der Workflow aus dem Profil; im Projekt bleibt `DEVELOPMENT_TEAM` leer und
die lokale automatische Signierung unverändert. Zertifikat und Profil laufen nach einem Jahr ab und müssen
dann hier erneuert werden.

## Hinweise

- **Exportkonformität:** Ohne `ITSAppUsesNonExemptEncryption` in der Info.plist fragt App Store Connect bei
  jedem Build nach der Verschlüsselung, bevor er in TestFlight freigegeben wird. Nutzt die App nur HTTPS, ist
  `NO` die übliche Antwort. Das ist eine rechtliche Angabe, deshalb ist es bewusst nicht vorbelegt.
- Bis 0.2.1 gab es nur ein ZIP für Apple Silicon (`PR-Radar-X.Y.Z-macOS-arm64.zip`).

## Lokal bauen und beglaubigen

Die CI macht dasselbe mit denselben Skripten (`native/scripts/`). Der PR-Check *App-Bündel und DMG* baut
Universal-Binary und Image ad hoc signiert, damit Fehler nicht erst beim Release auffallen.

```sh
cd native
scripts/bundle.sh                 # nur die Architektur dieses Macs, ad hoc signiert
scripts/bundle.sh --universal     # Apple Silicon + Intel (braucht beide Rust-Targets, das Skript ergänzt sie)
scripts/dmg.sh                    # dist/PR-Radar.dmg mit Verknüpfung zu /Applications

# Wie ein Release: Developer ID, dann beglaubigen
export CODESIGN_IDENTITY="Developer ID Application: …"
export NOTARY_PROFILE=pr-radar
xcrun notarytool store-credentials pr-radar   # einmalig: Apple-ID mit App-Passwort oder API-Key
scripts/bundle.sh --universal && scripts/notarize.sh "dist/PR Radar.app"
scripts/dmg.sh && scripts/notarize.sh dist/PR-Radar.dmg
```

`notarize.sh` nimmt statt `NOTARY_PROFILE` auch `ASC_KEY_PATH`, `ASC_KEY_ID` und `ASC_ISSUER_ID` (so in der CI).
Ad hoc signiert läuft die App nur auf dem eigenen Mac; auf anderen blockiert Gatekeeper sie.
