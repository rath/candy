#!/bin/sh
# Render the application icon from scripts/icon/*.svg into macos/Resources/AppIcon.icns.
set -eu
candy_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
if ! command -v rsvg-convert >/dev/null; then
    echo 'rsvg-convert is required: brew install librsvg' >&2
    exit 1
fi
candy_tmp=$(mktemp -d "${TMPDIR:-/tmp}/candy-icon.XXXXXX")
trap 'rm -rf "$candy_tmp"' EXIT
trap 'exit 1' HUP INT TERM
candy_iconset="$candy_tmp/AppIcon.iconset"
mkdir "$candy_iconset"

render() {
    rsvg-convert --width "$2" --height "$2" --output "$candy_iconset/$3.png" \
        "$candy_root/scripts/icon/$1.svg"
}
# The full spiral blurs at 16 points, so that slot uses simplified artwork.
render app-icon-small 16 icon_16x16
render app-icon-small 32 icon_16x16@2x
for candy_size in 32 128 256 512; do
    render app-icon "$candy_size" "icon_${candy_size}x${candy_size}"
    render app-icon $((candy_size * 2)) "icon_${candy_size}x${candy_size}@2x"
done
iconutil --convert icns --output "$candy_tmp/AppIcon.icns" "$candy_iconset"
mv "$candy_tmp/AppIcon.icns" "$candy_root/macos/Resources/AppIcon.icns"
printf 'Built %s/macos/Resources/AppIcon.icns\n' "$candy_root"
