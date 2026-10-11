#!/bin/sh
# Render localized social cards with the repository's visible browser session.
set -eu
candy_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
command -v python3 >/dev/null
command -v agent-browser >/dev/null
candy_tmp=$(mktemp -d "${TMPDIR:-/tmp}/candy-og.XXXXXX")
candy_server_pid=''
cleanup() {
    if [ -n "$candy_server_pid" ]; then
        kill "$candy_server_pid" 2>/dev/null || true
        wait "$candy_server_pid" 2>/dev/null || true
    fi
    rm -rf "$candy_tmp"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

# An override can reuse a running repository-root preview. Otherwise start an
# isolated loopback server on a free port and stop only that server on exit.
candy_preview_url=${CANDY_PREVIEW_URL:-}
if [ -z "$candy_preview_url" ]; then
    python3 - "$candy_root" "$candy_tmp/url" >"$candy_tmp/server.log" 2>&1 <<'PY' &
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import sys
handler = partial(SimpleHTTPRequestHandler, directory=sys.argv[1])
with ThreadingHTTPServer(("127.0.0.1", 0), handler) as server:
    Path(sys.argv[2]).write_text(f"http://127.0.0.1:{server.server_port}")
    server.serve_forever()
PY
    candy_server_pid=$!
    candy_attempt=0
    while [ ! -s "$candy_tmp/url" ]; do
        if ! kill -0 "$candy_server_pid" 2>/dev/null || [ "$candy_attempt" -ge 50 ]; then
            cat "$candy_tmp/server.log" >&2
            echo 'Could not start the OG preview server.' >&2
            exit 1
        fi
        candy_attempt=$((candy_attempt + 1))
        sleep 0.1
    done
    candy_preview_url=$(cat "$candy_tmp/url")
fi

for candy_language in en ko; do
    agent-browser --headed --session candy open "${candy_preview_url%/}/scripts/og/?lang=$candy_language"
    agent-browser --headed --session candy set viewport 1200 630
    agent-browser --headed --session candy eval \
        'document.fonts.ready.then(() => Promise.all(Array.from(document.images, image => image.decode())))' >/dev/null
    agent-browser --headed --session candy screenshot '#card' "$candy_tmp/og-$candy_language.png"
done

# Validate both renders before replacing either checked-in asset. A browser
# session with an unexpected device scale must not silently ship wrong sizes.
python3 - "$candy_tmp" <<'PY'
from pathlib import Path
import struct
import sys
for language in ("en", "ko"):
    path = Path(sys.argv[1]) / f"og-{language}.png"
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n" or struct.unpack(">II", data[16:24]) != (1200, 630):
        raise SystemExit(f"Expected a 1200 x 630 PNG: {path}")
    print(f"Verified {path.name}: 1200 x 630, {len(data):,} bytes")
PY
for candy_language in en ko; do
    cp "$candy_tmp/og-$candy_language.png" "$candy_root/site/assets/og-$candy_language.png"
done
printf 'Built %s/site/assets/og-{en,ko}.png\n' "$candy_root"
