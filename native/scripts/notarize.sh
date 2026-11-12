#!/usr/bin/env bash
# Lässt eine mit Developer ID signierte .app oder .dmg von Apple beglaubigen (Notarisierung),
# heftet das Ticket an (stapeln) und prüft das Ergebnis wie Gatekeeper beim Öffnen.
#   scripts/notarize.sh "dist/PR Radar.app"
#   scripts/notarize.sh dist/PR-Radar.dmg
# Zugang, eins von beiden:
#   NOTARY_PROFILE=<name>  – lokal, vorher einmal: xcrun notarytool store-credentials <name>
#   ASC_KEY_PATH, ASC_KEY_ID, ASC_ISSUER_ID – App-Store-Connect-API-Key (so macht es die CI)
set -euo pipefail

target=${1:?Aufruf: scripts/notarize.sh <Pfad zu .app oder .dmg>}
[ -e "$target" ] || { echo "Fehlt: $target" >&2; exit 1; }

if [ -n "${NOTARY_PROFILE:-}" ]; then
  auth=(--keychain-profile "$NOTARY_PROFILE")
elif [ -n "${ASC_KEY_PATH:-}" ] && [ -n "${ASC_KEY_ID:-}" ] && [ -n "${ASC_ISSUER_ID:-}" ]; then
  auth=(--key "$ASC_KEY_PATH" --key-id "$ASC_KEY_ID" --issuer "$ASC_ISSUER_ID")
else
  echo "Kein Zugang: NOTARY_PROFILE oder ASC_KEY_PATH/ASC_KEY_ID/ASC_ISSUER_ID setzen" >&2
  exit 2
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
case "$target" in
  *.app)
    # notarytool nimmt keine Ordner: Die App geht als ZIP hoch, angeheftet wird an die App selbst.
    upload="$tmp/$(basename "$target" .app).zip"
    ditto -c -k --keepParent "$target" "$upload"
    ;;
  *.dmg) upload=$target ;;
  *) echo "Nur .app oder .dmg: $target" >&2; exit 2 ;;
esac

echo "→ Beglaubigung läuft (meist wenige Minuten): $target"
result=$(xcrun notarytool submit "$upload" "${auth[@]}" --wait --timeout 30m --output-format json) || true
echo "$result"
field() { printf '%s' "$result" | python3 -c "import json,sys; print(json.load(sys.stdin).get('$1', ''))"; }
id=$(field id)
status=$(field status)
if [ "$status" != "Accepted" ]; then
  [ -n "$id" ] && xcrun notarytool log "$id" "${auth[@]}" || true
  echo "Beglaubigung abgelehnt: ${status:-unbekannt}" >&2
  exit 1
fi

xcrun stapler staple "$target"
xcrun stapler validate "$target"
case "$target" in
  *.app) spctl --assess --type execute --verbose=2 "$target" ;;
  *.dmg) spctl --assess --type open --context context:primary-signature --verbose=2 "$target" ;;
esac
echo "→ beglaubigt und angeheftet: $target"
