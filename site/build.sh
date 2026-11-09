#!/bin/sh
# Build-Schritt für Cloudflare (Projekt pr-radar, Workers Builds mit
# statischen Assets, Konfiguration in wrangler.jsonc).
#   Build command:  sh site/build.sh
#   Deploy command: npx wrangler deploy
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
  # Produktion erkennen: Workers Builds setzt WORKERS_CI_BRANCH, Cloudflare
  # Pages setzte CF_PAGES_BRANCH. Beides prüfen, damit ein Wechsel der
  # Plattform die Sperre nicht still aushebelt.
  # Läuft der Build bei Cloudflare, ohne dass ein Branch erkennbar ist, gilt er
  # sicherheitshalber ebenfalls als Produktion.
  branch="${WORKERS_CI_BRANCH:-${CF_PAGES_BRANCH:-}}"
  on_cloudflare="${WORKERS_CI:-${CF_PAGES:-}}"
  if [ "$branch" = "main" ] || { [ -n "$on_cloudflare" ] && [ -z "$branch" ]; } ||
    [ "${1:-}" = "--strict" ]; then
    echo "Produktions-Build abgebrochen: erst die Platzhalter füllen (site/README.md)."
    exit 1
  fi
  echo "Vorschau-Build: Platzhalter erlaubt."
fi
echo "Seite bereit: site/public"
