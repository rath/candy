# Releasing Candy

Releases currently target **Apple Silicon on macOS 14+**. Build on an Apple
Silicon Mac with Rust 1.95+ and Xcode 16+ (Swift 6). CI checks source builds;
the maintainer's Mac signs and notarizes public release artifacts.

## One-time authentication

Install a Developer ID Application certificate and its private key in the login
keychain. `security find-identity -v -p codesigning` should list it. A development
or App Store distribution certificate is not a substitute.

Reuse an existing notarytool profile for the same Apple developer team. The
maintainer currently uses `vimdow-notary`, shared with Vimdow. Profile names
are local references, not passwords, and need not match the application name.

To set up a new profile, create an app-specific password at account.apple.com
and run this interactively in your own terminal:

```sh
xcrun notarytool store-credentials candy-notary
```

Enter the Apple ID, developer Team ID, and app-specific password when prompted.
Do not commit credentials or put passwords in command arguments or chat.
Check the stored profile with:

```sh
xcrun notarytool history --keychain-profile candy-notary
```

## Build and verify

1. Update `workspace.package.version` in `Cargo.toml` and refresh `Cargo.lock`.
   The app's marketing and build versions derive from the CLI package version.
2. Write `docs/releases/<version>.md` and commit all release changes.
3. Run from a clean working tree:

   ```sh
   NOTARY_PROFILE=vimdow-notary scripts/release.sh
   ```

   If multiple Developer ID certificates exist, also set `SIGNING_IDENTITY`
   to the chosen certificate hash. The script runs Rust and Swift checks, builds
   the app, signs the CLI and GUI with hardened runtime and secure timestamps,
   submits to Apple, staples the accepted ticket, and verifies Gatekeeper.
   Keychain and native Vision access require a normal macOS session.
4. Outputs are in `dist/releases/<version>/`: the signed `Candy.app`,
   `Candy-<version>-macos-arm64.zip`, `SHA256SUMS`, and Apple's submission result.
   Only the ZIP and checksum file are published. The ordinary `dist/Candy.app`
   remains an ad-hoc signed development build.
5. Unzip the final ZIP into a temporary directory and verify its signature and
   ticket. Test launch, CLI version, an image edit, and text detection. Check
   `otool -L` on both executables for dependencies outside system libraries.
6. Check the size claims on the website against this build: the ZIP should
   still be about 2 MB (`ls -l` the ZIP), the app under 4 MB (`du -sh`), and
   an empty window about 30 MB of memory (`footprint -p <pid>`, the
   `phys_footprint` line; Activity Monitor's Memory column shows the same
   value). Update `site/index.html` and `site/ko/index.html` if a claim no
   longer holds.

## Publish

Tag the exact commit used to build the verified artifact. Never replace assets
of an already published version; ship a new patch version instead.

```sh
git tag -a v0.1.0 -m "Candy 0.1.0"
git push origin main v0.1.0
gh release create v0.1.0 --verify-tag --title "Candy 0.1.0" \
  --notes-file docs/releases/0.1.0.md \
  dist/releases/0.1.0/Candy-0.1.0-macos-arm64.zip \
  dist/releases/0.1.0/SHA256SUMS
```

In `rath/homebrew-tap`, update `Casks/candy.rb` with the version and ZIP's SHA-256.
The Cask installs `Candy.app` and links `Contents/Helpers/candy` into Homebrew's
bin directory. Keep the ARM architecture and macOS Sonoma requirements.
Run Cask style/audit and installation checks, commit, and push. Its CI verifies
the packaged version, architecture, signature, notarization and a CLI edit.

Once the release is public, point the website's download links at the new ZIP:
the hero button, the installation link and the JSON-LD `downloadUrl` in both
`site/index.html` and `site/ko/index.html` use
`https://github.com/rath/candy/releases/download/v<version>/Candy-<version>-macos-arm64.zip`
so the button downloads directly. Change them only after `gh release create`;
an earlier push would publish a link to a missing asset.

Update both website languages and README installation instructions when needed.
Pushing `main` publishes committed website changes (see
[website maintenance](website.md)). Finally verify the public release assets, tap
installation, and https://candy.told.me/ in both languages.
