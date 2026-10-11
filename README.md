# Candy

A small image tool for your Mac. View an image in a Kitty-compatible terminal,
then open a lightweight editor when you need to change it.

```sh
candy photo.png
candy edit photo.png
```

Candy is an early Rust implementation using egui/eframe, macOS Image I/O and
Apple Vision. Requires **macOS 14+**; currently tested on Apple Silicon.

## Build and run

Install Rust 1.95 or newer and the Xcode command-line tools, then run from this repository:

```sh
cargo build --locked --release
./target/release/candy edit photo.png
```

Install the CLI into Cargo's bin directory (make sure it is on your PATH):

```sh
cargo install --locked --path crates/candy
```

Build a local application bundle:

```sh
scripts/build-app.sh
open dist/Candy.app
```

The script builds for the current architecture; `--debug` selects a debug build.
The bundle is for local use and is not Developer ID signed or notarized.
Open images with the editor's Open button, drag and drop, or the CLI. Finder
file associations and an application icon are not included in this draft.

## Terminal workflow

`candy photo.png` probes Kitty graphics support and displays a fitted preview
when available. Otherwise it launches the editor. `candy edit photo.png`
launches a detached editor and returns control to the shell immediately.
Running `candy` without arguments opens an empty editor.

```sh
candy --display gui photo.heic
candy --display kitty photo.avif
```

`--display kitty` skips capability detection. Automatic detection falls back
to the GUI inside tmux/screen; terminal passthrough and the iTerm image protocol
are not implemented. Redirected image-display output is rejected rather than
writing escape sequences into a file. The automatic path has been checked in Ghostty.

## Editing

- **Select:** drag a rectangle, move it or resize its corners, then extract it into a new tab.
- **Mosaic:** pixelate the selected rectangle with an adjustable block size.
- **Highlight:** draw translucent strokes with adjustable color, size and opacity.
- **Text mosaic:** detect text locally with Apple Vision, review or adjust boxes, then apply mosaic. Korean and English are supported.
- **Resize:** set dimensions, optionally preserving the aspect ratio.
- **Flatten alpha:** composite transparency onto a chosen solid color.

Each document has undo/redo. First Save asks for a new file; subsequent saves
update that copy. Save as creates another copy. Original input paths, including
symlink and hardlink aliases, are protected. Files are encoded before an atomic
replacement, and closing unsaved documents prompts to save or discard.

| Shortcut | Action |
| --- | --- |
| Cmd+O | Open image |
| Cmd+S / Shift+Cmd+S | Save / Save as |
| Cmd+Z / Shift+Cmd+Z | Undo / Redo |
| Cmd+W / Cmd+Q | Close tab / Quit |
| Escape | Cancel a drag or clear selection |
| Delete / Backspace | Remove the selected OCR box |
| Space + drag | Pan |
| Scroll / pinch | Zoom |

OCR can miss text. Review the detected regions before sharing an edited image;
mosaic is a visual effect, not a guarantee of irreversible redaction.

## CLI edits

All editing commands require a separate output path. Rectangle coordinates are
`x,y,width,height` in image pixels, measured from the top-left after EXIF rotation.

```sh
candy resize photo.heic --width 1200 -o smaller.png
candy crop photo.png --rect 20,30,300,200 -o crop.png
candy mosaic photo.png --rect 20,30,300,200 --block-size 16 -o mosaic.png
candy mosaic screenshot.png --text -o text-mosaic.png
candy flatten logo.png --background '#ffffff' -o flat.jpg
```

One resize dimension preserves the aspect ratio; providing both sets the exact
size. Use `--force` to replace an existing output; it never permits overwriting
the input. Run `candy --help` or `candy <command> --help` for details.

## Formats and current limits

| Operation | Formats |
| --- | --- |
| Open | PNG, JPEG, WebP, HEIC, AVIF |
| Save | PNG, JPEG (quality 92), lossless WebP |

Images are edited as 8-bit sRGB RGBA. EXIF orientation is applied on load;
source metadata, HDR data and wide-gamut profiles are not retained on export.
JPEG exports flatten transparency onto white. Only the first image/frame is
loaded. Images are limited to approximately 64 megapixels. The GUI preview is
limited to 4096 pixels per side; edits and saves use full image resolution.

This version has no layers, animation editing, cloud services or LLM integration.
OCR runs locally and sends no image data to a service. Detection quality and
codec availability depend on the installed macOS version.

## Development

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
scripts/build-app.sh
```

- `candy-core`: pixel operations, edit history and atomic export.
- `candy-macos`: Image I/O decoding and Apple Vision OCR.
- `candy`: CLI, Kitty graphics protocol and egui editor.

Tests cover edit boundaries, alpha compositing, save protection, undo state,
terminal encoding, GUI async-state regressions, all five input formats, EXIF
orientation and Korean/English OCR. Synthetic fixtures can be regenerated with
`swift scripts/make-fixtures.swift crates/candy-macos/tests/fixtures`.
GitHub Actions runs the checks and produces a zipped local app bundle on macOS.

MIT licensed. See [PRODUCT.md](PRODUCT.md) for scope and [DESIGN.md](DESIGN.md)
for the editor's visual conventions.
