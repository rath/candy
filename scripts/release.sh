#!/bin/bash
# Build and notarize on the maintainer's Mac. Publishing is a separate step.
set -euo pipefail
cd "$(dirname "$0")/.."

fail() { echo "$*" >&2; exit 1; }
[[ $# == 0 ]] || fail "Usage: NOTARY_PROFILE=<keychain-profile> scripts/release.sh"
[[ -n "${NOTARY_PROFILE:-}" ]] || fail "Set NOTARY_PROFILE to an existing notarytool keychain profile; see docs/releasing.md."
[[ -z "$(git status --porcelain)" ]] || fail "Release from a clean, committed working tree."
[[ "$(uname -m)" == arm64 ]] || fail "Release binaries currently support Apple Silicon only."
version=$(cargo pkgid -p candy)
version=${version##*#}
version=${version##*@}
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "Expected a stable X.Y.Z version."
if tagged=$(git rev-parse -q --verify "refs/tags/v$version^{commit}"); then
  [[ "$tagged" == "$(git rev-parse HEAD)" ]] || fail "v$version already points to a different commit."
fi

# Only identity metadata is read. Credentials stay in the login keychain.
identity=${SIGNING_IDENTITY:-}
if [[ -z "$identity" ]]; then
  identities=$(security find-identity -v -p codesigning | awk '/"Developer ID Application: / {print $2}')
  [[ -n "$identities" && "$(wc -w <<<"$identities" | tr -d ' ')" == 1 ]] || fail "Set SIGNING_IDENTITY to a Developer ID Application certificate hash."
  identity=$identities
fi
xcrun notarytool history --keychain-profile "$NOTARY_PROFILE" --output-format json >/dev/null

cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
scripts/build-app.sh --debug
scripts/swift.sh test
scripts/build-app.sh

out="$PWD/dist/releases/$version"
app="$out/Candy.app"
archive="Candy-$version-macos-arm64.zip"
mkdir -p "$out"
rm -rf "$app"
ditto dist/Candy.app "$app"
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist")" == "$version" ]] || fail "App version mismatch."
[[ "$("$app/Contents/Helpers/candy" --version)" == "candy $version" ]] || fail "CLI version mismatch."
for executable in "$app/Contents/Helpers/candy" "$app/Contents/MacOS/Candy"; do
  [[ "$(lipo -archs "$executable")" == arm64 ]] || fail "Unexpected architecture: $executable"
  rpaths=$(otool -l "$executable" | sed -n 's/^ *path \(.*\) (offset [0-9]*)$/\1/p' | grep -Ev '^(@|/usr/lib/|/System/)' || true)
  [[ -z "$rpaths" ]] || fail "Build-machine rpath in $executable: $rpaths"
  codesign --force --sign "$identity" --options runtime --timestamp "$executable"
done
codesign --force --sign "$identity" --options runtime --timestamp "$app"
codesign --verify --deep --strict "$app"
for executable in "$app" "$app/Contents/Helpers/candy"; do
  signature=$(codesign -dvv "$executable" 2>&1)
  grep -q '^Authority=Developer ID Application: ' <<<"$signature" || fail "Missing Developer ID signature."
  grep -q '^CodeDirectory .*flags=.*runtime' <<<"$signature" || fail "Missing hardened runtime."
done

ditto -c -k --keepParent "$app" "$out/notarize.zip"
xcrun notarytool submit "$out/notarize.zip" --keychain-profile "$NOTARY_PROFILE" \
  --wait --output-format json >"$out/notarization.json"
status=$(plutil -extract status raw -o - "$out/notarization.json")
if [[ "$status" != Accepted ]]; then
  submission=$(plutil -extract id raw -o - "$out/notarization.json")
  xcrun notarytool log "$submission" --keychain-profile "$NOTARY_PROFILE" "$out/notarization-log.json"
  fail "Notarization: $status. See $out/notarization-log.json."
fi
xcrun stapler staple "$app"
xcrun stapler validate "$app"
spctl --assess --type execute --verbose "$app"
rm "$out/notarize.zip"
ditto -c -k --sequesterRsrc --keepParent "$app" "$out/$archive"
(cd "$out" && shasum -a 256 "$archive" >SHA256SUMS)
printf '\nRelease ready: %s/%s\nCommit: %s\n' "$out" "$archive" "$(git rev-parse HEAD)"
cat "$out/SHA256SUMS"
