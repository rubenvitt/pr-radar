#!/bin/sh
# Build-Schritt für Cloudflare (Worker pr-radar, Workers Builds mit statischen
# Assets, Konfiguration in wrangler.jsonc).
#   Build command:  sh site/build.sh
#   Deploy command: npx wrangler deploy
#
# 1. prüft site/public (Links, Anker, keine fremden Ressourcen),
# 2. erzeugt site/dist und setzt die Anschrift aus den Build-Variablen
#    IMPRESSUM_STRASSE und IMPRESSUM_ORT ein (render.py),
# 3. prüft site/dist noch einmal.
# Auf main (Produktion) oder mit --strict bricht der Build ab, wenn eine
# Variable fehlt, damit kein Impressum ohne Anschrift live geht.
set -eu
cd "$(dirname "$0")"

python3 check.py public

# Produktion erkennen: Workers Builds setzt WORKERS_CI_BRANCH, Cloudflare Pages
# setzte CF_PAGES_BRANCH. Läuft der Build bei Cloudflare, ohne dass ein Branch
# erkennbar ist, gilt er sicherheitshalber ebenfalls als Produktion.
branch="${WORKERS_CI_BRANCH:-${CF_PAGES_BRANCH:-}}"
on_cloudflare="${WORKERS_CI:-${CF_PAGES:-}}"
production=0
if [ "$branch" = "main" ] || { [ -n "$on_cloudflare" ] && [ -z "$branch" ]; } ||
  [ "${1:-}" = "--strict" ]; then
  production=1
fi

python3 render.py public dist "$production"
python3 check.py dist
echo "Seite bereit: site/dist"
