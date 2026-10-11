#!/bin/sh
set -eu
candy_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$candy_root"
export MACOSX_DEPLOYMENT_TARGET=14.0
candy_version=$(cargo pkgid -p candy)
candy_version=${candy_version##*#}
candy_version=${candy_version##*@}
case "${1:-release}" in
    release) candy_profile=release; set -- --release ;;
    --debug) candy_profile=debug; set -- ;;
    *) echo "Usage: scripts/build-app.sh [--debug]" >&2; exit 2 ;;
esac
cargo build --locked -p candy -p candy-ffi "$@"
export CANDY_RUST_PROFILE="$candy_profile"
scripts/swift.sh build -c "$candy_profile"
candy_swift_bin=$(scripts/swift.sh build -c "$candy_profile" --show-bin-path)
mkdir -p "$candy_root/dist"
candy_stage=$(mktemp -d "$candy_root/dist/.candy-build.XXXXXX")
candy_destination="$candy_root/dist/Candy.app"
cleanup() {
    if [ ! -e "$candy_destination" ] && [ -e "$candy_stage/previous.app" ]; then
        mv "$candy_stage/previous.app" "$candy_destination" || return 1
    fi
    rm -rf "$candy_stage"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
candy_app="$candy_stage/Candy.app"
mkdir -p "$candy_app/Contents/MacOS" "$candy_app/Contents/Resources" "$candy_app/Contents/Helpers"
cp "$candy_swift_bin/Candy" "$candy_app/Contents/MacOS/Candy"
cp "${CARGO_TARGET_DIR:-target}/$candy_profile/candy" "$candy_app/Contents/Helpers/candy"
cp LICENSE "$candy_app/Contents/Resources/LICENSE"
cp macos/Resources/AppIcon.icns "$candy_app/Contents/Resources/AppIcon.icns"
cat > "$candy_app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
    <key>CFBundleExecutable</key><string>Candy</string>
    <key>CFBundleIdentifier</key><string>me.told.candy.app</string>
    <key>CFBundleName</key><string>Candy</string>
    <key>CFBundleDisplayName</key><string>Candy</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0</string>
    <key>CFBundleVersion</key><string>0</string>
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
            <string>com.compuserve.gif</string><string>public.tiff</string><string>com.microsoft.bmp</string>
            <string>public.jpeg-2000</string><string>public.jpeg-xl</string><string>com.adobe.photoshop-image</string>
            <string>com.truevision.tga-image</string><string>com.microsoft.ico</string><string>com.apple.icns</string>
            <string>com.ilm.openexr-image</string><string>public.radiance</string><string>public.pbm</string>
            <string>public.camera-raw-image</string><string>com.adobe.raw-image</string>
        </array>
    </dict></array>
    <key>NSHumanReadableCopyright</key><string>Copyright © 2026 Candy contributors. MIT license.</string>
</dict></plist>
PLIST
plutil -replace CFBundleShortVersionString -string "$candy_version" "$candy_app/Contents/Info.plist"
plutil -replace CFBundleVersion -string "$candy_version" "$candy_app/Contents/Info.plist"
plutil -lint "$candy_app/Contents/Info.plist"
# Sign the complete local bundle, including its final Info.plist. Staging avoids
# stale resource seals and case-only executable names from older bundles.
codesign --force --sign - "$candy_app"
codesign --verify --deep --strict "$candy_app"
if [ -e "$candy_destination" ]; then
    mv "$candy_destination" "$candy_stage/previous.app"
fi
if ! mv "$candy_app" "$candy_destination"; then
    if [ -e "$candy_stage/previous.app" ]; then
        mv "$candy_stage/previous.app" "$candy_destination"
    fi
    exit 1
fi
printf 'Built %s\n' "$candy_destination"
