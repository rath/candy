#!/bin/sh
set -eu
candy_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$candy_root"
export MACOSX_DEPLOYMENT_TARGET=14.0
case "${1:-release}" in
    release) candy_profile=release; set -- --release ;;
    --debug) candy_profile=debug; set -- ;;
    *) echo "Usage: scripts/build-app.sh [--debug]" >&2; exit 2 ;;
esac
cargo build --locked -p candy -p candy-ffi "$@"
export CANDY_RUST_PROFILE="$candy_profile"
scripts/swift.sh build -c "$candy_profile"
candy_swift_bin=$(scripts/swift.sh build -c "$candy_profile" --show-bin-path)
candy_app="$candy_root/dist/Candy.app"
mkdir -p "$candy_app/Contents/MacOS" "$candy_app/Contents/Resources" "$candy_app/Contents/Helpers"
cp "$candy_swift_bin/Candy" "$candy_app/Contents/MacOS/Candy"
cp "${CARGO_TARGET_DIR:-target}/$candy_profile/candy" "$candy_app/Contents/Helpers/candy"
cp LICENSE "$candy_app/Contents/Resources/LICENSE"
cat > "$candy_app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
    <key>CFBundleExecutable</key><string>Candy</string>
    <key>CFBundleIdentifier</key><string>org.candyapp.Candy</string>
    <key>CFBundleName</key><string>Candy</string>
    <key>CFBundleDisplayName</key><string>Candy</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
    <key>CFBundleDocumentTypes</key><array><dict>
        <key>CFBundleTypeName</key><string>Image</string>
        <key>CFBundleTypeRole</key><string>Editor</string>
        <key>LSHandlerRank</key><string>Alternate</string>
        <key>LSItemContentTypes</key><array>
            <string>public.png</string><string>public.jpeg</string><string>org.webmproject.webp</string>
            <string>public.heic</string><string>public.heif</string><string>public.avif</string>
        </array>
    </dict></array>
    <key>NSHumanReadableCopyright</key><string>Copyright © 2026 Candy contributors. MIT license.</string>
</dict></plist>
PLIST
plutil -lint "$candy_app/Contents/Info.plist"
printf 'Built %s\n' "$candy_app"
