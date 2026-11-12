#!/bin/sh
# Keep SwiftPM caches in the checkout, including when running in a sandbox.
set -eu
candy_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
export CLANG_MODULE_CACHE_PATH="$candy_root/macos/.build/ModuleCache"
# SwiftPM does not track archives found through -L/-l as build inputs. Put the
# archive's content hash in CCandy's compile settings so Rust-only changes force
# a new bridge object and relink both the application and test executables.
candy_target=${CARGO_TARGET_DIR:-"$candy_root/target"}
case "$candy_target" in
    /*) ;;
    *) candy_target="$candy_root/$candy_target" ;;
esac
candy_library="$candy_target/${CANDY_RUST_PROFILE:-debug}/libcandy_ffi.a"
CANDY_RUST_BUILD_ID=missing
if [ -f "$candy_library" ]; then
    candy_hash=$(shasum -a 256 < "$candy_library")
    CANDY_RUST_BUILD_ID=${candy_hash%% *}
fi
export CANDY_RUST_BUILD_ID
if [ -n "${CANDY_SWIFT_BUILD_SYSTEM:-}" ]; then
    set -- "$@" --build-system "$CANDY_SWIFT_BUILD_SYSTEM"
fi
exec swift "$@" --package-path "$candy_root/macos" \
    --cache-path "$candy_root/macos/.build/cache" \
    --config-path "$candy_root/macos/.build/config" \
    --security-path "$candy_root/macos/.build/security" --disable-sandbox
