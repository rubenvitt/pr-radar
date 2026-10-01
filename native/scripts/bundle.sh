#!/usr/bin/env bash
# Baut „PR Radar.app“ (Release) und installiert sie optional nach /Applications.
# Systembenachrichtigungen funktionieren unter macOS nur aus einem App-Bundle.
#   scripts/bundle.sh            → native/dist/PR Radar.app
#   scripts/bundle.sh --install  → zusätzlich nach /Applications kopieren
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release
target_dir=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)

app="dist/PR Radar.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$target_dir/release/pr-radar" "$app/Contents/MacOS/pr-radar"
[ -f assets/AppIcon.icns ] && cp assets/AppIcon.icns "$app/Contents/Resources/AppIcon.icns"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>PR Radar</string>
  <key>CFBundleDisplayName</key><string>PR Radar</string>
  <key>CFBundleIdentifier</key><string>dev.rubeen.pr-radar</string>
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

codesign --force --deep --sign - "$app" >/dev/null 2>&1 || true
echo "→ $app"

if [ "${1:-}" = "--install" ]; then
  rm -rf "/Applications/PR Radar.app"
  cp -R "$app" /Applications/
  echo "→ /Applications/PR Radar.app"
fi
