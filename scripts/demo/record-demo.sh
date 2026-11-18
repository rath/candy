#!/bin/sh
# Records the landing-page demo: renders the scenes, then opens each one in the release build of
# Candy and records the edits, chat, spreadsheet and order page for every language. Raw movies
# and the finished order-page stills go to WORK_DIR/raw; build-video.py turns them into the site
# assets.
#
# The recording moves the real pointer and brings Candy to the front for about four minutes.
# Run it only while nobody is using the Mac. Set CANDY_DEMO_DRY_RUN=1 to check every scene
# without touching the mouse or activating Candy.
#
#   scripts/build-app.sh
#   scripts/demo/record-demo.sh [WORK_DIR]         # default .impeccable/review/demo
#   python3 scripts/demo/build-video.py [WORK_DIR]
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
work=${1:-$root/.impeccable/review/demo}
mkdir -p "$work/raw"
work=$(cd "$work" && pwd)
app=$root/dist/Candy.app
[ -d "$app" ] || { echo "error: build $app first with scripts/build-app.sh" >&2; exit 1; }
dry=${CANDY_DEMO_DRY_RUN:+--dry-run}

# Candy instances left by an interrupted run would keep their windows in the recordings.
trap 'pkill -f "$app/Contents/MacOS/Candy" 2>/dev/null || true' EXIT

swiftc -O -swift-version 5 -o "$work/record" "$root/scripts/demo/record.swift"
"$root/scripts/demo/render-scenes.sh" "$work/scenes"

# Window titles: the scenes are named like macOS screenshots in each language. The English
# names have a narrow no-break space before AM and PM, as macOS writes them.
nnbsp=$(printf '\342\200\257')
title() {
  case "$1.$2" in
    chat.en) echo "Screenshot 2026-10-11 at 3.25.08${nnbsp}PM.png" ;;
    sheet.en) echo "Screenshot 2026-10-14 at 10.42.19${nnbsp}AM.png" ;;
    order.en) echo "Screenshot 2026-10-11 at 2.31.47${nnbsp}PM.png" ;;
    chat.ko) echo "스크린샷 2026-10-11 오후 3.25.08.png" ;;
    sheet.ko) echo "스크린샷 2026-10-14 오전 10.42.19.png" ;;
    order.ko) echo "스크린샷 2026-10-11 오후 2.31.47.png" ;;
    chat.ja) echo "スクリーンショット 2026-10-11 15.25.08.png" ;;
    sheet.ja) echo "スクリーンショット 2026-10-14 10.42.19.png" ;;
    order.ja) echo "スクリーンショット 2026-10-11 14.31.47.png" ;;
  esac
}

for lang in ${CANDY_DEMO_LANGS:-en ko ja}; do
  mkdir -p "$work/open/$lang"
  for scene in chat sheet order; do
    image="$work/open/$lang/$(title "$scene" "$lang")"
    cp "$work/scenes/$scene.$lang.png" "$image"
    set -- --app "$app" --image "$image" --targets "$work/scenes/$scene.$lang.json" --lang "$lang" \
      --out "$work/raw/$scene.$lang.mov"
    [ "$scene" = order ] && set -- "$@" --still "$work/raw/still.$lang.png"
    # shellcheck disable=SC2086
    "$work/record" "$@" $dry
    echo "recorded $scene.$lang"
  done
done
