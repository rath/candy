#!/bin/sh
set -eu
candy_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$candy_root"
case "${1:-release}" in
    release) cargo build --locked --release -p candy; candy_profile=release ;;
    --debug) cargo build --locked -p candy; candy_profile=debug ;;
    *) echo "Usage: scripts/build-app.sh [--debug]" >&2; exit 2 ;;
esac
candy_app="$candy_root/dist/Candy.app"
mkdir -p "$candy_app/Contents/MacOS" "$candy_app/Contents/Resources"
cp "${CARGO_TARGET_DIR:-target}/$candy_profile/candy" "$candy_app/Contents/MacOS/candy"
cp LICENSE "$candy_app/Contents/Resources/LICENSE"
cat > "$candy_app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
    <key>CFBundleExecutable</key><string>candy</string>
    <key>CFBundleIdentifier</key><string>org.candyapp.Candy</string>
    <key>CFBundleName</key><string>Candy</string>
    <key>CFBundleDisplayName</key><string>Candy</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
    <key>NSHumanReadableCopyright</key><string>Copyright © 2026 Candy contributors. MIT license.</string>
</dict></plist>
PLIST
plutil -lint "$candy_app/Contents/Info.plist"
printf 'Built %s\n' "$candy_app"
