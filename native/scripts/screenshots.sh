#!/usr/bin/env bash
# Nimmt alle Ansichten im Demo-Modus hell und dunkel auf – ohne Token, ohne Netzwerk.
#
#   scripts/screenshots.sh [Zielordner]      # Standard: screenshots/
#
# Linux: braucht Xvfb, xdotool, ImageMagick (import) und einen Vulkan-Treiber, ohne GPU z. B.
#   apt-get install xvfb xdotool imagemagick libxkbcommon-dev libxkbcommon-x11-dev mesa-vulkan-drivers
# macOS: nutzt screencapture; das Terminal braucht die Freigabe „Bildschirmaufnahme“.
#
# Umgebung: SETTLE=Sekunden bis zur Aufnahme (Standard 4), SIZE=BreitexHöhe (Standard 1240x820),
#           VIEWS="name:argumente;…" ersetzt die Liste der Ansichten.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="${1:-screenshots}"
SETTLE="${SETTLE:-4}"
SIZE="${SIZE:-1240x820}"
VIEWS="${VIEWS:-offen-liste:--tab open --view list;offen-flow:--tab open --view flow;rot:--tab open --quick failed;gemergt:--tab merged;releases:--tab releases}"

mkdir -p "$OUT"
cargo build --quiet
BIN="target/debug/pr-radar"

shoot_linux() { # name args...
  local name="$1"; shift
  "$BIN" --demo "$@" >/dev/null 2>&1 &
  local pid=$!
  local win=""
  for _ in $(seq 1 40); do
    win="$(xdotool search --onlyvisible --pid "$pid" 2>/dev/null | head -n1 || true)"
    [[ -n "$win" ]] && break
    sleep 0.25
  done
  if [[ -z "$win" ]]; then
    kill "$pid" 2>/dev/null || true
    echo "Kein Fenster für $name" >&2
    return 1
  fi
  xdotool windowsize "$win" "${SIZE%x*}" "${SIZE#*x}" windowmove "$win" 0 0
  sleep "$SETTLE"
  import -window "$win" "$OUT/$name.png"
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
}

# Fenster-ID über CoreGraphics (Swift aus den Command Line Tools), dann screencapture -l.
shoot_macos() { # name args...
  local name="$1"; shift
  "$BIN" --demo "$@" >/dev/null 2>&1 &
  local pid=$!
  sleep "$SETTLE"
  local win
  win="$(swift - "$pid" <<'SWIFT'
import CoreGraphics
let pid = Int(CommandLine.arguments[1])!
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
if let w = list.first(where: { ($0[kCGWindowOwnerPID as String] as? Int) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }) {
    print(w[kCGWindowNumber as String] as! Int)
}
SWIFT
)"
  if [[ -z "$win" ]]; then
    kill "$pid" 2>/dev/null || true
    echo "Kein Fenster für $name" >&2
    return 1
  fi
  screencapture -x -o -l "$win" "$OUT/$name.png"
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
}

case "$(uname -s)" in
  Darwin) shoot=shoot_macos ;;
  Linux)
    shoot=shoot_linux
    if [[ -z "${DISPLAY:-}" ]]; then
      export DISPLAY=":${XVFB_DISPLAY:-99}"
      Xvfb "$DISPLAY" -screen 0 "${SIZE}x24" -nolisten tcp >/dev/null 2>&1 &
      XVFB_PID=$!
      trap 'kill "$XVFB_PID" 2>/dev/null || true' EXIT
      sleep 1
    fi
    ;;
  *) echo "Nicht unterstützt: $(uname -s)" >&2; exit 1 ;;
esac

IFS=';' read -r -a entries <<<"$VIEWS"
for appearance in light dark; do
  for entry in "${entries[@]}"; do
    name="${entry%%:*}"
    read -r -a args <<<"${entry#*:}"
    "$shoot" "$name-$appearance" "--$appearance" "${args[@]}"
    echo "$OUT/$name-$appearance.png"
  done
done
