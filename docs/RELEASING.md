# Releases

Releases laufen über Tags. Jede Plattform hat ihren eigenen Tag und ihre eigene Version.

| Plattform | Tag | Version steht in | Workflow | Ergebnis |
|---|---|---|---|---|
| macOS | `macos-vX.Y.Z` | `native/Cargo.toml` (`version`) | `.github/workflows/release-macos.yml` | GitHub-Release mit signierter, beglaubigter `PR-Radar-X.Y.Z-macOS-arm64.zip` und SHA-256 |
| iOS | `ios-vX.Y.Z` | `MARKETING_VERSION` im Xcode-Projekt | `.github/workflows/release-ios.yml` | Build in App Store Connect bzw. TestFlight |

Passt der Tag nicht zur Version im Code, bricht der Workflow ab, bevor er etwas signiert.

## Ablauf

macOS:

1. `version` in `native/Cargo.toml` erhöhen, `cargo check` (aktualisiert `Cargo.lock`), committen, nach `main`.
2. Taggen und pushen:

   ```bash
   git tag macos-v0.2.0 && git push origin macos-v0.2.0
   ```

3. Der Workflow baut, signiert mit der Developer ID, lässt die App von Apple beglaubigen, heftet das Ticket an
   und legt das GitHub-Release mit automatischen Release Notes an (ab dem vorigen `macos-v*`-Tag).
   Versionen mit Bindestrich (`0.2.0-beta.1`) werden als Pre-Release markiert.

iOS:

1. `MARKETING_VERSION` in Xcode erhöhen (Target PR Radar → General → Version), committen, nach `main`.
2. Taggen und pushen:

   ```bash
   git tag ios-v0.2.0 && git push origin ios-v0.2.0
   ```

3. Der Workflow archiviert, signiert für den App Store und lädt hoch. Die Build-Nummer ist die Laufnummer des
   Workflows und steigt dadurch von selbst. Nach der Verarbeitung durch Apple (meist 10 bis 30 min) erscheint
   der Build in TestFlight. Für den App Store reicht man ihn dann in App Store Connect zur Prüfung ein.

Schlägt ein Lauf fehl, Ursache beheben, dann den Tag des fehlgeschlagenen Laufs löschen und auf dem
korrigierten Commit neu setzen:

```bash
TAG=macos-v0.2.0   # bzw. ios-v0.2.0
git push --delete origin "$TAG" && git tag -d "$TAG"
git tag "$TAG" && git push origin "$TAG"
```

Bei macOS vorher unter *Releases* nachsehen: Ist das Release veröffentlicht und hängen ZIP und `.sha256` dran,
ist es fertig und es braucht keinen neuen Lauf. Ist es noch ein Entwurf oder fehlt eine der beiden Dateien, das
Release vor dem Neusetzen löschen, sonst scheitert `gh release create`. Bei iOS lehnt App Store Connect eine schon hochgeladene Build-Nummer ab; ein neuer Lauf
bekommt aber ohnehin eine neue.

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
2. Falls noch nicht geschehen: App-ID `email.rubeen.prradar` registrieren und in App Store Connect eine App
   mit dieser Bundle-ID anlegen.
3. Provisioning-Profil vom Typ **App Store Connect** für `email.rubeen.prradar` mit diesem Zertifikat erstellen
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
