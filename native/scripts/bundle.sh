#!/usr/bin/env bash
# Baut „PR Radar.app“ (Release) und installiert sie optional nach /Applications.
# Systembenachrichtigungen funktionieren unter macOS nur aus einem App-Bundle.
#   scripts/bundle.sh              → native/dist/PR Radar.app (Architektur dieses Macs)
#   scripts/bundle.sh --universal  → Universal-Binary für Apple Silicon und Intel (wie die CI)
#   scripts/bundle.sh --install    → zusätzlich nach /Applications kopieren
#   CODESIGN_IDENTITY=… scripts/bundle.sh → mit Developer ID statt ad hoc signieren
# Danach: scripts/dmg.sh (Disk-Image) und scripts/notarize.sh (Beglaubigung), siehe docs/RELEASING.md.
set -euo pipefail
cd "$(dirname "$0")/.."

universal=false
install=false
for arg in "$@"; do
  case "$arg" in
    --universal) universal=true ;;
    --install) install=true ;;
    *) echo "Unbekannte Option: $arg" >&2; exit 2 ;;
  esac
done

# Ohne Drittanbieter-Hinweise darf die App nicht raus (MIT/Apache verlangen sie).
# cargo-about erzeugt sie; fehlt es, wird es einmalig nach ~/.cargo/bin installiert.
if ! cargo about --version >/dev/null 2>&1; then
  echo "→ cargo-about fehlt, wird installiert (einmalig)"
  cargo install --locked --features cli cargo-about
fi

target_dir=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
if $universal; then
  # Beide Architekturen bauen und mit lipo zu einem Programm zusammenführen.
  targets=(aarch64-apple-darwin x86_64-apple-darwin)
  if command -v rustup >/dev/null 2>&1; then
    rustup target add "${targets[@]}" >/dev/null
  fi
  cargo build --release --target "${targets[0]}" --target "${targets[1]}"
  mkdir -p "$target_dir/universal/release"
  binary="$target_dir/universal/release/pr-radar"
  lipo -create -output "$binary" \
    "$target_dir/${targets[0]}/release/pr-radar" "$target_dir/${targets[1]}/release/pr-radar"
  lipo "$binary" -verify_arch arm64 x86_64
  echo "→ Universal-Binary: $(lipo -archs "$binary")"
else
  cargo build --release
  binary="$target_dir/release/pr-radar"
fi
version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)

app="dist/PR Radar.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
# Leere Sprachordner: macOS bietet die App dann unter „Sprache pro App“ an und
# übersetzt System-Menüeinträge (z. B. Dienste) passend zur gewählten Sprache.
mkdir -p "$app/Contents/Resources/en.lproj" "$app/Contents/Resources/de.lproj"
cp "$binary" "$app/Contents/MacOS/pr-radar"
[ -f assets/AppIcon.icns ] && cp assets/AppIcon.icns "$app/Contents/Resources/AppIcon.icns"
# Menü Hilfe → „Drittanbieter-Lizenzen“ öffnet diese Datei (src/notices.rs).
cargo about generate --locked --fail about.hbs -o "$app/Contents/Resources/ThirdPartyNotices.html"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>PR Radar</string>
  <key>CFBundleDisplayName</key><string>PR Radar</string>
  <key>CFBundleIdentifier</key><string>dev.rubeen.pr-radar</string>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleLocalizations</key><array><string>en</string><string>de</string></array>
  <key>CFBundleExecutable</key><string>pr-radar</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
</dict>
</plist>
PLIST

# Gehärtete Laufzeit mit den Entitlements aus PRRadar.entitlements, in beiden Fällen gleich, damit
# lokal auffällt, was in der signierten Fassung scheitern würde. Kein --deep: Verschachtelter Code
# (Frameworks) wird einzeln und von innen nach außen signiert, sobald es welchen gibt.
sign=(codesign --force --options runtime --entitlements PRRadar.entitlements)
if [ -n "${CODESIGN_IDENTITY:-}" ]; then
  # Developer ID mit Zeitstempel: Voraussetzung für die Beglaubigung (Notarisierung) durch Apple.
  # So signiert der Release-Workflow (.github/workflows/release-macos.yml).
  "${sign[@]}" --timestamp --sign "$CODESIGN_IDENTITY" "$app"
else
  # Lokal ohne Zertifikat: Ad-hoc-Signatur, reicht für den eigenen Mac.
  "${sign[@]}" --sign - "$app"
fi
codesign --verify --strict --verbose=2 "$app"
echo "→ $app"

if $install; then
  rm -rf "/Applications/PR Radar.app"
  cp -R "$app" /Applications/
  echo "→ /Applications/PR Radar.app"
fi
