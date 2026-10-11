#!/bin/sh
# Outline the website wordmark from Fredoka so pages need no web font request.
set -eu
candy_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
if [ $# -ne 1 ] || [ ! -f "$1" ]; then
    echo 'Usage: scripts/build-wordmark.sh path/to/Fredoka[wdth,wght].ttf' >&2
    echo 'Fredoka (OFL): https://github.com/google/fonts/tree/main/ofl/fredoka' >&2
    exit 2
fi
if ! command -v hb-view >/dev/null; then
    echo 'hb-view is required: brew install harfbuzz' >&2
    exit 1
fi
candy_tmp=$(mktemp -d "${TMPDIR:-/tmp}/candy-wordmark.XXXXXX")
trap 'rm -rf "$candy_tmp"' EXIT
trap 'exit 1' HUP INT TERM

# A 1000 px size equals Fredoka's 1000-unit em, so SVG coordinates are font units.
hb-view --font-file="$1" --variations=wght=600,wdth=100 --font-size=1000 --margin=0 \
    --background='#00000000' --output-format=svg --output-file="$candy_tmp/text.svg" Candy
python3 -I - "$candy_tmp/text.svg" "$candy_root/site/assets/wordmark.svg" <<'PY'
import re
import sys
import xml.etree.ElementTree as ET

TRACKING = -10  # letter-spacing: -0.01em, applied after every glyph as in CSS.
SVG = "{http://www.w3.org/2000/svg}"
HREF = "{http://www.w3.org/1999/xlink}href"
root = ET.parse(sys.argv[1]).getroot()
_, _, width, height = (float(v) for v in root.get("viewBox").split())
glyphs = {g.get("id"): g.find(f"{SVG}path").get("d") for g in root.iter(f"{SVG}g") if g.get("id")}
uses = list(root.iter(f"{SVG}use"))


def number(value):
    text = f"{value:.1f}".rstrip("0").rstrip(".")
    return "0" if text == "-0" else text


parts = []
for index, use in enumerate(uses):
    dx = float(use.get("x")) + index * TRACKING
    dy = float(use.get("y"))
    tokens = re.findall(r"[A-Za-z]|-?[\d.]+(?:e-?\d+)?", glyphs[use.get(HREF)[1:]])
    if not set(t for t in tokens if t.isalpha()) <= {"M", "L", "C", "Z"}:
        raise SystemExit("Unexpected path command from hb-view")
    values = iter(tokens)
    for token in values:
        if token.isalpha():
            parts.append(token)
        else:
            parts.append(f"{number(float(token) + dx)} {number(float(next(values)) + dy)}")
path = re.sub(r" ?([A-Z]) ?", r"\1", " ".join(parts))
width += len(uses) * TRACKING
with open(sys.argv[2], "w", encoding="utf-8") as out:
    out.write(
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {number(width)} {number(height)}">\n'
        '<!-- "Candy" in Fredoka SemiBold (wght 600, wdth 100, SIL Open Font License 1.1), '
        "tracked -0.01em and outlined by scripts/build-wordmark.sh. -->\n"
        f'<path fill="#242b35" d="{path}"/>\n</svg>\n')
PY
printf 'Built %s/site/assets/wordmark.svg\n' "$candy_root"
