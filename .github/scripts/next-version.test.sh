#!/usr/bin/env bash
# Selbsttest für next-version.sh in einem Wegwerf-Repo.
set -euo pipefail
script=$(cd "$(dirname "$0")" && pwd)/next-version.sh
repo=$(mktemp -d); trap 'rm -rf "$repo"' EXIT
cd "$repo" && git init -q && git config user.email t@t && git config user.name t && git config commit.gpgsign false && git config tag.gpgsign false
commit() { mkdir -p "$1"; echo "$RANDOM" >> "$1/f"; git add -A; git commit -qm "$2"; }
check() { got=$("$script" "$1" "$2"); [ "$got" = "$3" ] || { echo "FAIL: $4: erwartet '$3', bekommen '$got'"; exit 1; }; }

commit a "chore: init";                  check x a "" "nur chore, kein Tag"
commit a "feat(x): erstes Feature";      check x a "0.1.0" "erstes Release ohne Tag"
git tag x-v0.1.0
commit b "feat: anderer Pfad";           check x a "" "Commit außerhalb des Pfads"
commit a "docs: nur Doku";               check x a "" "docs löst nichts aus"
commit a "fix(x): Fehler";               check x a "0.1.1" "fix → patch"
commit a "feat: neu";                    check x a "0.2.0" "feat schlägt fix"
commit a "refactor!: weg damit";         check x a "0.2.0" "Breaking vor 1.0 → minor"
git tag x-v1.0.0
commit a $'fix: x\n\nBREAKING CHANGE: anders'; check x a "2.0.0" "BREAKING CHANGE im Body → major"
git tag x-v2.0.0-beta.1
check x a "2.0.0" "Pre-Release-Tags zählen nicht als Basis"
echo "next-version: alle Tests grün"
