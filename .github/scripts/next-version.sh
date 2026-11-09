#!/usr/bin/env bash
# Nächste Version einer Plattform aus Conventional Commits seit ihrem letzten Tag.
# Aufruf: next-version.sh <tag-präfix> <pfad>…   z. B. next-version.sh macos native
# Ausgabe: neue Version (0.2.0) oder nichts, wenn kein feat/fix/perf/Breaking Change dabei ist.
set -euo pipefail
prefix=$1; shift

last=$(git tag --list "$prefix-v*" --sort=-v:refname | grep -E "^$prefix-v[0-9]+\.[0-9]+\.[0-9]+$" | head -1 || true)
if [ -n "$last" ]; then range="$last..HEAD"; base=${last#"$prefix"-v}; else range=HEAD; base=0.0.0; fi

level=0 # 0 nichts, 1 patch, 2 minor, 3 major
while IFS= read -r -d '' msg; do
  subject=${msg%%$'\n'*}
  if [[ $subject =~ ^[a-z]+(\([^\)]*\))?!: ]] || grep -qE '^BREAKING[ -]CHANGE:' <<<"$msg"; then n=3
  elif [[ $subject =~ ^feat(\([^\)]*\))?: ]]; then n=2
  elif [[ $subject =~ ^(fix|perf)(\([^\)]*\))?: ]]; then n=1
  else n=0; fi
  (( n > level )) && level=$n
done < <(git log -z --no-merges --format=%B "$range" -- "$@")

IFS=. read -r major minor patch <<<"$base"
# Vor 1.0 bricht schon ein Minor-Sprung die Kompatibilität; aus Breaking wird dort Minor.
(( major == 0 && level == 3 )) && level=2
case $level in
  3) echo "$((major + 1)).0.0" ;;
  2) echo "$major.$((minor + 1)).0" ;;
  1) echo "$major.$minor.$((patch + 1))" ;;
esac
