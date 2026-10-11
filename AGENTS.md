# Working on Candy

Candy is a native macOS image utility with a Rust CLI and a Swift/AppKit editor.
Read README.md for commands and limits, PRODUCT.md for scope, and DESIGN.md for
implemented UI conventions. Keep shared pixel operations in `candy-core`,
Image I/O and Vision in `candy-macos`, and the owned C boundary in `candy-ffi`.
The `candy` crate is CLI/terminal/launcher only. AppKit UI and document coordination
live in the SwiftPM package under `macos`.

## Build and verification

- Requires macOS 14+, Rust 1.95+, and Xcode 16+ with Swift 6. Native decoding and OCR tests require macOS;
  initial interactive validation was on Apple Silicon, not Intel.
- Use `cargo fmt --all --check`,
  `cargo clippy --locked --workspace --all-targets -- -D warnings`, and
  `cargo test --locked --workspace` for relevant code changes.
- Run `scripts/build-app.sh --debug` then `scripts/swift.sh test` for Swift/FFI
  changes. The Swift wrapper keeps caches local and disables nested manifest
  sandboxing; Vision integration tests require normal macOS pixel-buffer access.
- `scripts/build-app.sh` builds the release bundle at `dist/Candy.app`;
  `--debug` selects a debug build. Rebuild the bundle after changing code before
  verifying the app, then relaunch it. The GUI executable is
  `Contents/MacOS/Candy`; the CLI is `Contents/Helpers/candy`. Never put both
  in the same directory: the names collide on case-insensitive filesystems.
- `scripts/build-app.sh` creates an ad-hoc signed local bundle. Public releases
  use `scripts/release.sh` for Developer ID signing and notarization; see
  `docs/releasing.md`. Release binaries currently support Apple Silicon only.
- The app icon is `macos/Resources/AppIcon.icns`, rendered from
  `scripts/icon/*.svg` by `scripts/build-icon.sh`. Edit the SVGs, regenerate,
  and commit both; app builds do not require `rsvg-convert`.
- Test fixtures are synthetic. Regenerate with
  `swift scripts/make-fixtures.swift crates/candy-macos/tests/fixtures`.
  Do not add personal screenshots or contact details as fixtures.

## AppKit and Rust boundary

- UI and document coordination are `@MainActor`. All decode, render, save, and
  snapshot operations run on each document's serial worker; Vision consumes an
  independent immutable image snapshot on a background queue.
- Rust document handles are not concurrently callable. Swift owners keep them
  alive through queued operations. Copy preview bytes before freeing C results;
  pair every C error, image, region, and document allocation with its destructor.
- Native menus use responder-chain targets and validation. Undo/Redo route
  through the application to either the focused NSTextView's undo manager or
  document history; NSWindow's default Undo handling otherwise intercepts them.
- Keep `NSWindowController`/DocumentSession save semantics explicit. Do not add
  autosave-in-place or NSDocument defaults that overwrite original inputs.
- Use native controls, system fonts and appearance. No bundled system fonts.
- `CANDY_TEST_APPEARANCE=dark|light` is a debug-only, process-local visual QA aid.
  It must not change global preferences or become a release setting.

## Editing and asynchronous state

- Canvas selections and edits use image pixels after EXIF orientation is applied.
  Keep screen coordinates, zoom/pan transforms, and image pixels distinct.
- The display preview is capped at 4096 pixels per side; operations and exports must
  continue to use full-resolution pixels. Keep image-size allocation checks.
- Decode, render, OCR and save work runs off the UI thread. Match results by
  document ID and revision/state; discard stale results rather than applying
  them to whichever tab happens to be active.
- Unsaved-close prompts and save-then-close continuations must retain the target
  document ID. A late file load must not change which document gets discarded.
- Lock text-region editing while OCR is running. Other tools can change pixels,
  which invalidates the detection result. Lock all document interactions during
  quit confirmation and restore them on cancellation; queue incoming file opens.
- Canceling an OCR-box drag must restore both boxes and the active-box index.
  Bounds-check selection mutation/removal; restoring only boxes caused a crash.
- Clip live brush previews to the image/canvas intersection.
- First GUI save creates a copy. Preserve original protection, including
  symlink/hardlink aliases, and atomic output writes. CLI `--force` permits
  replacing a separate output, never the input; only an explicit `--in-place`
  writes over the input, through the same atomic replacement, keeping the file's
  permissions and resolving a symlink to its target first.
- Keep alpha-aware compositing and resize behavior. Removing alpha means
  compositing onto a color, not simply dropping the alpha channel.
- Saving routes by extension through `candy_macos::save`: PNG, JPEG, WebP, GIF
  and TGA use the `image` crate in `candy-core`; HEIC, AVIF, TIFF, BMP and
  JPEG 2000 use Image I/O. Image I/O cannot read the alpha BMP the `image` crate
  writes, and AVIF encoding exists only on newer macOS, so check `can_encode`
  instead of assuming. Loading accepts whatever Image I/O decodes; when naming
  a format, update the display names, the open-panel types and the
  `LSItemContentTypes` list together.

## Terminal and native UI checks

- Verify inline graphics in a real terminal; redirected output cannot establish
  Kitty support. `--display kitty` bypasses detection, so it does not test auto.
- Inside tmux, auto detection asks tmux instead of probing: a probe reply comes
  back as keystrokes without ESC bytes, and placeholders need more than Kitty
  graphics. It requires `#{pane_tty}` to be this process's tty,
  `allow-passthrough` on or all, and a Ghostty or kitty `#{client_termtype}`;
  otherwise, and under screen, it falls back to the GUI. A standalone terminal
  launched from an agent or tmux can inherit a stale `TMUX` that names another
  pane; clear it only in that isolated test process when verifying
  direct-terminal detection. Do not change the user's global setup.
- Inside tmux, Kitty output must be passthrough plus Unicode placeholders; bare
  graphics commands become the pane title. Placeholder image IDs avoid colour
  indices 0–15, which tmux replaces with `pane-colours`. Verify in a real
  terminal attached to an isolated server (`tmux -L <name>`) using the user's
  tmux binary and config. Attach before sending the image (output during attach
  can be lost), and remember that a detached server has no outer terminal.
- Capability probing temporarily changes tty mode and signal masks. Preserve
  the RAII restoration on success, timeout and errors.
- Before native mouse automation, read the current window bounds and target PID
  or window ID. Users can move windows between actions. macOS window coordinates
  are logical points; Retina screenshots can contain twice as many pixels.
- Capture only the test application's window. Store local captures under
  `.impeccable/review/` (ignored), and verify that each capture shows the intended
  window/state before treating it as evidence.
- If browser work is needed, use the installed `agent-browser` CLI with
  `--headed --session candy` and prefer its `snapshot` output.
- System Events can inspect native menus, toolbars, controls, and sheets. Use
  accessible controls where possible; read current bounds before canvas clicks.
  Native sheets may have their own window IDs: capturing only the parent can
  omit the sheet. Verify every image shows the intended state.

## Commits

Commit logical units separately. Use English Conventional Commits with a blank
line after the title and a body containing specific change bullets. Do not add
AI metadata. Invoke `git commit` directly with ordinary `-m` arguments or `-F`;
do not use ANSI-C shell quoting. Do not push unless requested, and do not ask
whether to push.
