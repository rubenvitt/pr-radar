#!/bin/sh
# Build-Schritt für Cloudflare Pages (Projekt r-pr-radar).
#   Build command:          sh site/build.sh
#   Build output directory: site/public
#
# Die Seite ist statisches HTML und braucht keinen Build. Das Skript prüft nur,
# ob noch Platzhalter (TODO-RUBEN) offen sind: in Vorschau-Builds als Warnung,
# auf main (Produktion) oder mit --strict als Fehler, damit kein Impressum ohne
# Anschrift live geht.
set -eu
cd "$(dirname "$0")"

python3 check.py public

todos=$(grep -rl 'TODO-RUBEN' public || true)
if [ -n "$todos" ]; then
  echo "Offene Platzhalter (TODO-RUBEN) in:"
  echo "$todos" | sed 's/^/  /'
  if [ "${CF_PAGES_BRANCH:-}" = "main" ] || [ "${1:-}" = "--strict" ]; then
    echo "Produktions-Build abgebrochen: erst die Platzhalter füllen (site/README.md)."
    exit 1
  fi
  echo "Vorschau-Build: Platzhalter erlaubt."
fi
echo "Seite bereit: site/public"
