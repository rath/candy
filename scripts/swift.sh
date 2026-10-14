#!/bin/sh
# Keep SwiftPM caches in the checkout, including when running in a sandbox.
set -eu
candy_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
export CLANG_MODULE_CACHE_PATH="$candy_root/macos/.build/ModuleCache"
if [ -n "${CANDY_SWIFT_BUILD_SYSTEM:-}" ]; then
    set -- "$@" --build-system "$CANDY_SWIFT_BUILD_SYSTEM"
fi
exec swift "$@" --package-path "$candy_root/macos" \
    --cache-path "$candy_root/macos/.build/cache" \
    --config-path "$candy_root/macos/.build/config" \
    --security-path "$candy_root/macos/.build/security" --disable-sandbox
