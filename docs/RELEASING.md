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
| macOS | `native/` | `macos-vX.Y.Z` | `.github/workflows/release-macos.yml` | GitHub-Release mit signierter, beglaubigter `PR-Radar-X.Y.Z-macOS-arm64.zip` und SHA-256 |
| iOS | `ios/` | `ios-vX.Y.Z` | `.github/workflows/release-ios.yml` | Build in App Store Connect bzw. TestFlight |

Maßgeblich ist der Tag. Die Versionen in `native/Cargo.toml` und `MARKETING_VERSION` bleiben im Repo stehen;
der Workflow setzt sie nur für den Build. Der Tag entsteht erst, wenn Build und Upload geklappt haben.

macOS: Der Workflow baut, signiert mit der Developer ID, lässt die App von Apple beglaubigen, heftet das
Ticket an und legt Tag und GitHub-Release mit automatischen Release Notes an (ab dem vorigen `macos-v*`-Tag).

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

- **macOS nur Apple Silicon:** Der Runner (`macos-15`) baut für arm64. Für Intel-Macs bräuchte es einen
  zweiten Build für `x86_64-apple-darwin` und `lipo`.
- **Exportkonformität:** Ohne `ITSAppUsesNonExemptEncryption` in der Info.plist fragt App Store Connect bei
  jedem Build nach der Verschlüsselung, bevor er in TestFlight freigegeben wird. Nutzt die App nur HTTPS, ist
  `NO` die übliche Antwort. Das ist eine rechtliche Angabe, deshalb ist es bewusst nicht vorbelegt.
- Lokal baut `scripts/bundle.sh` weiter ad hoc signiert. Mit `CODESIGN_IDENTITY="Developer ID Application: …"`
  signiert es wie die CI (gehärtete Laufzeit, Zeitstempel).
