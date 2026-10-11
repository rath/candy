# Working on Candy

Candy is a native macOS image utility with a Rust CLI and an egui/eframe editor.
Read README.md for commands and limits, PRODUCT.md for scope, and DESIGN.md for
implemented UI conventions. Keep shared pixel operations in `candy-core`, native
Image I/O and Vision integration in `candy-macos`, and CLI/GUI code in `candy`.

## Build and verification

- Requires macOS 14+ and Rust 1.95+. Native decoding and OCR tests require macOS;
  initial interactive validation was on Apple Silicon, not Intel.
- Use `cargo fmt --all --check`,
  `cargo clippy --locked --workspace --all-targets -- -D warnings`, and
  `cargo test --locked --workspace` for relevant code changes.
- `scripts/build-app.sh` builds the release bundle at `dist/Candy.app`;
  `--debug` selects a debug build. Rebuild the bundle after changing code before
  verifying the app: an existing bundle does not track `target/debug/candy`.
- The bundle is a local build, without Developer ID signing or notarization.
- Test fixtures are synthetic. Regenerate with
  `swift scripts/make-fixtures.swift crates/candy-macos/tests/fixtures`.
  Do not add personal screenshots or contact details as fixtures.

## egui/eframe API details

The project uses 0.36.2. Older examples often use incompatible APIs:

- Implement `eframe::App::ui(&mut self, &mut egui::Ui, &mut eframe::Frame)`,
  not the older `update` callback.
- Use `egui::Panel::top/bottom(...).show(ui, ...)`, not `TopBottomPanel` or
  panel calls taking a Context.
- Keyboard focus is queried with `ctx.egui_wants_keyboard_input()`.
- Dropped-file paths use `file.path().to_path_buf()`.
- Consume Shift+Cmd shortcuts before their plain Cmd variants. egui's logical
  matching can accept extra Shift, so the reverse order steals Save as/Redo.
- The Korean fallback font is loaded from macOS at runtime. Do not commit the
  system font file or assume egui's default fonts cover Korean.

## Editing and asynchronous state

- Canvas selections and edits use image pixels after EXIF orientation is applied.
  Keep screen coordinates, zoom/pan transforms, and image pixels distinct.
- The GPU preview is capped at 4096 pixels per side; operations and exports must
  continue to use full-resolution pixels. Keep image-size allocation checks.
- Decode, render, OCR and save work runs off the UI thread. Match results by
  document ID and revision/state; discard stale results rather than applying
  them to whichever tab happens to be active.
- Unsaved-close prompts and save-then-close continuations must retain the target
  document ID. A late file load must not change which document gets discarded.
- Canceling an OCR-box drag must restore both boxes and the active-box index.
  Bounds-check selection mutation/removal; restoring only boxes caused a crash.
- Clip live brush previews to the image/canvas intersection.
- First GUI save creates a copy. Preserve original protection, including
  symlink/hardlink aliases, and atomic output writes. CLI `--force` permits
  replacing a separate output, never the input.
- Keep alpha-aware compositing and resize behavior. Removing alpha means
  compositing onto a color, not simply dropping the alpha channel.

## Terminal and native UI checks

- Verify inline graphics in a real terminal; redirected output cannot establish
  Kitty support. `--display kitty` bypasses detection, so it does not test auto.
- Auto detection intentionally falls back to GUI when `TMUX` is set or TERM
  identifies tmux/screen. A standalone terminal launched from an agent or tmux
  can inherit a stale `TMUX`; clear it only in that isolated test process when
  verifying direct-terminal detection. Do not change the user's global setup.
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

## Commits

Commit logical units separately. Use English Conventional Commits with a blank
line after the title and a body containing specific change bullets. Do not add
AI metadata. Invoke `git commit` directly with ordinary `-m` arguments or `-F`;
do not use ANSI-C shell quoting. Do not push unless requested, and do not ask
whether to push.
