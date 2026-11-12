#!/usr/bin/env bash
# Packt „dist/PR Radar.app“ in ein Disk-Image zum Herunterladen: App plus Verknüpfung zu
# /Applications zum Hineinziehen. Vorher scripts/bundle.sh, danach scripts/notarize.sh.
#   scripts/dmg.sh                    → dist/PR-Radar.dmg
#   scripts/dmg.sh dist/Name.dmg      → anderer Dateiname
#   CODESIGN_IDENTITY=… scripts/dmg.sh → Image mit Developer ID signieren (für die Beglaubigung)
set -euo pipefail
cd "$(dirname "$0")/.."

app="dist/PR Radar.app"
out=${1:-dist/PR-Radar.dmg}
[ -d "$app" ] || { echo "Fehlt: $app (erst scripts/bundle.sh)" >&2; exit 1; }

staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
ditto "$app" "$staging/PR Radar.app"
ln -s /Applications "$staging/Applications"

rm -f "$out"
hdiutil create -volname "PR Radar" -srcfolder "$staging" -fs HFS+ -format UDZO -ov "$out" >/dev/null

if [ -n "${CODESIGN_IDENTITY:-}" ]; then
  codesign --force --timestamp --sign "$CODESIGN_IDENTITY" "$out"
  codesign --verify --strict --verbose=2 "$out"
fi
hdiutil verify "$out" >/dev/null
echo "→ $out"
