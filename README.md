# Candy

[Website](https://candy.told.me/) · [한국어](https://candy.told.me/ko/)

A small image tool for your Mac. View an image in a Kitty-compatible terminal,
then open a lightweight editor when you need to change it.

```sh
candy photo.png
candy edit photo.png
```

Candy uses a native Swift/AppKit editor and a shared Rust image engine, with
macOS Image I/O and Apple Vision. Requires **macOS 14+**; currently tested on
Apple Silicon.

## Build and run

Install Rust 1.95 or newer and Xcode 16 or newer (Swift 6), then build the
application and CLI together from this repository:

```sh
scripts/build-app.sh
./target/release/candy edit photo.png
```

Install just the CLI into Cargo's bin directory (make sure it is on your PATH):

```sh
cargo install --locked --path crates/candy
```

`cargo install` does not build the Swift editor. Build `dist/Candy.app` with the
script above and open it, or copy it to `/Applications` or `~/Applications`:

```sh
open dist/Candy.app
```

The bundle includes a CLI at `Contents/Helpers/candy`. The GUI executable is
`Contents/MacOS/Candy`; they are in separate directories for case-insensitive
macOS filesystems. The CLI uses `CANDY_APP` when set, otherwise a bundle beside
or containing the executable, a repository `dist/Candy.app`, or the app
registered with LaunchServices. An invalid `CANDY_APP` is reported explicitly.

```sh
CANDY_APP=/path/to/Candy.app candy edit photo.png
```

The script builds for the current architecture; `--debug` selects a debug build.
The bundle is for local use and is not Developer ID signed or notarized.
Open images from the File menu, drag and drop, Finder Open With, or the CLI.
The bundle declares supported document types without changing your default
image application. An application icon is not included in this draft.

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

File, Edit, View and Tools actions live in the macOS menu bar. Each document
has a native window, toolbar, contextual controls, canvas, and undo/redo. Native
tabs follow your macOS preference; use Window → Merge All Windows or Move Tab
to New Window to group or separate documents.
First Save asks for a new file; subsequent saves update that copy. Save As
creates another copy. The save panel has a Format selector for PNG, JPEG and
lossless WebP. Original input paths, including
symlink and hardlink aliases, are protected. Files are encoded before an atomic
replacement, and closing unsaved documents prompts to save or discard.

| Shortcut | Action |
| --- | --- |
| Cmd+O | Open image |
| Cmd+S / Shift+Cmd+S | Save / Save as |
| Cmd+Z / Shift+Cmd+Z | Undo / Redo |
| Cmd+W / Cmd+Q | Close document / Quit |
| Cmd+0 / Cmd+9 | Actual size / Zoom to fit |
| Cmd+1 … Cmd+6 | Select, Mosaic, Highlight, Text mosaic, Resize, Flatten alpha |
| Escape | Cancel a drag or clear selection |
| Delete / Backspace | Remove the selected OCR box |
| Space + drag | Pan |
| Scroll / pinch | Zoom |

Actual Size maps one image pixel to one display backing pixel (so a 1000 px
image occupies 500 logical points on a 2× Retina display). Fit never enlarges
images beyond that size; manual zoom ranges from 1% to 1600%.

OCR boxes are locked while detection runs. Other tools remain available; editing
the image discards the old detection result. During quit confirmation all
documents are locked; cancellation restores interaction and opens queued files.

Text detection finds candidate bounds without transcribing the words. It uses
Vision's text-region detector, avoiding OCR language-model startup even on images
without text. It can miss text or mistake graphics for text. Review the detected
regions before sharing an edited image;
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
Text detection runs locally and sends no image data to a service. Detection quality and
codec availability depend on the installed macOS version.

## Development

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
scripts/build-app.sh --debug
scripts/swift.sh test
scripts/build-app.sh
```

- `candy-core`: pixel operations, edit history and atomic export.
- `candy-macos`: shared Image I/O decoding and Apple Vision text detection.
- `candy-ffi`: synchronous C ABI, owned document/image handles, bounded previews.
- `candy`: Rust CLI, Kitty graphics protocol and native-app launcher.
- `macos`: SwiftPM AppKit application, document coordination and canvas interaction.

The Swift target links the Rust engine statically. Build `candy-ffi` before running
Swift tests alone (`cargo build --locked -p candy-ffi`). `scripts/swift.sh` keeps
Swift caches in `macos/.build` and disables the nested package-manifest sandbox
so it can run inside an existing development sandbox. Native OCR and UI checks
need normal macOS access to Vision/pixel buffers and the window server.

Debug builds support a process-only appearance override for visual checks:

```sh
CANDY_TEST_APPEARANCE=dark dist/Candy.app/Contents/MacOS/Candy
```

Quit any existing test instance first. This does not change system preferences
and is not included in release builds.

Tests cover edit boundaries, alpha compositing, save protection, undo state,
terminal encoding, FFI ownership, native document/async-state regressions, all five input formats, EXIF
orientation and Korean/English text-region coverage. Synthetic fixtures can be regenerated with
`swift scripts/make-fixtures.swift crates/candy-macos/tests/fixtures`.
GitHub Actions runs the checks and produces a zipped local app bundle on macOS.

To profile text detection on synthetic blank, graphical, Korean/English and large
images (requires normal macOS Vision access):

```sh
CANDY_OCR_TIMING=1 cargo run --locked --release -p candy-macos --example ocr-timing
```

The opt-in log separates image preparation from Vision execution and reports no
image contents. Timings depend on the machine and macOS version.

MIT licensed. See [PRODUCT.md](PRODUCT.md) for scope and [DESIGN.md](DESIGN.md)
for the editor's visual conventions.

The bilingual GitHub Pages landing page is in `site/`. See [website maintenance](docs/website.md) for preview, publishing, and DNS setup.
