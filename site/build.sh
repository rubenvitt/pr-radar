#!/bin/sh
# Build-Schritt für Cloudflare (Worker pr-radar, Workers Builds mit statischen
# Assets, Konfiguration in wrangler.jsonc).
#   Build command:  sh site/build.sh
#   Deploy command: npx wrangler deploy
#
# Prüft site/public (Links, Anker, keine fremden Ressourcen) und kopiert es nach
# site/dist, das ausgeliefert wird. Build-Variablen braucht es keine.
set -eu
cd "$(dirname "$0")"

python3 check.py public
rm -rf dist
cp -R public dist
echo "Seite bereit: site/dist"
