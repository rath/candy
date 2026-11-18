#!/bin/sh
# Render every demo scene (scripts/demo/scenes/*.html, 820 x 560 CSS pixels) at 2x into OUT_DIR as
# <scene>.<lang>.png (1640 x 1120), with <scene>.<lang>.json from targets.js beside it. Uses a
# headless agent-browser session, so no window opens.
set -eu
out=${1:?usage: scripts/demo/render-scenes.sh OUT_DIR}
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$out"
# The browser daemon has its own working directory: hand it absolute paths.
out=$(cd "$out" && pwd)
session=candy-demo-render
browse() { agent-browser --session "$session" "$@"; }
trap 'browse close >/dev/null 2>&1 || true' EXIT

for scene in chat sheet order; do
  for lang in en ko ja; do
    browse set viewport 820 560 2 >/dev/null
    browse open "file://$here/scenes/$scene.$lang.html" >/dev/null
    browse eval 'document.fonts.ready.then(() => true)' >/dev/null
    browse screenshot "$out/$scene.$lang.png" >/dev/null
    browse eval "$(cat "$here/targets.js")" >"$out/$scene.$lang.json"
    size=$(sips -g pixelWidth -g pixelHeight "$out/$scene.$lang.png" | awk '/pixel/ { printf "%s ", $2 }')
    [ "$size" = "1640 1120 " ] || { echo "error: $scene.$lang.png is $size, expected 1640 1120" >&2; exit 1; }
    echo "rendered $scene.$lang"
  done
done
